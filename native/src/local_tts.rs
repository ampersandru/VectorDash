//! Optional offline speech runtime. The child and its models share an owned job.
use crate::{config::Settings, setup::Setup};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
static HOME: OnceLock<PathBuf> = OnceLock::new();
static WANTED: AtomicBool = AtomicBool::new(false);
static EXITING: AtomicBool = AtomicBool::new(false);
static INSTALLING: AtomicBool = AtomicBool::new(false);
static STATE: OnceLock<Mutex<Option<Runtime>>> = OnceLock::new();
fn state() -> &'static Mutex<Option<Runtime>> {
    STATE.get_or_init(Default::default)
}
pub fn configure(home: &Path) {
    if HOME.set(home.to_owned()).is_ok() {
        std::thread::spawn(|| {
            while !EXITING.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_secs(1));
                tick();
            }
        });
    }
}
fn root() -> Result<PathBuf> {
    Ok(HOME
        .get()
        .context("Speech storage is not initialized")?
        .join("runtimes/local-tts"))
}
pub const MODELS: [(&str, &str); 5] = [
    ("paradee-v2f_lite", "Paradee v2f lite"),
    ("paradee-v2f", "Paradee v2f"),
    ("paradee-v2g", "Paradee v2g"),
    ("kokoro-fp16", "Kokoro 82M FP16"),
    ("kokoro-q8", "Kokoro 82M Q8"),
];
pub const VOICES: [&str; 4] = ["af_heart", "af_bella", "am_adam", "am_michael"];
#[derive(Deserialize)]
struct Asset {
    id: String,
    name: String,
    url: String,
    size: u64,
    sha: String,
}
fn install_signature() -> &'static str {
    static SIGNATURE: OnceLock<String> = OnceLock::new();
    SIGNATURE.get_or_init(|| {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"windows-local-tts-loader-v1");
        h.update(include_bytes!("../assets/local-tts/worker.mjs"));
        h.update(include_bytes!("../assets/local-tts/package-lock.json"));
        h.update(include_bytes!("../assets/local-tts/downloads.json"));
        format!("{:x}", h.finalize())
    })
}
fn assets() -> Vec<Asset> {
    serde_json::from_str(include_str!("../assets/local-tts/downloads.json"))
        .expect("speech manifest")
}
fn find(root: &Path, name: &str) -> Option<PathBuf> {
    walkdir::WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_map(Result::ok)
        .find(|e| e.file_type().is_file() && e.file_name() == name)
        .map(|e| e.into_path())
}
fn validate(s: &Settings) -> Result<()> {
    ensure!(
        MODELS.iter().any(|(id, _)| *id == s.tts_local_model),
        "Choose a supported local speech model"
    );
    ensure!(
        VOICES.contains(&s.tts_local_voice.as_str()),
        "Choose a supported local voice"
    );
    ensure!(
        matches!(s.tts_local_device.as_str(), "CPU" | "GPU (DirectML)"),
        "Choose CPU or GPU (DirectML)"
    );
    ensure!(
        !s.tts_local_model.starts_with("paradee-") || s.tts_local_device == "CPU",
        "Paradee PDN uses optimized CPU SIMD; use Kokoro for GPU speech"
    );
    Ok(())
}
pub fn ready(home: &Path, s: &Settings) -> bool {
    let r = home.join("runtimes/local-tts");
    validate(s).is_ok()
        && find(&r.join("node"), "node.exe").is_some()
        && r.join("node_modules/phonemizer/dist/phonemizer.js")
            .is_file()
        && fs::read_to_string(r.join("installed.txt")).ok().as_deref() == Some(install_signature())
        && r.join("paradee/paradee-web-v2/models/v2g.pdn").is_file()
        && (!s.tts_local_model.starts_with("kokoro-")
            || r.join("kokoro")
                .join(if s.tts_local_model == "kokoro-fp16" {
                    "model_fp16.onnx"
                } else {
                    "model_quantized.onnx"
                })
                .is_file())
}
fn extract_zip(file: &Path, root: &Path, cancel: &AtomicBool) -> Result<()> {
    fs::create_dir_all(root)?;
    let mut z = zip::ZipArchive::new(fs::File::open(file)?)?;
    let mut total = 0u64;
    for i in 0..z.len() {
        ensure!(!cancel.load(Ordering::Acquire), "Speech setup cancelled");
        let mut e = z.by_index(i)?;
        let rel = e.enclosed_name().context("Unsafe speech archive path")?;
        ensure!(!e.is_symlink(), "Speech archive symlinks are not allowed");
        total = total.saturating_add(e.size());
        ensure!(total < 650 * 1024 * 1024, "Speech archive too large");
        let p = root.join(rel);
        if e.is_dir() {
            fs::create_dir_all(p)?;
        } else {
            fs::create_dir_all(p.parent().unwrap())?;
            std::io::copy(&mut e, &mut fs::File::create(p)?)?;
        }
    }
    Ok(())
}
pub fn install(setup: &Setup, s: &Settings) -> Result<()> {
    validate(s)?;
    ensure!(
        !INSTALLING.swap(true, Ordering::AcqRel),
        "Local speech setup is already running"
    );
    struct Installing;
    impl Drop for Installing {
        fn drop(&mut self) {
            INSTALLING.store(false, Ordering::Release);
        }
    }
    let _installing = Installing;
    unload_idle()?;

    let r = setup.home.join("runtimes/local-tts");
    fs::create_dir_all(&r)?;
    let _ = fs::remove_file(r.join("installed.txt"));
    for a in assets() {
        let selected = a.id == "node"
            || a.id == "paradee"
            || (s.tts_local_model.starts_with("kokoro-")
                && (a.id == s.tts_local_model || VOICES.contains(&a.id.as_str())));
        if !selected {
            continue;
        }
        let source = setup.download(&a.url, &a.name, a.size, &a.sha, s)?;
        match a.id.as_str() {
            "node" => extract_zip(&source, &r.join("node"), &setup.cancel)?,
            "paradee" => extract_zip(&source, &r.join("paradee"), &setup.cancel)?,
            _ => {
                fs::create_dir_all(r.join("kokoro"))?;
                fs::copy(source, r.join("kokoro").join(a.name))?;
            }
        }
    }
    fs::write(
        r.join("package.json"),
        include_str!("../assets/local-tts/package.json"),
    )?;
    fs::write(
        r.join("package-lock.json"),
        include_str!("../assets/local-tts/package-lock.json"),
    )?;
    fs::write(
        r.join("worker.mjs"),
        include_str!("../assets/local-tts/worker.mjs"),
    )?;
    let node = find(&r.join("node"), "node.exe").context("Local speech Node runtime is missing")?;
    let npm = node
        .parent()
        .unwrap()
        .join("node_modules/npm/bin/npm-cli.js");
    let mut cmd = Command::new(&node);
    cmd.arg(npm)
        .args([
            "ci",
            "--ignore-scripts",
            "--omit=dev",
            "--no-audit",
            "--no-fund",
        ])
        .current_dir(&r)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::content::hidden(&mut cmd);
    let mut child = cmd
        .spawn()
        .context("Could not install optional speech libraries")?;
    let _job = crate::platform::owned_process(&child)?;
    let started = Instant::now();
    loop {
        if setup.cancel.load(Ordering::Acquire) || started.elapsed() > Duration::from_secs(300) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Optional speech library setup cancelled or timed out");
        }
        if let Some(status) = child.try_wait()? {
            ensure!(
                status.success(),
                "Optional speech library setup failed; check network access and retry"
            );
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // Windows file URL compatibility fixes to the pinned upstream bundle.
    for rel in ["paradee.mjs", "engine/pdn.mjs"] {
        let p = r.join("paradee/paradee-web-v2").join(rel);
        let text = fs::read_to_string(&p)?
            .replace("decodeURIComponent(u.pathname)", "fileURLToPath(u)")
            .replace(
                "new URL(relaxed ? \"pdn_r.wasm\" : \"pdn.wasm\", here).pathname",
                "fileURLToPath(new URL(relaxed ? \"pdn_r.wasm\" : \"pdn.wasm\", here))",
            );
        fs::write(
            p,
            format!("import {{fileURLToPath}} from 'node:url';\n{text}"),
        )?;
    }
    let p = r.join("paradee/paradee-web-v2/frontend/kokoro-phonemize.mjs");
    fs::write(
        &p,
        fs::read_to_string(&p)?.replace(
            "from \"phonemizer\"",
            "from \"../../../node_modules/phonemizer/dist/phonemizer.js\"",
        ),
    )?;
    fs::write(r.join("installed.txt"), install_signature())?;
    fs::write(
        r.join("VectorDash-NOTICE.txt"),
        "Optional offline speech: Node publisher license in node/; ONNX Runtime and phonemizer notices in node_modules/; Paradee licenses in paradee/paradee-web-v2/LICENSES.md. Phonemizer includes eSpeak NG, GPL-3.0-or-later; source https://github.com/espeak-ng/espeak-ng . Windows file-URL and local-import loader changes by VectorDash.",
    )?;
    ensure!(ready(&setup.home, s), "Incomplete local speech setup");
    Ok(())
}
struct Runtime {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<String>,
    signature: String,
    expires: Option<Instant>,
    _job: crate::platform::Job,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn request(s: &Settings, op: &str, text: &str, cancel: &AtomicBool) -> Result<Value> {
    validate(s)?;
    ensure!(
        !INSTALLING.load(Ordering::Acquire),
        "Local speech setup is in progress; retry after it completes"
    );
    ensure!(
        !EXITING.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire),
        "Local speech stopped"
    );
    let mut slot = state()
        .lock()
        .map_err(|_| anyhow::anyhow!("Speech worker unavailable"))?;
    let signature = format!(
        "{}|{}|{}",
        s.tts_local_model, s.tts_local_device, s.tts_local_voice
    );
    if slot.as_ref().is_some_and(|r| r.signature != signature) {
        slot.take();
    }
    if slot.is_none() {
        let r = root()?;
        ensure!(
            ready(HOME.get().unwrap(), s),
            "Download the selected local speech model in Models and AI first"
        );
        let mut cmd =
            Command::new(find(&r.join("node"), "node.exe").context("Speech runtime missing")?);
        cmd.arg(r.join("worker.mjs"))
            .arg(&r)
            .current_dir(&r)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::content::hidden(&mut cmd);
        let mut child = cmd.spawn()?;
        let job = match crate::platform::owned_process(&child) {
            Ok(j) => j,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let input = child.stdin.take().context("Speech input missing")?;
        let stdout = child.stdout.take().context("Speech output missing")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader
                    .by_ref()
                    .take(48 * 1024 * 1024 + 1)
                    .read_line(&mut line)
                {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if line.len() > 48 * 1024 * 1024 || tx.send(line).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        *slot = Some(Runtime {
            child,
            input,
            output: rx,
            signature,
            expires: None,
            _job: job,
        });
    }
    let r = slot.as_mut().unwrap();
    r.expires = None;
    let result = (|| -> Result<Value> {
        writeln!(
            r.input,
            "{}",
            json!({"op":op,"model":s.tts_local_model,"device":s.tts_local_device,"voice":s.tts_local_voice,"text":text})
        )?;
        r.input.flush()?;
        let started = Instant::now();
        loop {
            ensure!(
                !EXITING.load(Ordering::Acquire)
                    && (!cancel.load(Ordering::Acquire)
                        || (op == "synth" && s.tts_unload_minutes > 0)),
                "Local speech stopped"
            );
            ensure!(
                op != "prepare" || WANTED.load(Ordering::Acquire) || s.tts_unload_minutes > 0,
                "Local speech preload released"
            );
            ensure!(
                started.elapsed() < Duration::from_secs(120),
                "Local speech generation timed out"
            );
            match r.output.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    let value: Value = serde_json::from_str(&line)?;
                    if let Some(e) = value["error"].as_str() {
                        anyhow::bail!("Local speech: {e}");
                    }
                    return Ok(value);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => anyhow::bail!("Local speech worker exited"),
            }
        }
    })();
    if result.is_err() {
        slot.take();
    }
    result
}
pub fn preload(s: Settings, tx: crate::engine::NotifySender) {
    if !s.tts_local || !s.tts_enabled {
        return;
    }
    WANTED.store(true, Ordering::Release);
    std::thread::spawn(move || {
        if !WANTED.load(Ordering::Acquire) {
            return;
        }
        if let Err(e) = request(&s, "prepare", "", &AtomicBool::new(false)) {
            if WANTED.load(Ordering::Acquire) {
                let _ = tx.send(crate::engine::Event::Log("Warning".into(), e.to_string()));
            }
        }
        if !WANTED.load(Ordering::Acquire) {
            release(&s);
        }
    });
}
pub fn release(s: &Settings) {
    WANTED.store(false, Ordering::Release);
    if let Ok(mut slot) = state().try_lock() {
        if s.tts_unload_minutes == 0 {
            slot.take();
        } else if let Some(r) = slot.as_mut() {
            r.expires = Some(
                Instant::now()
                    + Duration::from_secs(u64::from(s.tts_unload_minutes.min(1440)) * 60),
            );
        }
    }
}
pub fn tick() {
    if WANTED.load(Ordering::Acquire) {
        return;
    }
    if let Ok(mut slot) = state().try_lock() {
        if slot
            .as_ref()
            .is_some_and(|r| r.expires.is_some_and(|t| Instant::now() >= t))
        {
            slot.take();
        }
    }
}
pub fn unload_idle() -> Result<()> {
    WANTED.store(false, Ordering::Release);
    let mut slot = state().try_lock().map_err(|_| {
        anyhow::anyhow!("Local speech is busy; stop read aloud before changing its files")
    })?;
    slot.take();
    Ok(())
}
pub fn loaded() -> bool {
    state().try_lock().map(|s| s.is_some()).unwrap_or(true)
}
pub fn shutdown() {
    EXITING.store(true, Ordering::Release);
    WANTED.store(false, Ordering::Release);
    if let Ok(mut s) = state().try_lock() {
        s.take();
    }
}
pub fn synthesize(s: &Settings, text: &str, cancel: &Arc<AtomicBool>) -> Result<Vec<u8>> {
    use base64::Engine;
    let result = request(s, "synth", text, cancel).and_then(|v| {
        ensure!(!cancel.load(Ordering::Acquire), "Read aloud stopped");
        let data = v["audio"]
            .as_str()
            .context("Local speech returned no audio")?;
        ensure!(
            data.len() < 44 * 1024 * 1024,
            "Local speech output too large"
        );
        let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
        ensure!(
            bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"),
            "Invalid local speech WAV"
        );
        Ok(bytes)
    });
    release(s);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_options_do_not_mutate_remote_configuration() {
        let mut s = Settings::default();
        s.tts_url = "https://tts.example/v1".into();
        s.tts_key = "secret".into();
        s.tts_model = "remote".into();
        s.tts_voice = "voice".into();
        s.tts_local = true;
        assert!(validate(&s).is_ok());
        let home = tempfile::tempdir().unwrap();
        s.save(home.path()).unwrap();
        let mut s = Settings::load(home.path()).unwrap();
        assert!(s.tts_local);
        s.tts_local = false;
        assert_eq!(s.tts_url, "https://tts.example/v1");
        assert_eq!(s.tts_key, "secret");
        assert_eq!(s.tts_model, "remote");
        assert_eq!(s.tts_voice, "voice");
    }
    #[test]
    fn paradee_does_not_claim_gpu_support() {
        let s = Settings {
            tts_local_device: "GPU (DirectML)".into(),
            ..Default::default()
        };
        assert!(validate(&s).is_err());
    }
    #[test]
    fn manifest_is_pinned() {
        for a in assets() {
            assert_eq!(a.sha.len(), 64);
            assert!(a.size > 0 && a.url.starts_with("https://"));
        }
    }
    #[test]
    #[ignore = "downloads optional pinned speech files and generates real local audio"]
    fn live_optional_speech_all_models_and_unload() {
        let home = PathBuf::from(
            std::env::var("VECTORDASH_TTS_TEST_HOME")
                .expect("explicit disposable speech test home required"),
        );
        assert!(home.is_absolute());
        configure(&home);
        let (tx, _rx) = crossbeam_channel::unbounded();
        let tx = crate::engine::NotifySender::new(tx, None);
        let setup = Setup::new(home.clone(), tx);
        let mut s = Settings {
            tts_local: true,
            ..Default::default()
        };
        let cancel = Arc::new(AtomicBool::new(false));
        for model in [
            "paradee-v2f_lite",
            "paradee-v2f",
            "paradee-v2g",
            "kokoro-q8",
            "kokoro-fp16",
        ] {
            s.tts_local_model = model.into();
            install(&setup, &s).unwrap();
            let audio = synthesize(
                &s,
                "VectorDash can read your answers aloud. Andrew is testing local speech.",
                &cancel,
            )
            .unwrap();
            assert!(audio.len() > 24000);
            assert!(!loaded(), "immediate unload failed");
            fs::write(home.join(format!("{model}.wav")), audio).unwrap();
            println!("PASS real speech and immediate unload: {model}");
        }
        s.tts_local_model = "paradee-v2g".into();
        preload(s.clone(), setup.tx.clone());
        for _ in 0..100 {
            if loaded() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(loaded(), "preload did not start");
        release(&s);
        for _ in 0..200 {
            if !loaded() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!loaded(), "unchecking read aloud did not release preload");
        s.tts_unload_minutes = 1;
        let audio = synthesize(&s, "The idle unload timer is active.", &cancel).unwrap();
        assert!(audio.len() > 24000 && loaded());
        if let Some(r) = state().lock().unwrap().as_mut() {
            r.expires = Some(Instant::now() - Duration::from_secs(1));
        }
        tick();
        assert!(!loaded(), "idle unload failed");
        for minutes in [1, 0] {
            s.tts_unload_minutes = minutes;
            let selected = s.clone();
            let cancelled = Arc::new(AtomicBool::new(false));
            let flag = cancelled.clone();
            let pending = std::thread::spawn(move || {
                synthesize(
                    &selected,
                    &"This generation is cancelled before any audio is played. ".repeat(50),
                    &flag,
                )
            });
            std::thread::sleep(Duration::from_millis(50));
            cancelled.store(true, Ordering::Release);
            assert!(pending.join().unwrap().is_err());
            assert_eq!(loaded(), minutes > 0, "cancel did not respect retention");
        }
        // Explicit GPU tests: neither a different model nor silent CPU retry is allowed.
        s.tts_local_device = "GPU (DirectML)".into();
        s.tts_unload_minutes = 0;
        for model in ["kokoro-q8", "kokoro-fp16"] {
            s.tts_local_model = model.into();
            let audio = synthesize(
                &s,
                "This speech uses the Windows graphics provider.",
                &cancel,
            )
            .unwrap();
            assert!(audio.len() > 24000);
            assert!(!loaded());
            fs::write(home.join(format!("{model}-directml.wav")), audio).unwrap();
            println!("PASS real DirectML speech and immediate unload: {model}");
        }
        shutdown();
        assert!(!loaded());
    }
}
