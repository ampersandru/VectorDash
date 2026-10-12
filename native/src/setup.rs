use crate::{config::Settings, engine::Event};
use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
#[derive(Clone)]
pub struct Setup {
    pub home: PathBuf,
    pub cancel: Arc<AtomicBool>,
    pub tx: crate::engine::NotifySender,
    pub agent: ureq::Agent,
}
impl Setup {
    pub fn new(home: PathBuf, tx: crate::engine::NotifySender) -> Self {
        Self {
            home,
            cancel: Arc::new(AtomicBool::new(false)),
            tx,
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(120)))
                .http_status_as_error(false)
                .build()
                .into(),
        }
    }
    fn headers(&self, url: &str, s: &Settings) -> Result<Option<String>> {
        let token = if url.starts_with("https://api.github.com/") {
            &s.github_token
        } else if url.starts_with("https://huggingface.co/") {
            &s.hf_token
        } else {
            return Ok(None);
        };
        anyhow::ensure!(
            token.chars().all(|c| !c.is_control() && !c.is_whitespace()),
            "Invalid download token; paste only the token value"
        );
        Ok(if token.is_empty() {
            None
        } else {
            Some(format!("Bearer {token}"))
        })
    }
    fn get(
        &self,
        url: &str,
        s: &Settings,
        offset: u64,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        for attempt in 0..3u32 {
            anyhow::ensure!(
                !self.cancel.load(Ordering::Relaxed),
                "Setup cancelled; partial downloads retained"
            );
            let mut req = self
                .agent
                .get(url)
                .header("User-Agent", "VectorDash-Rust/0.3");
            if let Some(token) = self.headers(url, s)? {
                req = req.header("Authorization", token);
            }
            if offset > 0 {
                req = req.header("Range", format!("bytes={offset}-"));
            }
            let response = req.call();
            match response {
                Ok(r) => {
                    let status = r.status().as_u16();
                    if status < 400 {
                        return Ok(r);
                    }
                    let delay = r
                        .headers()
                        .get("retry-after")
                        .and_then(|x| x.to_str().ok())
                        .and_then(|x| x.parse::<u64>().ok())
                        .unwrap_or(1 << attempt);
                    let rate = status == 429
                        || status == 403
                            && r.headers()
                                .get("x-ratelimit-remaining")
                                .is_some_and(|x| x == "0");
                    let retry = rate || [500, 502, 503, 504].contains(&status);
                    if retry && delay <= 10 && attempt < 2 {
                        self.log(format!("Download service busy; retrying in {delay}s"));
                        self.wait(delay)?;
                        continue;
                    }
                    let provider = if url.contains("github") {
                        "GitHub"
                    } else {
                        "Hugging Face"
                    };
                    anyhow::bail!(
                        "{provider} HTTP {status}. {} Optional download tokens are in Models & hardware. Partial downloads are retained.",
                        if rate {
                            "Rate limit reached; retry later."
                        } else {
                            "Check service access and download token."
                        }
                    );
                }
                Err(e) => {
                    if attempt < 2 {
                        self.wait(1 << attempt)?;
                        continue;
                    }
                    anyhow::bail!("Download connection failed: {e}");
                }
            }
        }
        anyhow::bail!("Download retries exhausted")
    }
    fn wait(&self, seconds: u64) -> Result<()> {
        for _ in 0..seconds * 10 {
            anyhow::ensure!(!self.cancel.load(Ordering::Relaxed), "Setup cancelled");
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
    fn log(&self, message: String) {
        let _ = self.tx.send(Event::Log("Info".into(), message));
    }
    pub fn catalog(&self, url: &str, s: &Settings) -> Result<Value> {
        let name = format!("{:x}.json", Sha256::digest(url.as_bytes()));
        let folder = self.home.join("downloads/catalog-cache");
        fs::create_dir_all(&folder)?;
        let path = folder.join(name);
        let cached = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        let age = cached
            .as_ref()
            .and_then(|v| v["time"].as_i64())
            .map(|t| chrono::Utc::now().timestamp() - t)
            .unwrap_or(i64::MAX);
        if age >= 0 && age < 600 {
            return Ok(cached.unwrap()["data"].clone());
        }
        let result = self.get(url, s, 0).and_then(|mut r| {
            Ok(r.body_mut()
                .with_config()
                .limit(16 * 1024 * 1024)
                .read_json::<Value>()?)
        });
        match result {
            Ok(data) => {
                let record = serde_json::json!({"time":chrono::Utc::now().timestamp(),"data":data});
                fs::write(&path, serde_json::to_vec(&record)?)?;
                Ok(record["data"].clone())
            }
            Err(e) => {
                if age >= 0 && age < 86400 {
                    self.log("Using cached publisher metadata during service failure".into());
                    Ok(cached.unwrap()["data"].clone())
                } else {
                    Err(e)
                }
            }
        }
    }
    pub fn download(
        &self,
        url: &str,
        name: &str,
        size: u64,
        sha: &str,
        s: &Settings,
    ) -> Result<PathBuf> {
        anyhow::ensure!(
            Path::new(name).file_name().and_then(|x| x.to_str()) == Some(name),
            "Invalid download filename"
        );
        anyhow::ensure!(
            sha.len() == 64 && sha.chars().all(|x| x.is_ascii_hexdigit()),
            "Publisher SHA-256 required"
        );
        let folder = self.home.join("downloads");
        fs::create_dir_all(&folder)?;
        let target = folder.join(name);
        if target.exists() && hash(&target)? == sha.to_lowercase() {
            return Ok(target);
        }
        let partial = folder.join(format!("{name}.part"));
        let mut offset = fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
        if offset > size && size > 0 {
            fs::remove_file(&partial)?;
            offset = 0;
        }
        if size == 0 || offset < size {
            let mut response = self.get(url, s, offset)?;
            let append = offset > 0 && response.status().as_u16() == 206;
            if append {
                let range = response
                    .headers()
                    .get("content-range")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("");
                anyhow::ensure!(
                    range.starts_with(&format!("bytes {offset}-")),
                    "Unexpected resume range"
                );
            } else {
                offset = 0;
            }
            let mut file = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(&partial)?;
            let mut reader = response.body_mut().as_reader();
            let mut bytes = vec![0; 1024 * 1024];
            loop {
                let n = reader.read(&mut bytes)?;
                if n == 0 {
                    break;
                }
                anyhow::ensure!(
                    !self.cancel.load(Ordering::Relaxed),
                    "Download cancelled; partial file retained"
                );
                file.write_all(&bytes[..n])?;
                offset += n as u64;
                anyhow::ensure!(
                    size == 0 || offset <= size,
                    "Download exceeds publisher size"
                );
                self.log(format!(
                    "Downloading {name}: {} / {} MiB",
                    offset / 1024 / 1024,
                    size / 1024 / 1024
                ));
            }
        }
        anyhow::ensure!(
            size == 0 || offset == size,
            "Download incomplete; retry to resume"
        );
        if hash(&partial)? != sha.to_lowercase() {
            fs::remove_file(&partial)?;
            anyhow::bail!("Checksum mismatch; corrupt download discarded");
        }
        fs::rename(partial, &target)?;
        Ok(target)
    }
    pub fn install(&self, s: Settings, runtime: bool, model: bool, encoder: bool, ocr: bool) {
        self.cancel.store(false, Ordering::Relaxed);
        let owner = self.clone();
        std::thread::spawn(move || {
            let result = owner
                .install_inner(&s, runtime, model, encoder, ocr)
                .map_err(|e| e.to_string());
            let _ = owner.tx.send(Event::Setup(result));
        });
    }
    fn install_inner(
        &self,
        s: &Settings,
        runtime: bool,
        model: bool,
        encoder: bool,
        ocr: bool,
    ) -> Result<Vec<(String, String)>> {
        let mut paths = vec![];
        let result = (|| -> Result<()> {
            if runtime {
                let releases = self.catalog(
                    "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10",
                    s,
                )?;
                let backend = if s.hardware == "CPU / system RAM" {
                    "cpu"
                } else if s.device.to_uppercase().starts_with("CUDA") {
                    "cuda-"
                } else {
                    "vulkan"
                };
                let release = releases
                    .as_array()
                    .context("Invalid runtime catalog")?
                    .iter()
                    .find(|r| {
                        r["assets"].as_array().is_some_and(|a| {
                            a.iter().any(|x| {
                                x["name"].as_str().is_some_and(|n| {
                                    n.contains(&format!("bin-win-{backend}"))
                                        && n.ends_with("-x64.zip")
                                })
                            })
                        })
                    })
                    .context("No compatible Windows runtime found")?;
                let assets = release["assets"].as_array().unwrap();
                let asset = assets
                    .iter()
                    .find(|x| {
                        x["name"].as_str().is_some_and(|n| {
                            n.starts_with("llama-")
                                && n.contains(&format!("bin-win-{backend}"))
                                && n.ends_with("-x64.zip")
                        })
                    })
                    .context("Runtime asset missing")?;
                let version = release["tag_name"]
                    .as_str()
                    .context("Runtime version missing")?;
                anyhow::ensure!(
                    version
                        .chars()
                        .all(|x| x.is_ascii_alphanumeric() || x == '-' || x == '_'),
                    "Unsafe runtime version"
                );
                let target = self
                    .home
                    .join("runtimes")
                    .join(format!("{version}-{backend}"));
                fs::create_dir_all(&target)?;
                let mut bundle = vec![asset];
                if backend == "cuda-" {
                    let name = asset["name"].as_str().unwrap();
                    let companion = name
                        .strip_prefix(&format!("llama-{version}-"))
                        .map(|tail| format!("cudart-llama-{tail}"));
                    if let Some(companion) = companion {
                        if let Some(a) = assets.iter().find(|x| x["name"] == companion) {
                            bundle.push(a);
                        }
                    }
                }
                for a in bundle {
                    let zip = self.download(
                        a["browser_download_url"]
                            .as_str()
                            .context("Runtime URL missing")?,
                        a["name"].as_str().unwrap(),
                        a["size"].as_u64().unwrap_or(0),
                        a["digest"]
                            .as_str()
                            .unwrap_or("")
                            .trim_start_matches("sha256:"),
                        s,
                    )?;
                    unzip(&zip, &target)?;
                }
                let server = walkdir::WalkDir::new(&target)
                    .into_iter()
                    .filter_map(|x| x.ok())
                    .find(|x| x.file_name() == "llama-server.exe")
                    .context("Runtime lacks llama-server.exe")?
                    .path()
                    .to_path_buf();
                if let Some(parent) = server.parent() {
                    for entry in walkdir::WalkDir::new(&target)
                        .into_iter()
                        .filter_map(|x| x.ok())
                    {
                        if entry.path().extension().is_some_and(|x| x == "dll")
                            && entry.path().parent() != Some(parent)
                        {
                            let destination = parent.join(entry.file_name());
                            if !destination.exists() {
                                fs::copy(entry.path(), destination)?;
                            }
                        }
                    }
                }
                paths.push(("server".into(), server.to_string_lossy().into_owned()));
            }
            if model || encoder {
                let metadata = self.catalog(
                    "https://huggingface.co/api/models/ggml-org/embeddinggemma-2-GGUF?blobs=true",
                    s,
                )?;
                let revision = metadata["sha"].as_str().context("Model revision missing")?;
                anyhow::ensure!(
                    revision.chars().all(|x| x.is_ascii_hexdigit()),
                    "Invalid pinned revision"
                );
                let items = metadata["siblings"]
                    .as_array()
                    .context("Model files missing")?;
                for (prefix, enabled, key) in [
                    ("embeddinggemma-2-", model, "model"),
                    ("mmproj-embeddinggemma-2-", encoder, "projector"),
                ] {
                    if !enabled {
                        continue;
                    }
                    let name = format!("{prefix}{}.gguf", s.quant);
                    let item = items
                        .iter()
                        .find(|x| x["rfilename"] == name)
                        .context("Selected quantization unavailable")?;
                    let url = format!(
                        "https://huggingface.co/ggml-org/embeddinggemma-2-GGUF/resolve/{revision}/{name}"
                    );
                    let file = self.download(
                        &url,
                        &name,
                        item["lfs"]["size"].as_u64().unwrap_or(0),
                        item["lfs"]["sha256"].as_str().unwrap_or(""),
                        s,
                    )?;
                    let target = self.home.join("models");
                    fs::create_dir_all(&target)?;
                    let installed = target.join(name);
                    fs::copy(file, &installed)?;
                    paths.push((key.into(), installed.to_string_lossy().into_owned()));
                }
            }
            if ocr {
                let mut c = std::process::Command::new("winget.exe");
                c.args([
                    "install",
                    "--id",
                    "UB-Mannheim.TesseractOCR",
                    "--exact",
                    "--silent",
                    "--accept-source-agreements",
                    "--accept-package-agreements",
                ]);
                crate::content::hidden(&mut c);
                let status = c
                    .status()
                    .context("Install Windows App Installer to use automatic OCR setup")?;
                anyhow::ensure!(
                    status.success(),
                    "OCR installation failed; select an existing tesseract.exe"
                );
                for root in [
                    std::env::var("ProgramFiles").unwrap_or_default(),
                    std::env::var("LOCALAPPDATA").unwrap_or_default() + "\\Programs",
                ] {
                    let p = PathBuf::from(root).join("Tesseract-OCR/tesseract.exe");
                    if p.exists() {
                        paths.push(("ocr".into(), p.to_string_lossy().into_owned()));
                        break;
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            if !paths.is_empty() {
                let _ = self.tx.send(Event::Setup(Ok(paths)));
            }
            return Err(e);
        }
        Ok(paths)
    }
    pub fn uninstall(&self) -> Result<()> {
        let home = self.home.canonicalize()?;
        for folder in ["models", "runtimes", "downloads"] {
            let target = home.join(folder);
            if !target.exists() {
                continue;
            }
            let resolved = target.canonicalize()?;
            anyhow::ensure!(
                resolved.starts_with(&home) && resolved != home,
                "Uninstall path is outside managed downloads"
            );
            fs::remove_dir_all(target)?;
        }
        Ok(())
    }
}
fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut bytes = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        hasher.update(&bytes[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
fn unzip(path: &Path, target: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry
            .enclosed_name()
            .context("Unsafe runtime archive path")?;
        anyhow::ensure!(
            !entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000),
            "Runtime archive contains a symlink"
        );
        total += entry.size();
        anyhow::ensure!(
            total <= 4 * 1024 * 1024 * 1024,
            "Runtime archive expansion limit"
        );
        let out = target.join(name);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            fs::create_dir_all(out.parent().unwrap())?;
            std::io::copy(&mut entry, &mut fs::File::create(out)?)?;
        }
    }
    Ok(())
}
