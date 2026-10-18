use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub const PROVIDERS: [&str; 3] = ["SearXNG", "Degoog", "Brave"];
const BRAVE_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const MAX_BODY: usize = 4 * 1024 * 1024;
const MAX_RESULTS: usize = 40;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub previews: bool,
    pub provider: String,
    pub ai_provider: String,
    pub ai_concurrent_searches: usize,
    pub searxng_url: String,
    pub searxng_key: String,
    pub degoog_url: String,
    pub degoog_key: String,
    pub brave_key: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            previews: false,
            provider: "SearXNG".into(),
            ai_provider: String::new(),
            ai_concurrent_searches: 1,
            searxng_url: String::new(),
            searxng_key: String::new(),
            degoog_url: String::new(),
            degoog_key: String::new(),
            brave_key: String::new(),
        }
    }
}
impl Settings {
    pub fn providers(&self) -> Vec<&'static str> {
        PROVIDERS
            .into_iter()
            .filter(|p| match *p {
                "SearXNG" => !self.searxng_url.trim().is_empty(),
                "Degoog" => !self.degoog_url.trim().is_empty(),
                "Brave" => !self.brave_key.trim().is_empty(),
                _ => false,
            })
            .collect()
    }
    pub fn ai_concurrency(&self) -> usize {
        self.ai_concurrent_searches.clamp(1, 2)
    }
    pub fn ai_selected(&self) -> String {
        self.selected(&self.ai_provider)
    }
    pub fn selected(&self, current: &str) -> String {
        let providers = self.providers();
        if providers.contains(&current) {
            current.into()
        } else if providers.contains(&self.provider.as_str()) {
            self.provider.clone()
        } else {
            providers.first().copied().unwrap_or("SearXNG").into()
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Hit {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String,
    pub thumbnail: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Page {
    pub hits: Vec<Hit>,
}
#[derive(Default)]
pub struct Session {
    pub provider: String,
    pub hits: Vec<Hit>,
    pub submitted: String,
    pub error: Option<String>,
    pub busy: bool,
    pub started: Option<Instant>,
    pub elapsed_ms: Option<f64>,
    id: u64,
    cancel: Arc<AtomicBool>,
}
impl Session {
    pub fn start(&mut self, query: &str) -> (u64, Arc<AtomicBool>) {
        self.clear();
        self.submitted = query.trim().into();
        self.busy = true;
        self.started = Some(Instant::now());
        self.cancel = Arc::new(AtomicBool::new(false));
        (self.id, self.cancel.clone())
    }
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.id = self.id.wrapping_add(1);
        self.busy = false;
        self.started = None;
    }
    pub fn clear(&mut self) {
        self.stop();
        self.hits.clear();
        self.error = None;
        self.elapsed_ms = None;
    }
    pub fn edited(&mut self, query: &str) {
        if query.trim() != self.submitted {
            self.clear();
        }
    }
    pub fn finish(&mut self, id: u64, result: std::result::Result<Page, String>, ms: f64) -> bool {
        if id != self.id || !self.busy {
            return false;
        }
        self.busy = false;
        self.started = None;
        self.elapsed_ms = Some(ms);
        match result {
            Ok(page) => self.hits = page.hits,
            Err(error) => self.error = Some(error),
        }
        true
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

pub fn endpoint(base: &str, route: &str) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(base.trim())
        .context("Enter a valid HTTP(S) instance URL in Web settings")?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Use an HTTP(S) instance URL without credentials, query, or fragment"
    );
    let path = url.path().trim_end_matches('/');
    if !path.ends_with(&format!("/{route}")) {
        let route = if route == "api/search" && path.ends_with("/api") {
            "search"
        } else {
            route
        };
        url.set_path(&format!("{path}/{route}"));
    } else {
        url.set_path(&path.to_owned());
    }
    Ok(url)
}
pub fn result_url(value: &str) -> Option<reqwest::Url> {
    reqwest::Url::parse(value).ok().filter(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}
fn text(value: &str, limit: usize) -> String {
    let bounded = value.chars().take(8192).collect::<String>();
    let html = scraper::Html::parse_fragment(&bounded);
    html.root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
fn parse(provider: &str, value: &Value) -> Result<Page> {
    let rows = if provider == "Brave" {
        value.pointer("/web/results")
    } else {
        value.get("results")
    };
    let empty = vec![];
    let rows = if provider == "Brave" && rows.is_none() && value.get("query").is_some() {
        &empty
    } else {
        rows.and_then(Value::as_array)
            .context("The provider did not return a web result list")?
    };
    let mut seen = std::collections::HashSet::new();
    let mut hits = vec![];
    for row in rows {
        let Some(url) = row.get("url").and_then(Value::as_str).and_then(result_url) else {
            continue;
        };
        let url = url.to_string();
        if !seen.insert(url.clone()) {
            continue;
        }
        let title = text(row["title"].as_str().unwrap_or(""), 512);
        let snippet = ["snippet", "content", "description"]
            .iter()
            .find_map(|key| {
                row.get(*key)
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty())
            })
            .unwrap_or("");
        hits.push(Hit {
            title: if title.is_empty() { url.clone() } else { title },
            url,
            snippet: text(snippet, 2000),
            source: text(row["source"].as_str().unwrap_or(provider), 80),
            thumbnail: [
                row.pointer("/thumbnail/src"),
                row.get("thumbnail"),
                row.get("img_src"),
            ]
            .into_iter()
            .flatten()
            .find_map(|v| v.as_str().and_then(result_url))
            .filter(|u| u.as_str().len() <= 2048)
            .map(|u| u.to_string()),
        });
        if hits.len() == MAX_RESULTS {
            break;
        }
    }
    Ok(Page { hits })
}
fn request(
    s: &Settings,
    provider: &str,
    query: &str,
) -> Result<(reqwest::Url, Option<(&'static str, String)>)> {
    anyhow::ensure!(s.enabled, "Web search is disabled");
    anyhow::ensure!(
        !query.trim().is_empty() && query.len() <= 4096,
        "Enter a web query of up to 4096 bytes"
    );
    let (mut url, header) = match provider {
        "SearXNG" => (
            endpoint(&s.searxng_url, "search")?,
            (!s.searxng_key.trim().is_empty())
                .then(|| ("Authorization", format!("Bearer {}", s.searxng_key.trim()))),
        ),
        "Degoog" => (
            endpoint(&s.degoog_url, "api/search")?,
            (!s.degoog_key.trim().is_empty())
                .then(|| ("Authorization", format!("Bearer {}", s.degoog_key.trim()))),
        ),
        "Brave" => {
            anyhow::ensure!(
                !s.brave_key.trim().is_empty(),
                "Add a Brave API key in Web settings"
            );
            (
                reqwest::Url::parse(BRAVE_URL)?,
                Some(("X-Subscription-Token", s.brave_key.trim().into())),
            )
        }
        _ => anyhow::bail!("Select a configured provider in Web settings"),
    };
    url.query_pairs_mut().append_pair("q", query.trim());
    match provider {
        "SearXNG" => {
            url.query_pairs_mut().append_pair("format", "json");
        }
        "Degoog" => {
            url.query_pairs_mut().append_pair("type", "web");
        }
        "Brave" => {
            url.query_pairs_mut().append_pair("count", "20");
        }
        _ => {}
    }
    Ok((url, header))
}
pub fn search(s: &Settings, provider: &str, query: &str, cancel: &AtomicBool) -> Result<Page> {
    let (url, header) = request(s, provider, query)?;
    transport(provider, url, header, cancel)
}
fn transport(
    provider: &str,
    url: reqwest::Url,
    header: Option<(&str, String)>,
    cancel: &AtomicBool,
) -> Result<Page> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30)).redirect(reqwest::redirect::Policy::none()).build()?;
        let mut req = client.get(url).header("Accept","application/json");
        if let Some((name,key)) = header { req = req.header(name,key); }
        let cancellation = async { while !cancel.load(Ordering::Acquire) { tokio::time::sleep(Duration::from_millis(20)).await; } };
        tokio::pin!(cancellation);
        let mut response = tokio::select! { biased; _=&mut cancellation => anyhow::bail!("Web search stopped"), result=req.send() => result.map_err(|_|anyhow::anyhow!("Could not reach the web provider (check its URL and connection)"))? };
        let status = response.status();
        anyhow::ensure!(!status.is_redirection(), "The web provider redirected; enter its final instance URL");
        if provider == "SearXNG" && status.as_u16() == 403 { anyhow::bail!("SearXNG denied the request. Enable JSON output in its search formats and check the API key."); }
        if matches!(status.as_u16(),401|403) { anyhow::bail!("The web provider rejected the API key or access permissions"); }
        if status.as_u16()==429 { anyhow::bail!("The web provider is rate limiting requests. Try again shortly."); }
        anyhow::ensure!(status.is_success(), "Web provider returned HTTP {}",status.as_u16());
        let mut bytes = Vec::new();
        loop {
            let chunk = tokio::select! { biased; _=&mut cancellation => anyhow::bail!("Web search stopped"), next=response.chunk() => next.map_err(|_|anyhow::anyhow!("Could not read the web provider response"))? };
            let Some(chunk) = chunk else {break;};
            anyhow::ensure!(bytes.len()+chunk.len() <= MAX_BODY,"Web provider response exceeded the 4 MB limit");
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).context("The web provider did not return JSON")?;
        parse(provider,&value)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
    };
    fn server(
        status: &str,
        body: String,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let status = status.to_owned();
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = vec![];
            let mut chunk = [0; 2048];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..n]);
            }
            tx.send(String::from_utf8_lossy(&request).into()).unwrap();
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        });
        (url, rx, worker)
    }
    #[test]
    fn ai_provider_is_independent_and_legacy_settings_fall_back() {
        let mut s: Settings=serde_json::from_str(r#"{"provider":"Degoog","searxng_url":"http://localhost/searx","degoog_url":"http://localhost/degoog"}"#).unwrap();
        assert_eq!(s.ai_selected(), "Degoog");
        assert_eq!(s.ai_concurrency(), 1);
        s.ai_concurrent_searches = 999;
        assert_eq!(s.ai_concurrency(), 2);
        s.ai_concurrent_searches = 0;
        assert_eq!(s.ai_concurrency(), 1);
        s.ai_provider = "SearXNG".into();
        assert_eq!(s.ai_selected(), "SearXNG");
        let copy: Settings = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
        assert_eq!(copy.ai_selected(), "SearXNG");
        s.searxng_url.clear();
        assert_eq!(s.ai_selected(), "Degoog");
    }
    #[test]
    fn instance_urls_preserve_prefix_and_accept_full_endpoints() {
        for (base, route, want) in [
            (
                "https://example.org/",
                "search",
                "https://example.org/search",
            ),
            (
                "https://example.org/prefix/",
                "search",
                "https://example.org/prefix/search",
            ),
            (
                "https://example.org/search/",
                "search",
                "https://example.org/search",
            ),
            (
                "https://example.org/api/",
                "api/search",
                "https://example.org/api/search",
            ),
            (
                "https://example.org/prefix/api/search",
                "api/search",
                "https://example.org/prefix/api/search",
            ),
        ] {
            assert_eq!(endpoint(base, route).unwrap().as_str(), want);
        }
        for url in [
            "file:///c:/test",
            "javascript:alert(1)",
            "https://user:key@example.org",
            "https://example.org/?key=secret",
            "https://example.org/#secret",
        ] {
            assert!(endpoint(url, "search").is_err());
        }
    }
    #[test]
    fn all_provider_wire_contracts_encode_queries_and_keep_keys_in_headers() {
        for provider in PROVIDERS {
            let payload = if provider == "Brave" {
                json!({"web":{"results":[{"title":"Result","url":"https://example.org","description":"Brave answer"}]}})
            } else {
                json!({"results":[{"title":"Result","url":"https://example.org","snippet":"Answer","content":"Answer"}]})
            };
            let (base, rx, worker) = server("200 OK", payload.to_string());
            let s = Settings {
                enabled: true,
                searxng_url: base.clone(),
                searxng_key: "test-key".into(),
                degoog_url: base.clone(),
                degoog_key: "test-key".into(),
                brave_key: "test-key".into(),
                ..Default::default()
            };
            let query = "cats & dogs + 雨";
            let (mut url, header) = request(&s, provider, query).unwrap();
            if provider == "Brave" {
                let mock = reqwest::Url::parse(&base).unwrap();
                url.set_host(mock.host_str()).unwrap();
                url.set_scheme("http").unwrap();
                url.set_port(mock.port()).unwrap();
            }
            assert!(!url.as_str().contains("test-key"));
            let page = transport(provider, url, header, &AtomicBool::new(false)).unwrap();
            assert_eq!(page.hits.len(), 1);
            let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            let target = request
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            let url = reqwest::Url::parse(&format!("{base}{target}")).unwrap();
            let params = url
                .query_pairs()
                .collect::<std::collections::HashMap<_, _>>();
            assert_eq!(params["q"], query);
            match provider {
                "SearXNG" => {
                    assert_eq!(url.path(), "/search");
                    assert_eq!(params["format"], "json");
                    assert!(
                        request
                            .to_lowercase()
                            .contains("authorization: bearer test-key")
                    );
                }
                "Degoog" => {
                    assert_eq!(url.path(), "/api/search");
                    assert_eq!(params["type"], "web");
                    assert!(!params.contains_key("format"));
                    assert!(
                        request
                            .to_lowercase()
                            .contains("authorization: bearer test-key")
                    );
                }
                _ => {
                    assert_eq!(url.path(), "/res/v1/web/search");
                    assert_eq!(params["count"], "20");
                    assert!(
                        request
                            .to_lowercase()
                            .contains("x-subscription-token: test-key")
                    );
                }
            }
            worker.join().unwrap();
        }
    }
    #[test]
    fn results_are_plain_text_safe_deduplicated_and_bounded() {
        let mut rows = vec![
            json!({"title":"<b>Dogs &amp; cats</b>","url":"https://example.org/","content":"A <strong>small</strong> &lt;pet&gt;"}),
            json!({"url":"https://example.org"}),
            json!({"url":"javascript:alert(1)"}),
            json!({"url":"file:///c:/windows/notepad.exe"}),
            json!({"url":"https://key:secret@example.org"}),
        ];
        for n in 0..100 {
            rows.push(json!({"title":"x".repeat(2000),"url":format!("https://example.org/{n}"),"content":"x".repeat(20000)}));
        }
        let page = parse("SearXNG", &json!({"results":rows})).unwrap();
        assert_eq!(page.hits.len(), MAX_RESULTS);
        assert_eq!(page.hits[0].title, "Dogs & cats");
        assert_eq!(page.hits[0].snippet, "A small <pet>");
        assert!(page.hits.iter().all(|h| h.title.len() <= 512
            && h.snippet.len() <= 2000
            && result_url(&h.url).is_some()));
        assert!(
            parse("Brave", &json!({"query":{"original":"unmatched"}}))
                .unwrap()
                .hits
                .is_empty()
        );
        assert!(parse("Degoog", &json!({"error":"unavailable"})).is_err());
    }
    #[test]
    fn provider_errors_are_actionable_and_responses_are_bounded() {
        for (status, provider, body, expected) in [
            ("403 Forbidden", "SearXNG", "{}".into(), "JSON output"),
            ("401 Unauthorized", "Degoog", "{}".into(), "API key"),
            (
                "429 Too Many Requests",
                "Brave",
                "{}".into(),
                "rate limiting",
            ),
            (
                "500 Internal Server Error",
                "Degoog",
                "{}".into(),
                "HTTP 500",
            ),
            ("200 OK", "SearXNG", "<html>login</html>".into(), "JSON"),
            ("200 OK", "Degoog", "x".repeat(MAX_BODY + 1), "4 MB"),
            ("302 Found", "Degoog", "{}".into(), "redirected"),
        ] {
            let (url, rx, worker) = server(status, body);
            let error = transport(
                provider,
                reqwest::Url::parse(&url).unwrap(),
                None,
                &AtomicBool::new(false),
            )
            .unwrap_err()
            .to_string();
            assert!(error.contains(expected), "{error}");
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
            worker.join().unwrap();
        }
    }
    #[test]
    fn cancellation_disconnects_before_a_slow_provider_responds() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url =
            reqwest::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0; 2048];
            let _ = stream.read(&mut bytes);
            tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(700));
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let stopper = thread::spawn(move || {
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
            flag.store(true, Ordering::Release);
        });
        let started = Instant::now();
        assert!(transport("Degoog", url, None, &cancel).is_err());
        assert!(started.elapsed() < Duration::from_millis(500));
        stopper.join().unwrap();
        worker.join().unwrap();
    }
    #[test]
    fn disabled_web_never_builds_requests_and_stale_answers_are_ignored() {
        let s = Settings::default();
        assert!(!s.enabled);
        assert!(request(&s, "Brave", "query").is_err());
        let mut session = Session::default();
        let (old, flag) = session.start("old");
        let (new, _) = session.start("new");
        assert!(flag.load(Ordering::Acquire));
        assert!(!session.finish(old, Ok(Page { hits: vec![] }), 1.0));
        assert!(session.busy);
        assert!(session.finish(new, Ok(Page { hits: vec![] }), 2.0));
        assert!(!session.busy);
        assert_eq!(session.elapsed_ms, Some(2.0));
        session.edited("typed without Enter");
        assert!(session.elapsed_ms.is_none());
        assert!(!session.busy);
    }
}

