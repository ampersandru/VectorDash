use crate::config::Settings;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
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
            if p.starts_with(home.join("runtimes")) {
                let relative = p.strip_prefix(home.join("runtimes")).unwrap();
                relative
                    .components()
                    .next()
                    .map(|c| vec![home.join("runtimes").join(c.as_os_str())])
                    .unwrap_or_default()
            } else {
                vec![p]
            }
        }
        "embedding" => vec![PathBuf::from(&s.model)],
        "encoder" => vec![PathBuf::from(&s.projector)],
        "ocr" => {
            let p = PathBuf::from(&s.ocr);
            if p.starts_with(home.join("runtimes")) {
                let relative = p.strip_prefix(home.join("runtimes")).unwrap();
                relative
                    .components()
                    .next()
                    .map(|c| vec![home.join("runtimes").join(c.as_os_str())])
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
                if key == "ocr" && name == "tesseract.exe"
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
pub fn uninstall(home: &Path, s: &Settings, key: &str) -> anyhow::Result<()> {
    let root = home.canonicalize()?;
    let targets = paths(&root, s, key);
    anyhow::ensure!(!targets.is_empty(), "No managed installation found");
    for path in &targets {
        if !path.exists() {
            continue;
        }
        let resolved = path.canonicalize()?;
        anyhow::ensure!(
            (resolved.starts_with(root.join("models"))
                || resolved.starts_with(root.join("runtimes")))
                && resolved != root.join("models")
                && resolved != root.join("runtimes"),
            "This item is external or bundled; manage it with its installer"
        );
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
