use crate::{config::Settings, content::Segment};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Debug)]
pub struct RetryableEmbedding(pub String);
impl std::fmt::Display for RetryableEmbedding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for RetryableEmbedding {}
pub fn retryable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<RetryableEmbedding>().is_some()
}
fn http_runtime() -> Result<&'static tokio::runtime::Runtime> {
    static RUNTIME: OnceLock<std::result::Result<tokio::runtime::Runtime, String>> =
        OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("VectorDash HTTP")
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))
}
#[derive(Clone)]
pub struct Models {
    inner: Arc<Mutex<State>>,
    agent: ureq::Agent,
    http: Arc<OnceLock<std::result::Result<reqwest::Client, String>>>,
    pub query_generation: Arc<AtomicU64>,
    interactive: Arc<AtomicUsize>,
    pub loaded: Arc<AtomicBool>,
    pub loading: Arc<AtomicBool>,
    pub session_override: Arc<AtomicBool>,
    load_phase: Arc<Mutex<String>>,
    active: Arc<AtomicUsize>,
    query_cache: Arc<Mutex<Option<(String, Instant, Vec<f32>)>>>,
    media_single: Arc<Mutex<std::collections::HashSet<String>>>,
}
struct QueryPriority<'a>(&'a AtomicUsize);
impl Drop for QueryPriority<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Activity<'a>(&'a Models);
impl Drop for Activity<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.inner.lock() {
            state.last_used = Instant::now();
        }
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}
struct State {
    child: Option<Child>,
    job: Option<crate::platform::Job>,
    configuration: String,
    media: bool,
    last_used: Instant,
    marker: Option<(String, String)>,
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Default for Models {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(State {
                child: None,
                job: None,
                configuration: String::new(),
                media: false,
                last_used: Instant::now(),
                marker: None,
            })),
            agent: ureq::Agent::config_builder()
                .timeout_connect(Some(Duration::from_millis(500)))
                .timeout_global(Some(Duration::from_secs(1)))
                .max_redirects(0)
                .http_status_as_error(false)
                .build()
                .into(),
            http: Arc::new(OnceLock::new()),
            query_generation: Arc::new(AtomicU64::new(0)),
            interactive: Arc::new(AtomicUsize::new(0)),
            loaded: Arc::new(AtomicBool::new(false)),
            loading: Arc::new(AtomicBool::new(false)),
            session_override: Arc::new(AtomicBool::new(false)),
            load_phase: Arc::new(Mutex::new(String::new())),
            active: Arc::new(AtomicUsize::new(0)),
            query_cache: Arc::new(Mutex::new(None)),
            media_single: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }
}
impl Models {
    pub fn url(s: &Settings) -> String {
        if !s.remote_embeddings() {
            "http://127.0.0.1:8788".into()
        } else {
            s.remote_url
                .trim()
                .trim_end_matches('/')
                .trim_end_matches("/v1")
                .into()
        }
    }
    pub fn stop(&self) -> Result<()> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Model state unavailable"))?;
        if let Some(mut child) = state.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        state.job = None;
        state.media = false;
        state.marker = None;
        self.loaded.store(false, Ordering::Relaxed);
        self.loading.store(false, Ordering::Relaxed);
        drop(state);
        if let Ok(mut cache) = self.query_cache.try_lock() {
            *cache = None;
        }
        Ok(())
    }
    pub fn start(&self, s: &Settings, media: bool) -> Result<()> {
        let _activity = self.activity();
        self.ensure(s, media)
    }
    pub fn pid(&self) -> u32 {
        self.inner
            .try_lock()
            .ok()
            .and_then(|s| s.child.as_ref().map(|c| c.id()))
            .unwrap_or(0)
    }
    pub fn media_loaded(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
            && self.inner.try_lock().map(|s| s.media).unwrap_or(false)
    }
    pub fn idle_unload(&self, s: &Settings) -> bool {
        self.unload_with_metrics(s, crate::scheduling::metrics())
    }
    fn unload_with_metrics(&self, s: &Settings, metrics: crate::scheduling::Metrics) -> bool {
        if self.session_override.load(Ordering::Acquire) || crate::dictation::in_use() {
            return false;
        }
        let resource_hold = crate::scheduling::model_hold(s, metrics).is_some();
        if !s.unload_idle && !resource_hold {
            return false;
        }
        if let Ok(mut state) = self.inner.try_lock() {
            if self.active.load(Ordering::Acquire) == 0
                && state.child.is_some()
                && (resource_hold
                    || (s.unload_idle
                        && state.last_used.elapsed()
                            > Duration::from_secs(s.unload_minutes.max(1) as u64 * 60)))
            {
                if let Some(mut child) = state.child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                state.job = None;
                state.media = false;
                state.marker = None;
                self.loaded.store(false, Ordering::Relaxed);
                return true;
            }
        }
        false
    }
    fn activity(&self) -> Activity<'_> {
        self.active.fetch_add(1, Ordering::AcqRel);
        Activity(self)
    }
    pub fn loading_phase(&self) -> String {
        self.load_phase.lock().unwrap().clone()
    }
    pub fn search_hold(&self, s: &Settings) -> Option<&'static str> {
        if s.remote_embeddings() || self.session_override.load(Ordering::Acquire) {
            None
        } else {
            crate::scheduling::model_hold(s, crate::scheduling::metrics())
        }
    }
    fn ensure(&self, s: &Settings, media: bool) -> Result<()> {
        self.ensure_checked(s, media, &|| Ok(()))
    }
    fn ensure_checked(
        &self,
        s: &Settings,
        media: bool,
        checkpoint: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        checkpoint()?;
        if !!s.remote_embeddings() {
            return Ok(());
        }
        if !self.loaded.load(Ordering::Acquire) {
            self.loading.store(true, Ordering::Release);
            *self.load_phase.lock().unwrap() = "Starting local llama-server…".into();
        }
        let result = self.ensure_inner(s, media, checkpoint);
        self.loading.store(false, Ordering::Release);
        if let Err(error) = &result {
            *self.load_phase.lock().unwrap() = error.to_string();
        }
        result
    }
    fn ensure_inner(
        &self,
        s: &Settings,
        media: bool,
        checkpoint: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Model state unavailable"))?;
        state.last_used = Instant::now();
        if !!s.remote_embeddings() {
            return Ok(());
        }
        anyhow::ensure!(
            s.runtime_compatible(),
            "The installed local runtime has a different backend. Select it in setup and download the chosen CUDA/Vulkan runtime first."
        );
        let dead = if let Some(c) = state.child.as_mut() {
            c.try_wait()?.is_some()
        } else {
            true
        };
        let configuration = format!(
            "{}|{}|{}|{}|{}",
            s.server, s.model, s.projector, s.hardware, s.device
        );
        if !dead && state.configuration == configuration && (!media || state.media) {
            if self.loaded.load(Ordering::Relaxed) {
                return Ok(());
            }
            drop(state);
            return self.wait_ready_at_checked(&Self::url(s), checkpoint);
        }

        if let Some(mut c) = state.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.loaded.store(false, Ordering::Release);
        state.job = None;
        state.media = false;
        state.marker = None;
        anyhow::ensure!(
            !s.server.is_empty() && !s.model.is_empty(),
            "Set up or select llama-server and EmbeddingGemma 2 in Models and AI. Names search does not need a model."
        );
        self.loaded.store(false, Ordering::Release);
        self.loading.store(true, Ordering::Release);
        *self.load_phase.lock().unwrap() = "Starting local llama-server…".into();
        let mut cmd = Command::new(&s.server);
        cmd.args([
            "-m",
            &s.model,
            "--embedding",
            "--host",
            "127.0.0.1",
            "--port",
            "8788",
            "--ctx-size",
            "2048",
            "--batch-size",
            "2048",
            "--ubatch-size",
            "2048",
            "--parallel",
            "2",
            "--pooling",
            "mean",
        ]);
        if s.hardware == "CPU / system RAM" {
            cmd.args(["--device", "none", "--gpu-layers", "0"]);
        } else {
            anyhow::ensure!(
                !s.device.is_empty(),
                "Select your GPU device before full indexing"
            );
            cmd.args(["--device", &s.device, "--gpu-layers", "all"]);
        }
        if media {
            anyhow::ensure!(
                !s.projector.is_empty(),
                "Image/audio/video indexing needs the matching mmproj encoder"
            );
            cmd.args([
                "--mmproj",
                &s.projector,
                "--image-min-tokens",
                "70",
                "--image-max-tokens",
                "280",
            ]);
        }
        let log = std::env::var("VECTORDASH_MODEL_LOG")
            .ok()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("vectordash-rust-model.log"));
        let out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)?;
        cmd.stdout(Stdio::from(out.try_clone()?))
            .stderr(Stdio::from(out));
        crate::content::hidden(&mut cmd);
        let mut child = cmd.spawn()?;
        let job = match crate::platform::owned_process(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        state.job = Some(job);
        state.configuration = configuration;
        state.child = Some(child);
        state.media = media;
        state.marker = None;
        drop(state);
        self.wait_ready_at_checked(&Self::url(s), checkpoint)
    }
    #[cfg(test)]
    fn wait_ready_at(&self, url: &str) -> Result<()> {
        self.wait_ready_at_checked(url, &|| Ok(()))
    }
    fn wait_ready_at_checked(&self, url: &str, checkpoint: &dyn Fn() -> Result<()>) -> Result<()> {
        *self.load_phase.lock().unwrap() =
            "Loading EmbeddingGemma 2 · waiting for server readiness…".into();
        let start = Instant::now();
        loop {
            checkpoint()?;
            let alive = {
                let mut state = self
                    .inner
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Model state unavailable"))?;
                state
                    .child
                    .as_mut()
                    .is_some_and(|c| c.try_wait().ok().flatten().is_none())
            };
            anyhow::ensure!(alive, "Local model stopped while loading; check Model logs");
            if let Ok(r) = self.agent.get(format!("{url}/health")).call() {
                if r.status().as_u16() == 200 {
                    self.loaded.store(true, Ordering::Relaxed);
                    self.loading.store(false, Ordering::Release);
                    *self.load_phase.lock().unwrap() =
                        "EmbeddingGemma 2 and llama-server are ready".into();
                    return Ok(());
                }
            }
            if start.elapsed() > Duration::from_secs(90) {
                self.stop()?;
                anyhow::bail!("Model loading timed out");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    pub fn cancel_query(&self) {
        self.query_generation.fetch_add(1, Ordering::AcqRel);
    }
    fn post(&self, s: &Settings, input: Value) -> Result<Vec<Vec<f32>>> {
        self.post_checked(s, input, &|| Ok(()))
    }
    fn post_checked(
        &self,
        s: &Settings,
        input: Value,
        checkpoint: &dyn Fn() -> Result<()>,
    ) -> Result<Vec<Vec<f32>>> {
        checkpoint()?;
        let url = Self::url(s);
        let endpoint = crate::remote::api(&url, "embeddings")?;
        let runtime = http_runtime()?;
        let client = self
            .http
            .get_or_init(|| {
                reqwest::Client::builder()
                    .connect_timeout(Duration::from_secs(3))
                    .timeout(Duration::from_secs(90))
                    .redirect(reqwest::redirect::Policy::none())
                    .pool_max_idle_per_host(2)
                    .pool_idle_timeout(Duration::from_secs(15))
                    .build()
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(|e| anyhow::anyhow!(e.clone()))?;
        let payload = if s.remote_embeddings() && !s.remote_model.trim().is_empty() {
            json!({"model":s.remote_model,"input":input})
        } else {
            json!({"input":input})
        };
        let mut attempt = 0;
        let (status, body) = loop {
            let result=runtime.block_on(async{
                let mut request=client.post(&endpoint).json(&payload);
                if s.remote_embeddings() && !s.remote_key.is_empty(){request=request.bearer_auth(&s.remote_key);}
                let send=request.send();tokio::pin!(send);
                let mut response=loop{tokio::select!{
                    response=&mut send=>break response.map_err(|e|RetryableEmbedding(format!("Embedding transport unavailable: {}",e.without_url())))?,
                    _=tokio::time::sleep(Duration::from_millis(50))=>checkpoint()?,
                }};
                let status=response.status().as_u16();let mut bytes=vec![];
                loop{
                    let next=response.chunk();tokio::pin!(next);
                    let chunk=loop{tokio::select!{
                        chunk=&mut next=>break chunk.map_err(|e|RetryableEmbedding(format!("Embedding transport unavailable: {}",e.without_url())))?,
                        _=tokio::time::sleep(Duration::from_millis(50))=>checkpoint()?,
                    }};
                    let Some(chunk)=chunk else{break;};anyhow::ensure!(bytes.len()+chunk.len()<=16*1024*1024,"Embedding response exceeds limit");bytes.extend_from_slice(&chunk);
                }
                Ok::<_,anyhow::Error>((status,serde_json::from_slice::<Value>(&bytes)?))
            });
            match result {
                Ok(result) => break result,
                Err(error) if attempt == 0 && retryable(&error) => {
                    attempt += 1;
                    checkpoint()?;
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(error) => return Err(error),
            }
        };
        let error_message = body.get("error").map(Value::to_string).unwrap_or_default();
        let size_error = ["too large", "too long", "batch size", "context size"]
            .iter()
            .any(|word| error_message.to_lowercase().contains(word));
        if (status == 429 || status >= 500) && !size_error {
            return Err(RetryableEmbedding(format!(
                "Embedding server HTTP {status}: {error_message}"
            ))
            .into());
        }
        anyhow::ensure!(
            status < 400,
            "Embedding server HTTP {status}: {}",
            body.get("error").map(Value::to_string).unwrap_or_default()
        );
        let mut data = body["data"]
            .as_array()
            .context("Embedding response missing vectors")?
            .clone();
        data.sort_by_key(|x| x["index"].as_u64().unwrap_or(0));
        let mut vectors = vec![];
        let mut dimensions = None;
        let single = data.len() == 1;
        for (index, item) in data.into_iter().enumerate() {
            anyhow::ensure!(
                item["index"].as_u64() == Some(index as u64) || single && item["index"].is_null(),
                "Embedding response contains missing or duplicate indices"
            );
            let values: Vec<f32> = serde_json::from_value(item["embedding"].clone())?;
            let norm = values
                .iter()
                .map(|x| (*x as f64) * (*x as f64))
                .sum::<f64>();
            anyhow::ensure!(
                !values.is_empty()
                    && values.len() <= 16384
                    && values.iter().all(|v| v.is_finite())
                    && norm.is_finite()
                    && norm > 1e-30,
                "Invalid embedding response"
            );
            anyhow::ensure!(
                dimensions.is_none_or(|n| n == values.len()),
                "Embedding dimensions changed within one response"
            );
            dimensions = Some(values.len());
            vectors.push(normalize(values));
        }
        Ok(vectors)
    }
    pub fn text_batch(&self, s: &Settings, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.text_batch_checked(s, texts, &|| Ok(()))
    }
    fn text_batch_checked(
        &self,
        s: &Settings,
        texts: &[String],
        checkpoint: &dyn Fn() -> Result<()>,
    ) -> Result<Vec<Vec<f32>>> {
        checkpoint()?;
        let _activity = self.activity();
        if texts.is_empty() {
            return Ok(vec![]);
        }
        self.ensure_checked(s, false, checkpoint)?;
        if s.remote_embeddings()
            && texts
                .iter()
                .any(|t| t.chars().count() > s.remote_context.clamp(128, 131072) as usize * 2)
        {
            let mut output = vec![];
            for text in texts {
                let parts =
                    crate::content::chunks(text, s.remote_context.clamp(128, 131072) as usize * 2)
                        .into_iter()
                        .map(|(t, _)| t)
                        .collect::<Vec<_>>();
                let parts = if parts.is_empty() {
                    vec![text.clone()]
                } else {
                    parts
                };
                let vectors = self.text_batch_checked(s, &parts, checkpoint)?;
                let mut combined = vec![0.0; vectors[0].len()];
                for vector in vectors {
                    anyhow::ensure!(
                        vector.len() == combined.len(),
                        "Embedding dimensions changed"
                    );
                    for (a, b) in combined.iter_mut().zip(vector) {
                        *a += b;
                    }
                }
                output.push(normalize(combined));
            }
            return Ok(output);
        }
        match self.post_checked(s, json!(texts), checkpoint) {
            Ok(v) => {
                anyhow::ensure!(
                    v.len() == texts.len(),
                    "Server returned a different number of vectors than inputs"
                );
                Ok(v)
            }
            Err(e) => {
                let message = e.to_string().to_lowercase();
                if message.contains("too large")
                    || message.contains("too long")
                    || message.contains("batch size")
                    || message.contains("context size")
                {
                    if texts.len() > 1 {
                        let mid = texts.len() / 2;
                        let mut out = self.text_batch_checked(s, &texts[..mid], checkpoint)?;
                        out.extend(self.text_batch_checked(s, &texts[mid..], checkpoint)?);
                        return Ok(out);
                    }
                    let chars: Vec<char> = texts[0].chars().collect();
                    anyhow::ensure!(chars.len() > 1, "Model cannot embed a minimal input");
                    let mid = chars.len() / 2;
                    let parts = vec![chars[..mid].iter().collect(), chars[mid..].iter().collect()];
                    let v = self.text_batch_checked(s, &parts, checkpoint)?;
                    let mut combined = vec![0.0; v[0].len()];
                    for vector in v {
                        anyhow::ensure!(
                            vector.len() == combined.len(),
                            "Embedding dimensions changed"
                        );
                        for (x, y) in combined.iter_mut().zip(vector) {
                            *x += y;
                        }
                    }
                    return Ok(vec![normalize(combined)]);
                }
                Err(e)
            }
        }
    }
    pub fn segments(
        &self,
        s: &Settings,
        segments: &[Segment],
        checkpoint: &dyn Fn() -> Result<()>,
    ) -> Result<Vec<Vec<f32>>> {
        let mut out = vec![];
        let mut index = 0;
        while index < segments.len() {
            checkpoint()?;
            while self.interactive.load(Ordering::Acquire) > 0 {
                checkpoint()?;
                std::thread::sleep(Duration::from_millis(10));
            }
            let _activity = self.activity();
            let segment = &segments[index];
            if !segment.media.is_empty() {
                self.ensure_checked(s, true, checkpoint)?;
                let key = format!(
                    "{}|{}|{}|{}",
                    Self::url(s),
                    s.remote_model,
                    s.model,
                    s.remote_context
                );
                let single = self.media_single.lock().unwrap().contains(&key);
                let mut end = index + 1;
                let mut bytes = segment.media.len();
                while !single
                    && end < segments.len()
                    && end - index < 4
                    && !segments[end].media.is_empty()
                    && segments[end].kind == segment.kind
                    && (segments[end].media.starts_with(b"RIFF")
                        || segments[end].media.starts_with(b"fLaC"))
                        == (segment.media.starts_with(b"RIFF")
                            || segment.media.starts_with(b"fLaC"))
                    && bytes + segments[end].media.len() <= 2 * 1024 * 1024
                {
                    bytes += segments[end].media.len();
                    end += 1;
                }
                let input = Value::Array(
                    segments[index..end]
                        .iter()
                        .map(|r| crate::remote::media_input(&r.media)[0].clone())
                        .collect(),
                );
                let result = self.post_checked(s, input, checkpoint).and_then(|vectors| {
                    anyhow::ensure!(
                        vectors.len() == end - index,
                        "Server did not return one vector per media input"
                    );
                    Ok(vectors)
                });
                match result {
                    Ok(vectors) => out.extend(vectors),
                    Err(error) if end - index > 1 && !retryable(&error) => {
                        let mut cache = self.media_single.lock().unwrap();
                        if cache.len() >= 8 {
                            cache.clear();
                        }
                        cache.insert(key);
                        drop(cache);
                        for record in &segments[index..end] {
                            checkpoint()?;
                            let vectors = self.post_checked(
                                s,
                                crate::remote::media_input(&record.media),
                                checkpoint,
                            )?;
                            anyhow::ensure!(vectors.len() == 1, "Missing media embedding");
                            out.extend(vectors);
                        }
                    }
                    Err(error) => return Err(error),
                }
                index = end;
            } else {
                let end = (index + s.batch_size.clamp(1, 16)).min(segments.len());
                let end = (index..end)
                    .find(|&i| !segments[i].media.is_empty())
                    .unwrap_or(end);
                let texts = segments[index..end]
                    .iter()
                    .map(|x| format!("title: none | text: {}", x.text))
                    .collect::<Vec<_>>();
                out.extend(self.text_batch_checked(s, &texts, checkpoint)?);
                index = end;
            }
        }
        Ok(out)
    }
    pub fn query(&self, s: &Settings, q: &str) -> Result<Vec<f32>> {
        self.query_checked(s, q, &|| Ok(()))
    }
    pub fn query_checked(
        &self,
        s: &Settings,
        q: &str,
        checkpoint: &dyn Fn() -> Result<()>,
    ) -> Result<Vec<f32>> {
        checkpoint()?;
        self.interactive.fetch_add(1, Ordering::AcqRel);
        let _priority = QueryPriority(&self.interactive);
        if let Some(reason) = self.search_hold(s) {
            anyhow::bail!(
                "EmbeddingGemma 2 is paused: {reason}. Use Load anyway for this search session."
            );
        }
        // One short-lived query vector avoids repeating inference for multi-source searches.
        let key = format!(
            "{}|{}|{}|{}|{}",
            Self::url(s),
            s.remote_key,
            if s.remote_embeddings() {
                &s.remote_model
            } else {
                &s.model
            },
            s.server,
            q
        );
        let mut cache = self
            .query_cache
            .lock()
            .map_err(|_| anyhow::anyhow!("Query cache unavailable"))?;
        if let Some((saved, when, vector)) = &*cache {
            if *saved == key && when.elapsed() < Duration::from_secs(2) {
                return Ok(vector.clone());
            }
        }
        let vector = self
            .text_batch_checked(
                s,
                &[format!("task: search result | query: {q}")],
                checkpoint,
            )?
            .into_iter()
            .next()
            .context("Missing query embedding")?;
        *cache = Some((key, Instant::now(), vector.clone()));
        Ok(vector)
    }
}
pub fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt()
        .max(1e-30);
    for x in &mut v {
        *x = (*x as f64 / norm) as f32;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn models_are_lazy() {
        let m = Models::default();
        assert!(!m.loaded.load(Ordering::Relaxed));
        assert!(m.inner.lock().unwrap().child.is_none());
    }
    #[test]
    fn normalizing() {
        assert_eq!(normalize(vec![3.0, 4.0]), vec![0.6, 0.8]);
    }
}
#[cfg(test)]
mod batch_tests {
    use super::*;
    #[test]
    fn batching_recovers_size_error_and_preserves_order() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let address = server.server_addr().to_ip().unwrap();
        let thread = std::thread::spawn(move || {
            for _ in 0..3 {
                let mut request = server.recv().unwrap();
                let mut bytes = vec![];
                request.as_reader().read_to_end(&mut bytes).unwrap();
                let value: Value = serde_json::from_slice(&bytes).unwrap();
                let items = value["input"].as_array().unwrap();
                let (status, body) = if items.len() > 2 {
                    (
                        500,
                        json!({"error":"input too large: increase physical batch size"}),
                    )
                } else {
                    (
                        200,
                        json!({"data":items.iter().enumerate().rev().map(|(i,x)|json!({"index":i,"embedding":[1.0,x.as_str().unwrap().parse::<f32>().unwrap()]})).collect::<Vec<_>>()}),
                    )
                };
                request
                    .respond(
                        tiny_http::Response::from_string(body.to_string()).with_status_code(status),
                    )
                    .unwrap();
            }
        });
        let s = Settings {
            remote_url: format!("http://{address}"),
            ..Default::default()
        };
        let models = Models::default();
        let result = models
            .text_batch(&s, &["1".into(), "2".into(), "3".into(), "4".into()])
            .unwrap();
        assert_eq!(result.len(), 4);
        for (i, v) in result.iter().enumerate() {
            assert!((v[1] / v[0] - (i + 1) as f32).abs() < 0.001);
        }
        assert!(models.inner.lock().unwrap().child.is_none());
        thread.join().unwrap();
    }
}
#[cfg(test)]
mod idle_unload_tests {
    use super::*;
    #[test]
    #[ignore]
    fn sleeping_model_helper() {
        std::thread::sleep(Duration::from_secs(30));
    }
    #[test]
    fn idle_stops_owned_process_but_never_interrupts_activity() {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args([
            "--ignored",
            "--exact",
            "models::idle_unload_tests::sleeping_model_helper",
        ]);
        crate::content::hidden(&mut command);
        let mut child = command.spawn().unwrap();
        let job = crate::platform::owned_process(&mut child).unwrap();
        let models = Models::default();
        let s = Settings {
            unload_idle: true,
            unload_minutes: 1,
            ..Default::default()
        };
        {
            let mut state = models.inner.lock().unwrap();
            state.child = Some(child);
            state.job = Some(job);
            state.media = true;
            state.last_used = Instant::now() - Duration::from_secs(121);
        }
        models.loaded.store(true, Ordering::Relaxed);
        let activity = models.activity();
        assert!(!models.idle_unload(&s));
        assert!(models.inner.lock().unwrap().child.is_some());
        drop(activity);
        assert!(!models.idle_unload(&s));
        models.inner.lock().unwrap().last_used = Instant::now() - Duration::from_secs(121);
        assert!(models.idle_unload(&s));
        assert!(!models.loaded.load(Ordering::Relaxed));
        assert!(!models.media_loaded());
        assert!(models.inner.lock().unwrap().child.is_none());
        assert!(!models.idle_unload(&s));
    }
}

#[cfg(test)]
mod search_session_tests {
    use super::*;
    fn helper(models: &Models) {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--ignored",
            "--exact",
            "models::idle_unload_tests::sleeping_model_helper",
        ]);
        crate::content::hidden(&mut cmd);
        let child = cmd.spawn().unwrap();
        let job = crate::platform::owned_process(&child).unwrap();
        let mut state = models.inner.lock().unwrap();
        state.child = Some(child);
        state.job = Some(job);
        state.last_used = Instant::now() - Duration::from_secs(121);
        models.loaded.store(true, Ordering::Relaxed);
    }
    #[test]
    fn session_pin_defers_game_unload_until_closed_and_never_interrupts_a_request() {
        let models = Models::default();
        helper(&models);
        let s = Settings {
            pause_for_game: true,
            unload_idle: false,
            ..Default::default()
        };
        let metrics = crate::scheduling::Metrics {
            game: true,
            ..Default::default()
        };
        models.session_override.store(true, Ordering::Release);
        assert!(!models.unload_with_metrics(&s, metrics));
        models.session_override.store(false, Ordering::Release);
        let request = models.activity();
        assert!(!models.unload_with_metrics(&s, metrics));
        drop(request);
        assert!(models.unload_with_metrics(&s, metrics));
        assert!(!models.loaded.load(Ordering::Acquire));
        assert!(models.inner.lock().unwrap().child.is_none());
    }
    #[test]
    fn readiness_progress_waits_for_healthy_server() {
        let models = Models::default();
        helper(&models);
        models.loaded.store(false, Ordering::Relaxed);
        models.loading.store(true, Ordering::Release);
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let responded = Arc::new(AtomicBool::new(false));
        let seen = responded.clone();
        let task = std::thread::spawn(move || {
            let first = server.recv().unwrap();
            first
                .respond(tiny_http::Response::from_string("loading").with_status_code(503))
                .unwrap();
            seen.store(true, Ordering::Release);
            let second = server.recv().unwrap();
            std::thread::sleep(Duration::from_millis(300));
            second
                .respond(tiny_http::Response::from_string("ready").with_status_code(200))
                .unwrap();
        });
        let wait = models.clone();
        let loading = std::thread::spawn(move || wait.wait_ready_at(&url));
        let start = Instant::now();
        while !responded.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(models.loading.load(Ordering::Acquire));
        assert!(!models.loaded.load(Ordering::Acquire));
        assert!(models.loading_phase().contains("Loading EmbeddingGemma"));
        loading.join().unwrap().unwrap();
        task.join().unwrap();
        assert!(models.loaded.load(Ordering::Acquire));
        assert!(!models.loading.load(Ordering::Acquire));
        models.stop().unwrap();
    }
}

#[cfg(test)]
mod typed_media_tests {
    use super::*;
    #[test]
    fn media_indexing_routes_model_and_auth_without_legacy_props() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let settings = Settings {
            embedding_server: "Remote".into(),
            remote_url: format!("http://{}/v1", server.server_addr()),
            remote_model: "embeddinggemma2".into(),
            remote_key: "test-key".into(),
            ..Default::default()
        };
        let task = std::thread::spawn(move || {
            for media_type in ["image_url", "input_audio"] {
                let mut req = server.recv().unwrap();
                assert_eq!(req.url(), "/v1/embeddings");
                assert!(req.headers().iter().any(
                    |h| h.field.equiv("Authorization") && h.value.as_str() == "Bearer test-key"
                ));
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body).unwrap();
                let body: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(body["model"], "embeddinggemma2");
                assert_eq!(body["input"][0]["content"][0]["type"], media_type);
                req.respond(
                    tiny_http::Response::from_string(
                        json!({"data":[{"embedding":[0.1,0.2]}]}).to_string(),
                    )
                    .with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ),
                )
                .unwrap();
            }
        });
        let segments = [b"\xff\xd8image".to_vec(), b"RIFFaudioWAVE".to_vec()]
            .into_iter()
            .map(|media| Segment {
                media,
                kind: "image".into(),
                text: String::new(),
                page: 0,
                seconds: 0.0,
                line: 0,
            })
            .collect::<Vec<_>>();
        let vectors = Models::default()
            .segments(&settings, &segments, &|| Ok(()))
            .unwrap();
        assert_eq!(vectors.len(), 2);
        task.join().unwrap();
    }
}

