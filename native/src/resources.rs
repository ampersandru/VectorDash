use crate::config::Settings;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
static DISK: std::sync::OnceLock<std::sync::Mutex<Option<(PathBuf, std::time::Instant, u64)>>> =
    std::sync::OnceLock::new();
pub fn invalidate_storage() {
    if let Some(cache) = DISK.get() {
        *cache.lock().unwrap() = None;
    }
}
#[derive(Clone, Default, Debug)]
pub struct Usage {
    pub ram: u64,
    pub vram: Option<u64>,
    pub disk: u64,
    pub shared: bool,
}
fn bytes(path: &Path) -> u64 {
    if path.is_file() {
        return std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    }
    walkdir::WalkDir::new(path)
        .follow_links(false)
        .max_depth(8)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}
pub fn paths(home: &Path, s: &Settings, key: &str) -> Vec<PathBuf> {
    match key {
        "runtime" => {
            let p = PathBuf::from(&s.server);
            if let Some(base) = owned_roots(home).into_iter().find(|base| {
                crate::config::under(
                    &p.to_string_lossy(),
                    &base.join("runtimes").to_string_lossy(),
                )
            }) {
                let relative = p.strip_prefix(base.join("runtimes")).unwrap_or(&p);
                relative
                    .components()
                    .next()
                    .map(|c| vec![base.join("runtimes").join(c.as_os_str())])
                    .unwrap_or_default()
            } else {
                vec![p]
            }
        }
        "embedding" => vec![PathBuf::from(&s.model)],
        "encoder" => vec![PathBuf::from(&s.projector)],
        "ocr" => {
            let p = PathBuf::from(&s.ocr);
            if let Some(base) = owned_roots(home).into_iter().find(|base| {
                crate::config::under(
                    &p.to_string_lossy(),
                    &base.join("runtimes").to_string_lossy(),
                )
            }) {
                let relative = p.strip_prefix(base.join("runtimes")).unwrap_or(&p);
                relative
                    .components()
                    .next()
                    .map(|c| vec![base.join("runtimes").join(c.as_os_str())])
                    .unwrap_or_default()
            } else {
                vec![p]
            }
        }
        "dictation" => vec![
            crate::dictation::model_root(home, s),
            crate::dictation::runtime_root(home, s),
        ],
        "ffmpeg" => {
            let p = crate::content::ffmpeg(s)
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(&s.ffmpeg));
            let mut files = vec![p.clone()];
            if let Some(parent) = p.parent() {
                for name in ["ffprobe.exe", "ffplay.exe"] {
                    let file = parent.join(name);
                    if file.is_file() {
                        files.push(file);
                    }
                }
            }
            files
        }
        "raw" => crate::raw::helper()
            .ok()
            .and_then(|p| p.parent().map(|p| vec![p.to_path_buf()]))
            .unwrap_or_default(),
        "pdf" => std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|r| vec![r.join("pdfium.dll")]))
            .unwrap_or_default(),
        _ => vec![],
    }
}
pub fn sample(home: &Path, s: &Settings, inference_pid: u32) -> HashMap<String, Usage> {
    let mut result = HashMap::new();
    let (_, pids) = crate::platform::owned_usage();
    let gpu = vram();
    let infer = ram(inference_pid);
    let voice_pid = crate::dictation::pid();
    for key in [
        "runtime",
        "embedding",
        "encoder",
        "ocr",
        "dictation",
        "ffmpeg",
        "pdf",
        "raw",
    ] {
        let mut item = Usage {
            disk: paths(home, s, key).iter().map(|p| bytes(p)).sum(),
            ..Default::default()
        };
        if ["runtime", "embedding", "encoder"].contains(&key) {
            item.ram = infer;
            item.vram = gpu.as_ref().map(|m| *m.get(&inference_pid).unwrap_or(&0));
            item.shared = true;
        } else if key == "dictation" {
            item.ram = ram(voice_pid);
            item.vram = gpu.as_ref().map(|m| *m.get(&voice_pid).unwrap_or(&0));
        } else if key == "pdf" {
            item.shared = true;
            item.ram = 0;
            item.vram = Some(0);
        } else {
            for pid in &pids {
                let name = process_name(*pid).to_ascii_lowercase();
                if key == "raw" && name == "vectordash-raw.exe"
                    || key == "ocr" && name == "tesseract.exe"
                    || key == "ffmpeg"
                        && (name == "ffmpeg.exe" || name == "ffprobe.exe" || name == "ffplay.exe")
                {
                    item.ram += ram(*pid);
                    if let Some(g) = &gpu {
                        item.vram = Some(item.vram.unwrap_or(0) + g.get(pid).copied().unwrap_or(0));
                    }
                }
            }
            if item.vram.is_none() && gpu.is_some() {
                item.vram = Some(0);
            }
        }
        result.insert(key.into(), item);
    }
    let mut summary = Usage {
        ram: pids.iter().map(|p| ram(*p)).sum(),
        vram: gpu
            .as_ref()
            .map(|g| pids.iter().map(|p| g.get(p).copied().unwrap_or(0)).sum()),
        ..Default::default()
    };
    // Cache disk totals: hardware polling must not traverse the entire index folder every few seconds.
    let mut cache = DISK.get_or_init(Default::default).lock().unwrap();
    if let Some((path, at, size)) = &*cache {
        if path == home && at.elapsed() < std::time::Duration::from_secs(60) {
            summary.disk = *size;
        }
    }
    if summary.disk == 0 {
        let mut roots = vec![home.to_path_buf()];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                if !crate::config::under(&exe.to_string_lossy(), &home.to_string_lossy()) {
                    for file in [
                        "VectorDash.exe",
                        "vectordash.exe",
                        "raw-decoder",
                        "ffmpeg.exe",
                        "ffplay.exe",
                        "ffprobe.exe",
                        "pdfium.dll",
                    ] {
                        let path = dir.join(file);
                        if path.exists() {
                            roots.push(path);
                        }
                    }
                }
            }
        }
        if inference_pid != 0 {
            for key in ["embedding", "encoder", "runtime"] {
                for path in paths(home, s, key) {
                    if !crate::config::under(&path.to_string_lossy(), &home.to_string_lossy()) {
                        roots.push(path);
                    }
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        summary.disk = roots
            .iter()
            .filter(|p| seen.insert(crate::config::fold(p)))
            .map(|p| bytes(p))
            .sum();
        *cache = Some((home.into(), std::time::Instant::now(), summary.disk));
    }
    result.insert("total".into(), summary);
    result
}
#[cfg(windows)]
fn ram(pid: u32) -> u64 {
    if pid == 0 {
        return 0;
    }
    use windows::Win32::{
        Foundation::CloseHandle,
        System::{ProcessStatus::*, Threading::*},
    };
    unsafe {
        let Ok(p) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) else {
            return 0;
        };
        let mut info = PROCESS_MEMORY_COUNTERS::default();
        let ok = GetProcessMemoryInfo(p, &mut info, std::mem::size_of_val(&info) as u32).is_ok();
        let _ = CloseHandle(p);
        if ok { info.WorkingSetSize as u64 } else { 0 }
    }
}
#[cfg(not(windows))]
fn ram(_: u32) -> u64 {
    0
}
#[cfg(windows)]
fn process_name(pid: u32) -> String {
    use windows::Win32::{Foundation::CloseHandle, System::Threading::*};
    unsafe {
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 32768];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            p,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut n,
        )
        .is_ok();
        let _ = CloseHandle(p);
        if ok {
            String::from_utf16_lossy(&buf[..n as usize])
                .split('\\')
                .next_back()
                .unwrap_or("")
                .into()
        } else {
            String::new()
        }
    }
}
#[cfg(not(windows))]
fn process_name(_: u32) -> String {
    String::new()
}
#[cfg(windows)]
fn vram() -> Option<HashMap<u32, u64>> {
    use windows::Win32::System::Performance::*;
    unsafe {
        let mut query = PDH_HQUERY::default();
        if PdhOpenQueryW(None, 0, &mut query) != 0 {
            return None;
        }
        struct Guard(PDH_HQUERY);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe {
                    PdhCloseQuery(self.0);
                }
            }
        }
        let _guard = Guard(query);
        let mut counter = PDH_HCOUNTER::default();
        let name = "\\GPU Process Memory(*)\\Dedicated Usage"
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        if PdhAddEnglishCounterW(query, windows::core::PCWSTR(name.as_ptr()), 0, &mut counter) != 0
            || PdhCollectQueryData(query) != 0
        {
            return None;
        }
        let (mut size, mut count) = (0u32, 0u32);
        PdhGetFormattedCounterArrayW(counter, PDH_FMT_LARGE, &mut size, &mut count, None);
        if size == 0 || size > 8 * 1024 * 1024 {
            return Some(HashMap::new());
        }
        let mut data = vec![0u64; (size as usize + 7) / 8];
        if PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_LARGE,
            &mut size,
            &mut count,
            Some(data.as_mut_ptr().cast()),
        ) != 0
        {
            return None;
        }
        let items = std::slice::from_raw_parts(
            data.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
            count as usize,
        );
        let mut map = HashMap::new();
        for x in items {
            if x.FmtValue.CStatus <= 1 {
                let name = x.szName.to_string().unwrap_or_default();
                if let Some(pid) = name
                    .strip_prefix("pid_")
                    .and_then(|v| v.split('_').next())
                    .and_then(|v| v.parse::<u32>().ok())
                {
                    *map.entry(pid).or_insert(0) += x.FmtValue.Anonymous.largeValue.max(0) as u64;
                }
            }
        }
        Some(map)
    }
}
#[cfg(not(windows))]
fn vram() -> Option<HashMap<u32, u64>> {
    None
}
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / 1024f64.powi(3))
    } else {
        format!("{:.0} MB", bytes as f64 / 1024f64.powi(2))
    }
}
fn owned_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![home.to_path_buf()];
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let parent = PathBuf::from(local);
        if crate::config::fold(home) == crate::config::fold(parent.join("VectorDash-Rust")) {
            roots.push(parent.join("VectorDash"));
        }
    }
    roots
}
fn allowed_download(home: &Path, path: &Path) -> bool {
    owned_roots(home).iter().any(|root| {
        ["models", "runtimes", "tools"].iter().any(|folder| {
            let dir = root.join(folder);
            let cleaned = path
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_owned();
            let path = Path::new(&cleaned);
            crate::config::under(&path.to_string_lossy(), &dir.to_string_lossy())
                && crate::config::fold(path) != crate::config::fold(&dir)
        })
    })
}
pub fn managed(home: &Path, s: &Settings, key: &str) -> bool {
    let targets = paths(home, s, key);
    !targets.is_empty() && targets.iter().all(|p| allowed_download(home, p))
}
pub fn has_downloads(home: &Path, s: &Settings) -> bool {
    [
        "runtime",
        "embedding",
        "encoder",
        "ocr",
        "dictation",
        "ffmpeg",
    ]
    .iter()
    .any(|key| managed(home, s, key) && paths(home, s, key).iter().any(|p| p.exists()))
}
fn model_root_parent(home: &Path) -> PathBuf {
    home.join("models/parakeet")
}
pub fn uninstall_all(home: &Path) -> anyhow::Result<()> {
    let targets = owned_roots(home)
        .into_iter()
        .flat_map(|r| ["models", "runtimes", "tools", "downloads"].map(|f| r.join(f)))
        .filter(|p| p.exists())
        .collect::<Vec<_>>();
    for target in &targets {
        let parent = target.parent().unwrap().canonicalize()?;
        let resolved = target.canonicalize()?;
        anyhow::ensure!(
            resolved.starts_with(&parent) && resolved != parent,
            "Managed folder resolves outside the app data directory"
        );
    }
    for target in targets {
        std::fs::remove_dir_all(target)?;
    }
    Ok(())
}
pub fn uninstall(home: &Path, s: &Settings, key: &str) -> anyhow::Result<()> {
    invalidate_storage();
    if key == "models" || key == "runtimes" {
        let folder = if key == "models" {
            "models"
        } else {
            "runtimes"
        };
        let targets = owned_roots(home)
            .into_iter()
            .map(|r| r.join(folder))
            .filter(|p| p.exists())
            .collect::<Vec<_>>();
        for target in &targets {
            let parent = target.parent().unwrap().canonicalize()?;
            let resolved = target.canonicalize()?;
            anyhow::ensure!(
                resolved == parent.join(folder),
                "Managed folder resolves outside its expected location"
            );
        }
        for target in targets {
            std::fs::remove_dir_all(target)?;
        }
        return Ok(());
    }
    if key == "embedding-models" {
        for item in ["embedding", "encoder"] {
            if managed(home, s, item) {
                uninstall(home, s, item)?;
            }
        }
        return Ok(());
    }
    if key == "dictation-model" {
        let targets = vec![model_root_parent(home)];
        for target in targets {
            if target.exists() {
                let root = home.canonicalize()?.join("models");
                let resolved = target.canonicalize()?;
                anyhow::ensure!(
                    resolved.starts_with(&root) && resolved != root,
                    "Model path escapes managed directory"
                );
                std::fs::remove_dir_all(target)?;
            }
        }
        return Ok(());
    }

    let targets = paths(home, s, key);
    anyhow::ensure!(!targets.is_empty(), "No managed installation found");
    for path in &targets {
        anyhow::ensure!(
            allowed_download(home, path),
            "This item is external or bundled; manage it with its installer"
        );
        if path.exists() {
            let resolved = path.canonicalize()?;
            anyhow::ensure!(
                owned_roots(home).iter().any(|root| {
                    root.canonicalize().is_ok_and(|root| {
                        ["models", "runtimes", "tools"].iter().any(|folder| {
                            let dir = root.join(folder);
                            resolved.starts_with(&dir) && resolved != dir
                        })
                    })
                }),
                "Download resolves outside a managed folder"
            );
        }
    }
    for path in targets {
        if path.is_dir() {
            std::fs::remove_dir_all(path)?
        } else if path.is_file() {
            std::fs::remove_file(path)?
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_downloads_are_managed_but_external_files_are_not() {
        let local = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap());
        let home = local.join("VectorDash-Rust");
        let s = Settings {
            model: local
                .join("VectorDash/models/test.gguf")
                .to_string_lossy()
                .into_owned(),
            ..Default::default()
        };
        assert!(managed(&home, &s, "embedding"));
        let outside = Settings {
            model: local
                .join("Downloads/test.gguf")
                .to_string_lossy()
                .into_owned(),
            ..Default::default()
        };
        assert!(!managed(&home, &outside, "embedding"));
    }
    #[test]
    fn uninstall_is_scoped_to_owned_downloads() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let private = outside.path().join("weights.gguf");
        std::fs::write(&private, "keep").unwrap();
        let mut s = Settings {
            model: private.to_string_lossy().into_owned(),
            ..Default::default()
        };
        assert!(uninstall(home.path(), &s, "embedding").is_err());
        assert!(private.exists());
        std::fs::create_dir_all(home.path().join("models")).unwrap();
        let model = home.path().join("models/weights.gguf");
        std::fs::write(&model, "fixture").unwrap();
        s.model = model.to_string_lossy().into_owned();
        uninstall(home.path(), &s, "embedding").unwrap();
        assert!(!model.exists());
    }
}

#[cfg(test)]
mod removal_group_tests {
    use super::*;
    #[test]
    fn models_and_runtimes_are_removed_independently() {
        let home = tempfile::tempdir().unwrap();
        for dir in ["models/parakeet", "runtimes/parakeet", "tools"] {
            std::fs::create_dir_all(home.path().join(dir)).unwrap();
            std::fs::write(home.path().join(dir).join("keep.bin"), b"fixture").unwrap();
        }
        uninstall(home.path(), &Settings::default(), "models").unwrap();
        assert!(!home.path().join("models").exists());
        assert!(home.path().join("runtimes/parakeet/keep.bin").exists());
        uninstall(home.path(), &Settings::default(), "runtimes").unwrap();
        assert!(!home.path().join("runtimes").exists());
        assert!(home.path().join("tools/keep.bin").exists());
    }
}
