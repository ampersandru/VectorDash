use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

static RECORDING: AtomicBool = AtomicBool::new(false);
pub fn recording() -> bool {
    RECORDING.load(Ordering::Acquire)
}
struct Recording;
impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.store(false, Ordering::Release);
    }
}

static ACTIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static LEVEL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static WAVE: std::sync::OnceLock<Mutex<[f32; 64]>> = std::sync::OnceLock::new();
pub fn waveform() -> [f32; 64] {
    WAVE.get_or_init(|| Mutex::new([0.0; 64]))
        .try_lock()
        .map(|x| *x)
        .unwrap_or([0.0; 64])
}
pub fn in_use() -> bool {
    ACTIVE.load(Ordering::Acquire) > 0
}
pub fn pid() -> u32 {
    PID.load(Ordering::Acquire)
}
pub fn level() -> f32 {
    f32::from_bits(LEVEL.load(Ordering::Relaxed))
}
struct Activity;
impl Activity {
    fn new() -> Self {
        ACTIVE.fetch_add(1, Ordering::AcqRel);
        Self
    }
}
impl Drop for Activity {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
        LEVEL.store(0, Ordering::Relaxed);
        if let Ok(mut wave) = WAVE.get_or_init(|| Mutex::new([0.0; 64])).lock() {
            *wave = [0.0; 64];
        }
    }
}
struct ProcessMarker;
impl Drop for ProcessMarker {
    fn drop(&mut self) {
        PID.store(0, Ordering::Release);
    }
}
pub fn microphones() -> Result<Vec<String>> {
    let mut names = cpal::default_host()
        .input_devices()?
        .filter_map(|d| d.name().ok())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}
