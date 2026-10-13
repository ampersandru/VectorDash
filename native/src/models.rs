use crate::{config::Settings, content::Segment};
use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct Models {
    inner: Arc<Mutex<State>>,
    agent: ureq::Agent,
    pub loaded: Arc<AtomicBool>,
    active: Arc<AtomicUsize>,
    query_cache: Arc<Mutex<Option<(String, Instant, Vec<f32>)>>>,
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
                .timeout_connect(Some(Duration::from_secs(3)))
                .timeout_global(Some(Duration::from_secs(90)))
                .http_status_as_error(false)
                .build()
                .into(),
            loaded: Arc::new(AtomicBool::new(false)),
            active: Arc::new(AtomicUsize::new(0)),
            query_cache: Arc::new(Mutex::new(None)),
        }
    }
}
impl Models {
    pub fn url(s: &Settings) -> String {
        if s.remote_url.is_empty() {
            "http://127.0.0.1:8788".into()
        } else {
            s.remote_url.trim_end_matches('/').into()
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
        Ok(())
    }
    pub fn start(&self, s: &Settings, media: bool) -> Result<()> {
        let _activity = self.activity();
        self.ensure(s, media)
    }
    pub fn media_loaded(&self) -> bool {
        self.inner.try_lock().map(|s| s.media).unwrap_or(false)
    }
    pub fn idle_unload(&self, s: &Settings) -> bool {
        if !s.unload_idle {
            return false;
        }
        if let Ok(mut state) = self.inner.try_lock() {
            if self.active.load(Ordering::Acquire) == 0
                && state.child.is_some()
                && state.last_used.elapsed()
                    > Duration::from_secs(s.unload_minutes.max(1) as u64 * 60)
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
    fn ensure(&self, s: &Settings, media: bool) -> Result<()> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Model state unavailable"))?;
        state.last_used = Instant::now();
        if !s.remote_url.is_empty() {
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
            return self.wait_ready(s);
        }

        if let Some(mut c) = state.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        anyhow::ensure!(
            !s.server.is_empty() && !s.model.is_empty(),
            "Set up or select llama-server and EmbeddingGemma 2 in Models & hardware. Names search does not need a model."
        );
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
        self.wait_ready(s)
    }
    fn wait_ready(&self, s: &Settings) -> Result<()> {
        let start = Instant::now();
        loop {
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
            if let Ok(r) = self.agent.get(format!("{}/health", Self::url(s))).call() {
                if r.status().as_u16() == 200 {
                    self.loaded.store(true, Ordering::Relaxed);
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
    fn marker(&self, s: &Settings) -> Result<String> {
        let key = format!("{}|{}", Self::url(s), s.projector);
        if let Some((_, marker)) = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Model state unavailable"))?
            .marker
            .as_ref()
            .filter(|(k, _)| k == &key)
        {
            return Ok(marker.clone());
        }
        let mut req = self.agent.get(format!("{}/props", Self::url(s)));
        if !s.remote_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", s.remote_key));
        }
        let props: Value = req.call()?.body_mut().read_json()?;
        let marker=props["media_marker"].as_str().context("This server lacks multimodal embeddings; update llama-server and select its matching encoder")?.to_string();
        self.inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Model state unavailable"))?
            .marker = Some((key, marker.clone()));
        Ok(marker)
    }
    fn post(&self, s: &Settings, input: Value) -> Result<Vec<Vec<f32>>> {
        let url = Self::url(s);
        anyhow::ensure!(
            url.starts_with("http://") || url.starts_with("https://"),
            "Server URL must use HTTP or HTTPS"
        );
        let mut req = self.agent.post(format!("{url}/v1/embeddings"));
        if !s.remote_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", s.remote_key));
        }
        let mut response = req.send_json(json!({"input":input}))?;
        let status = response.status().as_u16();
        let body: Value = response
            .body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_json()?;
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
        for item in data {
            let values: Vec<f32> = serde_json::from_value(item["embedding"].clone())?;
            anyhow::ensure!(
                !values.is_empty() && values.iter().all(|x| x.is_finite()),
                "Invalid embedding response"
            );
            vectors.push(normalize(values));
        }
        Ok(vectors)
    }
    pub fn text_batch(&self, s: &Settings, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let _activity = self.activity();
        if texts.is_empty() {
            return Ok(vec![]);
        }
        self.ensure(s, false)?;
        match self.post(s, json!(texts)) {
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
                        let mut out = self.text_batch(s, &texts[..mid])?;
                        out.extend(self.text_batch(s, &texts[mid..])?);
                        return Ok(out);
                    }
                    let chars: Vec<char> = texts[0].chars().collect();
                    anyhow::ensure!(chars.len() > 1, "Model cannot embed a minimal input");
                    let mid = chars.len() / 2;
                    let parts = vec![chars[..mid].iter().collect(), chars[mid..].iter().collect()];
                    let v = self.text_batch(s, &parts)?;
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
            let _activity = self.activity();
            let segment = &segments[index];
            if !segment.media.is_empty() {
                self.ensure(s, true)?;
                let marker = self.marker(s)?;
                let v=self.post(s,json!({"prompt_string":marker,"multimodal_data":[STANDARD.encode(&segment.media)]}))?;
                out.push(v.into_iter().next().context("Missing media embedding")?);
                index += 1;
            } else {
                let end = (index + s.batch_size.clamp(1, 16)).min(segments.len());
                let end = (index..end)
                    .find(|&i| !segments[i].media.is_empty())
                    .unwrap_or(end);
                let texts = segments[index..end]
                    .iter()
                    .map(|x| format!("title: none | text: {}", x.text))
                    .collect::<Vec<_>>();
                out.extend(self.text_batch(s, &texts)?);
                index = end;
            }
        }
        Ok(out)
    }
    pub fn query(&self, s: &Settings, q: &str) -> Result<Vec<f32>> {
        // One short-lived query vector avoids repeating inference for multi-source searches.
        let key = format!(
            "{}|{}|{}|{}|{}",
            Self::url(s),
            s.remote_key,
            s.model,
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
            .text_batch(s, &[format!("task: search result | query: {q}")])?
            .into_iter()
            .next()
            .context("Missing query embedding")?;
        *cache = Some((key, Instant::now(), vector.clone()));
        Ok(vector)
    }
}
pub fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-30);
    for x in &mut v {
        *x /= norm;
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