/// Only fetch a provider-supplied thumbnail; never render a web page or fetch its metadata.
pub fn thumbnail(url: &str) -> Result<Vec<u8>> {
    let url = result_url(url).context("Invalid preview URL")?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(4))
                .redirect(reqwest::redirect::Policy::none())
                .build()?;
            let mut response = client.get(url).send().await?;
            anyhow::ensure!(response.status().is_success(), "Preview unavailable");
            let mut bytes = vec![];
            while let Some(chunk) = response.chunk().await? {
                anyhow::ensure!(
                    bytes.len() + chunk.len() <= 1024 * 1024,
                    "Preview too large"
                );
                bytes.extend_from_slice(&chunk);
            }
            let mut reader =
                image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(4096);
            limits.max_image_height = Some(4096);
            limits.max_alloc = Some(32 * 1024 * 1024);
            reader.limits(limits);
            let pixels = reader.decode()?.thumbnail(160, 100).to_rgba8();
            let mut out = std::io::Cursor::new(vec![]);
            image::DynamicImage::ImageRgba8(pixels).write_to(&mut out, image::ImageFormat::Png)?;
            Ok(out.into_inner())
        })
}

#[cfg(test)]
mod thumbnail_tests {
    use super::*;
    #[test]
    fn provider_images_are_optional_and_unsafe_urls_are_rejected() {
        let page=parse("SearXNG",&serde_json::json!({"results":[{"title":"a","url":"https://example.org/1","thumbnail":"https://example.org/thumb.png"},{"title":"b","url":"https://example.org/2","img_src":"file:///private.png"}]})).unwrap();
        assert_eq!(
            page.hits[0].thumbnail.as_deref(),
            Some("https://example.org/thumb.png")
        );
        assert!(page.hits[1].thumbnail.is_none());
        let brave=parse("Brave",&serde_json::json!({"web":{"results":[{"title":"c","url":"https://example.org/3","thumbnail":{"src":"https://example.org/3.png"}}]}})).unwrap();
        assert!(brave.hits[0].thumbnail.is_some());
    }
    #[test]
    fn preview_is_resized_in_memory_without_credentials() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/thumb.png", server.server_addr());
        let worker = std::thread::spawn(move || {
            let r = server
                .recv_timeout(Duration::from_secs(4))
                .unwrap()
                .unwrap();
            assert!(!r.headers().iter().any(|h| h.field.equiv("Authorization")));
            let mut bytes = std::io::Cursor::new(vec![]);
            image::DynamicImage::new_rgb8(1200, 800)
                .write_to(&mut bytes, image::ImageFormat::Png)
                .unwrap();
            r.respond(tiny_http::Response::from_data(bytes.into_inner()))
                .unwrap();
        });
        let result = thumbnail(&url).unwrap();
        let image = image::load_from_memory(&result).unwrap();
        assert!(image.width() <= 160 && image.height() <= 100);
        worker.join().unwrap();
    }
}