pub fn quant(s: &crate::config::Settings) -> &str {
    match s.dictation_quant.as_str() {
        "fp16" => "fp16",
        "8-bit" => "int8",
        _ => {
            if s.dictation_device == "NVIDIA CUDA" {
                "fp16"
            } else {
                "int8"
            }
        }
    }
}
#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    url: String,
    size: u64,
    sha: String,
}
fn assets() -> Vec<Asset> {
    serde_json::from_str(include_str!("../assets/parakeet-downloads.json"))
        .expect("speech manifest")
}
fn find(root: &Path, name: &str) -> Option<PathBuf> {
    walkdir::WalkDir::new(root)
        .max_depth(5)
        .into_iter()
        .filter_map(Result::ok)
        .find(|e| e.file_type().is_file() && e.file_name().to_string_lossy() == name)
        .map(|e| e.into_path())
}
pub fn ready(home: &Path, s: &crate::config::Settings) -> bool {
    let device = s.dictation_device.as_str();
    runtime(home, device).is_some_and(|exe| {
        exe.parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("lib/sherpa-onnx-c-api.dll")
            .is_file()
    }) && (device != "NVIDIA CUDA"
        || home.join("runtimes/parakeet/cuda/installed.txt").is_file()
            && find(&home.join("runtimes/parakeet/cuda"), "cublasLt64_12.dll").is_some()
            && find(&home.join("runtimes/parakeet/cuda"), "cudnn64_9.dll").is_some())
        && model(home, quant(s), "encoder").is_some()
        && model(home, quant(s), "decoder").is_some()
        && model(home, quant(s), "joiner").is_some()
        && model(home, quant(s), "tokens").is_some()
}
fn runtime(home: &Path, device: &str) -> Option<PathBuf> {
    find(
        &home.join(format!(
            "runtimes/parakeet/{}",
            if device == "NVIDIA CUDA" {
                "cuda"
            } else {
                "cpu"
            }
        )),
        "sherpa-onnx-offline.exe",
    )
}
fn model(home: &Path, quant: &str, part: &str) -> Option<PathBuf> {
    let root = home.join(format!("models/parakeet/{quant}"));
    if part == "tokens" {
        return find(&root, "tokens.txt");
    }
    find(&root, &format!("{part}.{quant}.onnx")).or_else(|| find(&root, &format!("{part}.onnx")))
}
pub fn files(home: &Path, s: &crate::config::Settings) -> Vec<PathBuf> {
    ["encoder", "decoder", "joiner", "tokens"]
        .iter()
        .filter_map(|p| model(home, quant(s), p))
        .collect()
}
pub fn runtime_root(home: &Path, s: &crate::config::Settings) -> PathBuf {
    home.join(format!(
        "runtimes/parakeet/{}",
        if s.dictation_device == "NVIDIA CUDA" {
            "cuda"
        } else {
            "cpu"
        }
    ))
}
pub fn model_root(home: &Path, s: &crate::config::Settings) -> PathBuf {
    home.join(format!("models/parakeet/{}", quant(s)))
}
pub fn install(setup: &crate::setup::Setup, s: &crate::config::Settings) -> Result<()> {
    let gpu = s.dictation_device == "NVIDIA CUDA";
    if ready(&setup.home, s) {
        return Ok(());
    }
    let list = assets();
    for (asset, folder) in [
        (
            list.iter()
                .find(|a| {
                    if gpu {
                        a.name.contains("cuda-12")
                    } else {
                        a.name.contains("Release-no-tts")
                    }
                })
                .context("Missing speech runtime manifest")?,
            setup.home.join(format!(
                "runtimes/parakeet/{}",
                if gpu { "cuda" } else { "cpu" }
            )),
        ),
        (
            list.iter()
                .find(|a| {
                    if quant(s) == "fp16" {
                        a.url.contains("fp16.tar")
                    } else {
                        a.name.contains("int8.tar")
                    }
                })
                .context("Missing speech model manifest")?,
            setup.home.join(format!("models/parakeet/{}", quant(s))),
        ),
    ] {
        let file = setup.download(&asset.url, &asset.name, asset.size, &asset.sha, s)?;
        std::fs::create_dir_all(&folder)?;
        let marker = folder.join("archive.sha256");
        let complete = if asset.name.contains("sherpa-onnx-v") {
            runtime(&setup.home, &s.dictation_device).is_some_and(|exe| {
                exe.parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("lib/sherpa-onnx-c-api.dll")
                    .is_file()
            })
        } else {
            ["encoder", "decoder", "joiner", "tokens"]
                .iter()
                .all(|part| model(&setup.home, quant(s), part).is_some())
        };
        if !complete || std::fs::read_to_string(&marker).ok().as_deref() != Some(asset.sha.as_str())
        {
            let _ = setup.tx.send(crate::engine::Event::Log(
                "Info".into(),
                format!("Installing optional dictation artifact: {}", asset.name),
            ));
            extract(&file, &folder, &setup.cancel)?;
            std::fs::write(marker, &asset.sha)?;
        }
    }
    if gpu {
        let bin = runtime(&setup.home, &s.dictation_device)
            .context("Missing CUDA speech runtime")?
            .parent()
            .unwrap()
            .to_owned();
        for asset in list.iter().filter(|a| a.name.ends_with(".zip")) {
            let file = setup.download(&asset.url, &asset.name, asset.size, &asset.sha, s)?;
            let mut zip = zip::ZipArchive::new(std::fs::File::open(file)?)?;
            for i in 0..zip.len() {
                anyhow::ensure!(!setup.cancel.load(Ordering::Acquire), "Download cancelled");
                let mut entry = zip.by_index(i)?;
                let path = entry.enclosed_name().context("Unsafe CUDA archive path")?;
                let name = path
                    .file_name()
                    .context("Missing CUDA filename")?
                    .to_string_lossy()
                    .into_owned();
                if name.ends_with(".dll") {
                    anyhow::ensure!(entry.size() < 1024 * 1024 * 1024, "CUDA library too large");
                    std::io::copy(&mut entry, &mut std::fs::File::create(bin.join(name))?)?;
                } else if name.to_ascii_lowercase().contains("license") {
                    let destination = setup
                        .home
                        .join("runtimes/parakeet/cuda/licenses")
                        .join(&asset.name);
                    std::fs::create_dir_all(&destination)?;
                    std::io::copy(
                        &mut entry,
                        &mut std::fs::File::create(destination.join(name))?,
                    )?;
                }
            }
        }
    }
    if gpu {
        std::fs::write(
            setup.home.join("runtimes/parakeet/cuda/installed.txt"),
            "sherpa-onnx 1.13.8 + CUDA 12.9 + cuDNN 9.10; verified complete",
        )?;
    }
    anyhow::ensure!(ready(&setup.home, s), "Incomplete dictation installation");
    Ok(())
}
fn extract(file: &Path, root: &Path, cancel: &AtomicBool) -> Result<()> {
    let file = std::fs::File::open(file)?;
    let mut archive = tar::Archive::new(bzip2::read::BzDecoder::new(file));
    let mut total = 0u64;
    for item in archive.entries()? {
        anyhow::ensure!(!cancel.load(Ordering::Acquire), "Download cancelled");
        let mut entry = item?;
        let path = entry.path()?.into_owned();
        anyhow::ensure!(
            path.components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            "Unsafe archive path"
        );
        let kind = entry.header().entry_type();
        anyhow::ensure!(
            kind.is_file() || kind.is_dir(),
            "Links are not allowed in speech downloads"
        );
        total += entry.size();
        anyhow::ensure!(
            total < 5 * 1024 * 1024 * 1024,
            "Speech archive exceeds installation limit"
        );
        anyhow::ensure!(entry.unpack_in(root)?, "Archive escaped destination");
    }
    Ok(())
}
fn resample(input: &[f32], channels: usize, rate: u32) -> Vec<i16> {
    let frames = input.len() / channels;
    let count = (frames as u64 * 16000 / rate as u64) as usize;
    (0..count)
        .map(|i| {
            let position = i as f64 * rate as f64 / 16000.0;
            let a = position as usize;
            let b = (a + 1).min(frames.saturating_sub(1));
            let f = (position - a as f64) as f32;
            let mono = |j: usize| {
                input[j * channels..j * channels + channels]
                    .iter()
                    .sum::<f32>()
                    / channels as f32
            };
            ((mono(a) * (1.0 - f) + mono(b) * f).clamp(-1.0, 1.0) * 32767.0) as i16
        })
        .collect()
}
#[derive(Default)]
struct Endpoint {
    noise: f32,
    speech: bool,
    voiced: f32,
    last_voice: f32,
}
impl Endpoint {
    fn update(&mut self, rms: f32, elapsed: f32, delay: f32) -> bool {
        let threshold = (self.noise * 3.0).clamp(0.003, 0.03);
        if rms > threshold {
            self.voiced += 0.025;
            self.last_voice = elapsed;
            if self.voiced >= 0.15 {
                self.speech = true;
            }
        } else if !self.speech {
            self.noise = self.noise * 0.95 + rms * 0.05;
        }
        self.speech && elapsed - self.last_voice >= delay.clamp(0.6, 5.0)
    }
}
struct WarmStart {
    handle: Option<std::thread::JoinHandle<Result<()>>>,
    cancel: Arc<AtomicBool>,
}
impl Drop for WarmStart {
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.cancel.store(true, Ordering::Release);
        }
    }
}
pub fn run(
    home: &Path,
    s: &crate::config::Settings,
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    mut state: impl FnMut(&str),
) -> Result<String> {
    anyhow::ensure!(
        s.remote_dictation() || ready(home, s),
        "Download Parakeet dictation in Models and AI first"
    );
    anyhow::ensure!(
        !s.remote_dictation() || !s.dictation_model.trim().is_empty(),
        "Select a remote dictation model in Models and AI"
    );
    let _activity = Activity::new();
    let host = cpal::default_host();
    let device = if s.microphone.is_empty() {
        host.default_input_device().context("No microphone found")?
    } else {
        host.input_devices()?
            .find(|d| d.name().ok().as_deref() == Some(s.microphone.as_str()))
            .context("Selected microphone is unavailable; choose one in Settings")?
    };
    let microphone = device.name().unwrap_or_else(|_| "microphone".into());
    let supported = device.default_input_config()?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    anyhow::ensure!(
        config.channels > 0 && config.sample_rate.0 > 0,
        "Invalid microphone format"
    );
    let limit = config.sample_rate.0 as usize * config.channels as usize * 60;
    let samples = Arc::new(Mutex::new(Vec::<f32>::with_capacity(limit.min(48000))));
    let error = Arc::new(Mutex::new(None));
    let fault = error.clone();
    let err = move |e: cpal::StreamError| {
        *fault.lock().unwrap() = Some(e.to_string());
    };
    let data = samples.clone();
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |v: &[f32], _| {
                let mut x = data.lock().unwrap();
                let n = v.len().min(limit.saturating_sub(x.len()));
                x.extend_from_slice(&v[..n]);
            },
            err,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |v: &[i16], _| {
                let mut x = data.lock().unwrap();
                let n = v.len().min(limit.saturating_sub(x.len()));
                x.extend(v[..n].iter().map(|x| *x as f32 / 32768.0));
            },
            err,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config,
            move |v: &[u16], _| {
                let mut x = data.lock().unwrap();
                let n = v.len().min(limit.saturating_sub(x.len()));
                x.extend(v[..n].iter().map(|x| (*x as f32 - 32768.0) / 32768.0));
            },
            err,
            None,
        )?,
        _ => anyhow::bail!("Unsupported microphone sample format"),
    };
    let warm_home = home.to_path_buf();
    let warm_settings = s.clone();
    let warm_cancel = cancel.clone();
    let mut warm = WarmStart {
        cancel: cancel.clone(),
        handle: if s.remote_dictation() {
            None
        } else {
            Some(std::thread::spawn(move || {
                let _activity = Activity::new();
                preload(&warm_home, &warm_settings, &warm_cancel)
            }))
        },
    };
    activation_sound();
    stream.play()?;
    RECORDING.store(true, Ordering::Release);
    let recording = Recording;
    state("Recording");
    let started = Instant::now();
    let mut endpoint = Endpoint::default();
    let mut last_samples = 0usize;
    while !stop.load(Ordering::Acquire)
        && !cancel.load(Ordering::Acquire)
        && started.elapsed() < Duration::from_secs(60)
    {
        std::thread::sleep(Duration::from_millis(25));
        if let Ok(x) = samples.try_lock() {
            let tail = &x[x
                .len()
                .saturating_sub(config.sample_rate.0 as usize * config.channels as usize / 10)..];
            let peak = tail.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
            LEVEL.store(peak.min(1.0).to_bits(), Ordering::Relaxed);
            if let Ok(mut wave) = WAVE.get_or_init(|| Mutex::new([0.0; 64])).try_lock() {
                let chunk = tail.len().div_ceil(64).max(1);
                *wave = [0.0; 64];
                for (i, part) in tail.chunks(chunk).take(64).enumerate() {
                    wave[i] = part.iter().map(|v| v.abs()).fold(0.0f32, f32::max).min(1.0);
                }
            }

            if x.len() > last_samples {
                let rms = if tail.is_empty() {
                    0.0
                } else {
                    (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt()
                };
                last_samples = x.len();
                if s.dictation_auto_finish
                    && endpoint.update(
                        rms,
                        started.elapsed().as_secs_f32(),
                        s.dictation_silence_seconds,
                    )
                {
                    state("Speech ended · transcribing automatically…");
                    break;
                }
            }
        }
        if let Some(e) = error.lock().unwrap().take() {
            anyhow::bail!("Microphone error: {e}");
        }
    }
    drop(stream);
    drop(recording);
    LEVEL.store(0, Ordering::Relaxed);
    *WAVE.get_or_init(|| Mutex::new([0.0; 64])).lock().unwrap() = [0.0; 64];
    anyhow::ensure!(!cancel.load(Ordering::Acquire), "Dictation cancelled");
    let audible = samples
        .lock()
        .unwrap()
        .iter()
        .any(|v| v.is_finite() && v.abs() > 0.001);
    anyhow::ensure!(
        audible,
        "No audible input from {microphone}. Select an active microphone in Settings."
    );
    let audio = resample(
        &samples.lock().unwrap(),
        config.channels as usize,
        config.sample_rate.0,
    );
    anyhow::ensure!(audio.len() >= 1600, "Record at least a tenth of a second");
    let temp = tempfile::tempdir()?;
    let wav = temp.path().join("dictation.wav");
    let mut writer = hound::WavWriter::create(
        &wav,
        hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    for sample in audio {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    state("Transcribing…");
    if s.remote_dictation() {
        return crate::remote::transcribe(s, &wav, &cancel);
    }
    warm.handle
        .take()
        .unwrap()
        .join()
        .map_err(|_| anyhow::anyhow!("Parakeet loader stopped"))??;
    transcribe(home, s, &wav, &cancel)
}

struct Retained {
    child: std::process::Child,
    _job: crate::platform::Job,
    input: std::process::ChildStdin,
    output: std::sync::mpsc::Receiver<serde_json::Value>,
    last: Instant,
    key: String,
}
impl Drop for Retained {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        PID.store(0, Ordering::Release);
    }
}
fn retained() -> &'static Mutex<Option<Retained>> {
    static WORKER: std::sync::OnceLock<Mutex<Option<Retained>>> = std::sync::OnceLock::new();
    WORKER.get_or_init(|| Mutex::new(None))
}
pub fn stop() {
    if let Ok(mut worker) = retained().try_lock() {
        worker.take();
    }
}
fn idle_unload(s: &crate::config::Settings) {
    if in_use() {
        return;
    }
    if let Ok(mut worker) = retained().try_lock() {
        if worker.as_ref().is_some_and(|w| {
            !s.dictation_keep_loaded
                || w.last.elapsed()
                    >= Duration::from_secs(s.dictation_unload_minutes.max(1) as u64 * 60)
                || crate::scheduling::resource_pause(s, crate::scheduling::metrics()).is_some()
        }) {
            worker.take();
        }
    }
}
pub fn start_monitor(settings: Arc<Mutex<crate::config::Settings>>, shutdown: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        while !shutdown.load(Ordering::Acquire) {
            let s = settings.lock().unwrap().clone();
            idle_unload(&s);
            std::thread::sleep(Duration::from_secs(1));
        }
        stop();
    });
}
fn spawn_worker(home: &Path, s: &crate::config::Settings, key: String) -> Result<Retained> {
    use std::{
        io::BufRead,
        process::{Command, Stdio},
    };
    let exe = runtime(home, &s.dictation_device).context("Missing Parakeet runtime")?;
    let bin = exe.parent().context("Missing speech runtime folder")?;
    let lib = bin.parent().unwrap_or(bin).join("lib");
    let config = crate::speech_ffi::WorkerConfig {
        library: lib
            .join("sherpa-onnx-c-api.dll")
            .to_string_lossy()
            .into_owned(),
        encoder: model(home, quant(s), "encoder")
            .context("Missing encoder")?
            .to_string_lossy()
            .into_owned(),
        decoder: model(home, quant(s), "decoder")
            .context("Missing decoder")?
            .to_string_lossy()
            .into_owned(),
        joiner: model(home, quant(s), "joiner")
            .context("Missing joiner")?
            .to_string_lossy()
            .into_owned(),
        tokens: model(home, quant(s), "tokens")
            .context("Missing tokens")?
            .to_string_lossy()
            .into_owned(),
        provider: if s.dictation_device == "NVIDIA CUDA" {
            "cuda"
        } else {
            "cpu"
        }
        .into(),
    };
    let mut cmd = Command::new(std::env::current_exe()?);
    #[cfg(not(test))]
    cmd.arg("--speech-worker");
    #[cfg(test)]
    cmd.args([
        "--ignored",
        "--exact",
        "dictation::integration_tests::speech_worker_helper",
        "--nocapture",
    ]);
    let mut paths = vec![bin.to_path_buf(), lib];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    cmd.env("PATH", std::env::join_paths(paths)?)
        .env("VECTORDASH_SPEECH_CONFIG", serde_json::to_string(&config)?)
        .current_dir(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(home.join("dictation.log"))?,
        );
    crate::content::hidden(&mut cmd);
    let mut child = cmd.spawn().context("Could not start Parakeet worker")?;
    let job = match crate::platform::owned_process(&child) {
        Ok(j) => j,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    PID.store(child.id(), Ordering::Release);
    let input = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, output) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if line.len() > 1024 * 1024 {
                break;
            }
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if tx.send(value).is_err() {
                    break;
                }
            }
        }
    });
    Ok(Retained {
        child,
        _job: job,
        input,
        output,
        last: Instant::now(),
        key,
    })
}
fn wait_reply(w: &mut Retained, cancel: &AtomicBool) -> Result<serde_json::Value> {
    let start = Instant::now();
    loop {
        anyhow::ensure!(
            !cancel.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(180),
            "Dictation cancelled or timed out"
        );
        match w.output.recv_timeout(Duration::from_millis(25)) {
            Ok(value) => return Ok(value),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("Parakeet worker stopped; see Model logs")
            }
            _ => {
                anyhow::ensure!(
                    w.child.try_wait()?.is_none(),
                    "Parakeet worker exited; see dictation.log in Model logs"
                );
            }
        }
    }
}
pub fn preload(home: &Path, s: &crate::config::Settings, cancel: &AtomicBool) -> Result<()> {
    let key = format!("{}|{}|{}", home.display(), s.dictation_device, quant(s));
    let mut lock = retained().lock().unwrap();
    if lock.as_ref().is_some_and(|w| w.key != key) {
        lock.take();
    }
    if lock.is_none() {
        let mut worker = spawn_worker(home, s, key)?;
        let ready = wait_reply(&mut worker, cancel)?;
        anyhow::ensure!(ready["ready"] == true, "Parakeet did not become ready");
        *lock = Some(worker);
    }
    Ok(())
}
fn activation_wave() -> Vec<u8> {
    let mut data = std::io::Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 24000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::new(&mut data, spec).unwrap();
    for n in 0..5280 {
        let t = n as f32 / 24000.0;
        let envelope = (t / 0.012).min(1.0) * (1.0 - t / 0.22).max(0.0).powf(2.0);
        let tone = (std::f32::consts::TAU * 523.25 * t).sin() * 0.09
            + (std::f32::consts::TAU * 659.25 * t).sin() * 0.045;
        writer
            .write_sample((tone * envelope * 32767.0) as i16)
            .unwrap();
    }
    writer.finalize().unwrap();
    data.into_inner()
}
fn activation_sound() {
    #[cfg(windows)]
    unsafe {
        use windows::{Win32::Media::Audio::*, core::PCWSTR};
        static SOUND: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        let sound = SOUND.get_or_init(activation_wave);
        // Finish the brief cue before capture so it does not trigger speech endpointing.
        let _ = PlaySoundW(
            PCWSTR(sound.as_ptr().cast()),
            None,
            SND_MEMORY | SND_SYNC | SND_NODEFAULT | SND_NOSTOP,
        );
    }
}
pub fn transcribe(
    home: &Path,
    s: &crate::config::Settings,
    wav: &Path,
    cancel: &AtomicBool,
) -> Result<String> {
    use std::io::Write;
    let key = format!("{}|{}|{}", home.display(), s.dictation_device, quant(s));
    let mut lock = retained().lock().unwrap();
    if lock.as_ref().is_some_and(|w| w.key != key) {
        lock.take();
    }
    let result = (|| -> Result<String> {
        if lock.is_none() {
            let mut w = spawn_worker(home, s, key)?;
            let ready = wait_reply(&mut w, cancel)?;
            anyhow::ensure!(ready["ready"] == true, "Parakeet did not become ready");
            *lock = Some(w);
        }
        let worker = lock.as_mut().unwrap();
        writeln!(
            worker.input,
            "{}",
            serde_json::json!({"wav":wav.to_string_lossy()})
        )?;
        worker.input.flush()?;
        let response = wait_reply(worker, cancel)?;
        if let Some(error) = response["error"].as_str() {
            anyhow::bail!("{error}")
        }
        worker.last = Instant::now();
        parse_output(&response.to_string())
    })();
    if result.is_err() || !s.dictation_keep_loaded {
        lock.take();
    }
    result
}
fn parse_output(text: &str) -> Result<String> {
    for (start, _) in text.match_indices('{').rev() {
        let mut iter =
            serde_json::Deserializer::from_str(&text[start..]).into_iter::<serde_json::Value>();
        if let Some(Ok(v)) = iter.next() {
            if let Some(s) = v["text"].as_str() {
                if !s.trim().is_empty() {
                    return Ok(s.trim().to_owned());
                }
            }
        }
    }
    anyhow::bail!(
        "Parakeet heard no recognizable speech. Check the microphone level and try again."
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_and_audio() {
        assert_eq!(
            parse_output("logs\n{\"text\":\" Hello world. \",\"tokens\":[]}\n").unwrap(),
            "Hello world."
        );
        assert!(parse_output("failed").is_err());
        let y = resample(&vec![0.5; 48000 * 2], 2, 48000);
        assert_eq!(y.len(), 16000);
        assert!(y.iter().all(|v| *v == 16383));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    #[test]
    #[ignore]
    fn speech_worker_helper() {
        crate::speech_ffi::worker().unwrap();
    }
    #[test]
    #[ignore = "Uses explicitly supplied installed Parakeet fixtures"]
    fn retained_model_reuses_process_then_unloads_when_idle() {
        let home = PathBuf::from(std::env::var_os("VECTORDASH_SPEECH_TEST_HOME").unwrap());
        let s = crate::config::Settings {
            dictation_device: std::env::var("VECTORDASH_SPEECH_TEST_DEVICE")
                .unwrap_or("CPU".into()),
            dictation_keep_loaded: true,
            dictation_unload_minutes: 1,
            ..Default::default()
        };
        let wav = walkdir::WalkDir::new(home.join("models/parakeet"))
            .into_iter()
            .filter_map(Result::ok)
            .find(|e| e.path().extension().is_some_and(|x| x == "wav"))
            .unwrap()
            .into_path();
        preload(&home, &s, &AtomicBool::new(false)).unwrap();
        let preload_pid = super::pid();
        assert!(preload_pid > 0);
        let first = transcribe(&home, &s, &wav, &AtomicBool::new(false)).unwrap();
        let pid = super::pid();
        assert_eq!(pid, preload_pid);
        assert!(pid > 0);
        let start = Instant::now();
        let second = transcribe(&home, &s, &wav, &AtomicBool::new(false)).unwrap();
        assert_eq!(first, second);
        assert_eq!(super::pid(), pid);
        println!(
            "Retained transcription {:?}; reused PID {}",
            start.elapsed(),
            pid
        );
        retained().lock().unwrap().as_mut().unwrap().last =
            Instant::now() - Duration::from_secs(61);
        idle_unload(&s);
        assert_eq!(super::pid(), 0);
        assert!(retained().lock().unwrap().is_none());
    }
    #[test]
    #[ignore = "Downloads optional Parakeet artifacts into an explicitly supplied test folder"]
    fn publisher_model_transcribes_fixture_and_exits() {
        let home = PathBuf::from(
            std::env::var_os("VECTORDASH_SPEECH_TEST_HOME").expect("explicit fixture home"),
        );
        std::fs::create_dir_all(&home).unwrap();
        let (tx, _rx) = crossbeam_channel::unbounded();
        let setup = crate::setup::Setup::new(home.clone(), tx.into());
        let s = crate::config::Settings {
            dictation_device: std::env::var("VECTORDASH_SPEECH_TEST_DEVICE")
                .unwrap_or("CPU".into()),
            ..Default::default()
        };
        install(&setup, &s).unwrap();
        let wav = walkdir::WalkDir::new(home.join("models/parakeet"))
            .into_iter()
            .filter_map(Result::ok)
            .find(|e| e.path().extension().is_some_and(|x| x == "wav"))
            .expect("publisher test recording")
            .into_path();
        let started = Instant::now();
        let text = transcribe(&home, &s, &wav, &AtomicBool::new(false)).unwrap();
        assert!(!text.is_empty());
        println!(
            "Native Parakeet {} fixture: {:?}, {} transcript characters; child process exited",
            s.dictation_device,
            started.elapsed(),
            text.len()
        );
    }
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    #[test]
    fn pauses_only_finish_after_detected_speech() {
        let mut e = Endpoint::default();
        for i in 0..100 {
            assert!(!e.update(0.0, i as f32 * 0.025, 1.2));
        }
        for i in 100..120 {
            assert!(!e.update(0.05, i as f32 * 0.025, 1.2));
        }
        assert!(!e.update(0.0, 3.3, 1.2));
        assert!(e.update(0.0, 4.3, 1.2));
    }
}

#[cfg(test)]
mod activation_tests {
    #[test]
    fn cue_is_short_and_quiet() {
        let bytes = super::activation_wave();
        let mut wav = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
        assert!(wav.duration() <= 6000);
        assert_eq!(wav.spec().sample_rate, 24000);
        assert!(wav.samples::<i16>().all(|n| n.unwrap().abs() < 5000));
    }
}

/// Strip sentence-ending ASR punctuation without changing punctuation inside names or queries.
pub fn query_text(text: &str) -> String {
    text.trim()
        .trim_end_matches(|c: char| c.is_whitespace() || ".!?,;:…。！？，；：".contains(c))
        .to_owned()
}
#[cfg(test)]
mod query_text_tests {
    #[test]
    fn trims_only_sentence_endings_and_keeps_filename_math_and_internal_punctuation() {
        for (spoken, query) in [
            ("Find dogs.", "Find dogs"),
            (" What is this?! ", "What is this"),
            ("report.v2.pdf...", "report.v2.pdf"),
            ("dog's photo, from 2025.", "dog's photo, from 2025"),
            ("report (final).", "report (final)"),
            ("12.5%.", "12.5%"),
            ("C:\\Downloads\\", "C:\\Downloads\\"),
            ("...", ""),
        ] {
            assert_eq!(super::query_text(spoken), query);
        }
    }
}