#[cfg(test)]
mod response_stability_tests {
    use super::*;
    fn server(body: &str) -> (Settings, std::thread::JoinHandle<()>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let s = Settings {
            remote_url: format!("http://{}", server.server_addr()),
            ..Default::default()
        };
        let body = body.to_owned();
        let handle = std::thread::spawn(move || {
            let request = server.recv().unwrap();
            request
                .respond(tiny_http::Response::from_string(body))
                .unwrap();
        });
        (s, handle)
    }
    #[test]
    fn bad_vectors_and_duplicate_indices_are_rejected() {
        for body in [
            r#"{"data":[{"index":0,"embedding":[0,0]}]}"#,
            r#"{"data":[{"index":0,"embedding":[1,0]},{"index":0,"embedding":[0,1]}]}"#,
            r#"{"data":[{"index":0,"embedding":[1,0]},{"index":1,"embedding":[1,0,0]}]}"#,
        ] {
            let (s, thread) = server(body);
            assert!(Models::default().post(&s, json!(["one", "two"])).is_err());
            thread.join().unwrap();
        }
    }
    #[test]
    fn media_batch_keeps_one_vector_per_input_and_falls_back_when_unsupported() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let s = Settings {
            remote_url: format!("http://{}", server.server_addr()),
            ..Default::default()
        };
        let thread = std::thread::spawn(move || {
            let mut count = 0;
            for _ in 0..5 {
                let mut request = server.recv().unwrap();
                let mut bytes = String::new();
                request.as_reader().read_to_string(&mut bytes).unwrap();
                let v: Value = serde_json::from_str(&bytes).unwrap();
                count += 1;
                if v["input"].as_array().unwrap().len() > 1 {
                    request
                        .respond(
                            tiny_http::Response::from_string("{\"error\":\"batch unsupported\"}")
                                .with_status_code(400),
                        )
                        .unwrap();
                } else {
                    request
                        .respond(tiny_http::Response::from_string(
                            "{\"data\":[{\"index\":0,\"embedding\":[1,2]}]}",
                        ))
                        .unwrap();
                }
            }
            count
        });
        let media = Segment {
            kind: "image".into(),
            text: String::new(),
            page: 0,
            seconds: -1.0,
            line: 0,
            media: vec![0x89, b'P', b'N', b'G', 1],
        };
        let models = Models::default();
        let a = models
            .segments(&s, &[media.clone(), media.clone()], &|| Ok(()))
            .unwrap();
        let b = models
            .segments(&s, &[media.clone(), media], &|| Ok(()))
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 2);
        assert_eq!(thread.join().unwrap(), 5);
    }
    #[test]
    fn transient_media_error_is_not_fanned_out_into_multiple_requests() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let s = Settings {
            remote_url: format!("http://{}", server.server_addr()),
            ..Default::default()
        };
        let thread = std::thread::spawn(move || {
            let r = server.recv().unwrap();
            r.respond(
                tiny_http::Response::from_string("{\"error\":\"busy\"}").with_status_code(503),
            )
            .unwrap();
            assert!(
                server
                    .recv_timeout(Duration::from_millis(300))
                    .unwrap()
                    .is_none()
            );
        });
        let media = Segment {
            kind: "image".into(),
            text: String::new(),
            page: 0,
            seconds: -1.0,
            line: 0,
            media: vec![0x89, b'P', b'N', b'G', 1],
        };
        assert!(
            Models::default()
                .segments(&s, &[media.clone(), media], &|| Ok(()))
                .is_err()
        );
        thread.join().unwrap();
    }
}
