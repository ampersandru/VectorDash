use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Source {
    pub path: String,
    pub mode: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub roots: Vec<Source>,
    pub exclusions: Vec<String>,
    pub excluded_types: String,
    pub count_files: bool,
    pub watch_changes: bool,
    pub auto_check_updates: bool,
    pub auto_update: bool,
    pub skip_windows_churn: bool,
    pub remote_url: String,
    pub remote_key: String,
    pub server: String,
    pub model: String,
    pub projector: String,
    pub device: String,
    pub hardware: String,
    pub quant: String,
    pub hf_token: String,
    pub github_token: String,
    pub encrypt_index: bool,
    pub performance: String,
    pub theme: String,
    pub accent: [u8; 3],
    pub popup_opacity: f32,
    pub unload_idle: bool,
    pub unload_minutes: u32,
    pub popup_gradient: f32,
    pub popup_tone: [u8; 3],
    pub sidebar_collapsed: bool,
    pub close_behavior: String,
    pub minimize_behavior: String,
    pub runtime_backend: String,
    pub ocr_enabled: bool,
    pub setup_runtime: bool,
    pub setup_model: bool,
    pub setup_encoder: bool,
    pub setup_ocr: bool,
    pub check_cpu: bool,
    pub check_gpu: bool,
    pub check_user_idle: bool,
    pub cpu_limit: f32,
    pub gpu_limit: f32,
    pub ac_only: bool,
    pub idle_interval_minutes: u32,
    pub schedule_days: [bool; 7],
    pub last_success: i64,
    pub last_schedule_slot: i64,
    pub schedule_enabled_at: i64,
    pub max_run_minutes: u32,
    pub schedule_history: Vec<String>,
    pub video_interval: f32,
    pub pdf_visuals: bool,
    pub visual_detail: String,
    pub default_search_mode: String,
    pub search_excluded_types: String,
    pub include_programs: bool,
    pub hotkey: String,
    pub explorer_integration: bool,
    pub explorer_scope: bool,
    pub dialog_integration: bool,
    pub type_to_search: bool,
    pub close_to_tray: bool,
    pub start_with_windows: bool,
    pub start_hidden: bool,
    pub schedule: String,
    pub schedule_hour: u32,
    pub schedule_minute: u32,
    pub idle_seconds: u32,
    pub index_images: bool,
    pub index_audio: bool,
    pub index_video: bool,
    pub code_search: bool,
    pub ocr: String,
    pub ocr_languages: String,
    pub ffmpeg: String,
    pub mcp_enabled: bool,
    pub mcp_lan: bool,
    pub mcp_auth: bool,
    pub mcp_token: String,
    pub mcp_port: u16,
    pub excluded_sensitive: bool,
    pub batch_size: usize,
    pub max_file_mib: u64,
    pub retry_failed: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            roots: vec![],
            exclusions: vec![],
            excluded_types: String::new(),
            count_files: false,
            watch_changes: false,
            auto_check_updates: true,
            auto_update: false,
            skip_windows_churn: true,
            remote_url: String::new(),
            remote_key: String::new(),
            server: String::new(),
            model: String::new(),
            projector: String::new(),
            device: String::new(),
            hardware: "CPU / system RAM".into(),
            quant: "Q8_0".into(),
            hf_token: String::new(),
            github_token: String::new(),
            encrypt_index: true,
            performance: "Balanced".into(),
            theme: "Dark".into(),
            accent: [133, 115, 255],
            popup_opacity: 0.8,
            unload_idle: true,
            unload_minutes: 5,
            popup_gradient: 0.2,
            popup_tone: [138, 115, 168],
            sidebar_collapsed: false,
            close_behavior: "Hide to tray".into(),
            minimize_behavior: "Taskbar".into(),
            runtime_backend: "Auto".into(),
            ocr_enabled: true,
            setup_runtime: true,
            setup_model: true,
            setup_encoder: true,
            setup_ocr: true,
            check_cpu: true,
            check_gpu: false,
            check_user_idle: true,
            cpu_limit: 25.0,
            gpu_limit: 20.0,
            ac_only: true,
            idle_interval_minutes: 60,
            schedule_days: [true; 7],
            last_success: 0,
            last_schedule_slot: 0,
            schedule_enabled_at: 0,
            max_run_minutes: 120,
            schedule_history: vec![],
            video_interval: 5.0,
            pdf_visuals: false,
            visual_detail: "Balanced".into(),
            default_search_mode: "Both".into(),
            search_excluded_types: String::new(),
            include_programs: true,
            hotkey: "Ctrl+Shift+Space".into(),
            explorer_integration: true,
            explorer_scope: true,
            dialog_integration: true,
            type_to_search: false,
            close_to_tray: true,
            start_with_windows: false,
            start_hidden: false,
            schedule: "Manual".into(),
            schedule_hour: 2,
            schedule_minute: 0,
            idle_seconds: 120,
            index_images: true,
            index_audio: true,
            index_video: true,
            code_search: false,
            ocr: String::new(),
            ocr_languages: "eng".into(),
            ffmpeg: String::new(),
            mcp_enabled: false,
            mcp_lan: false,
            mcp_auth: true,
            mcp_token: random_key(),
            mcp_port: 8790,
            excluded_sensitive: true,
            batch_size: 4,
            max_file_mib: 32,
            retry_failed: false,
        }
    }
}
pub fn random_key() -> String {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}
pub fn home(portable: bool) -> Result<PathBuf> {
    let root = if let Ok(p) = std::env::var("VECTORDASH_DATA_HOME") {
        PathBuf::from(p)
    } else if portable
        || std::env::current_exe()?
            .with_file_name("portable.flag")
            .exists()
    {
        std::env::current_exe()?
            .parent()
            .context("Executable directory")?
            .join("data")
    } else {
        PathBuf::from(std::env::var("LOCALAPPDATA")?).join("VectorDash-Rust")
    };
    fs::create_dir_all(&root)?;
    Ok(root)
}
pub fn fold(p: impl AsRef<Path>) -> String {
    p.as_ref()
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}
pub fn under(p: &str, root: &str) -> bool {
    let p = fold(p);
    let r = fold(root);
    p == r || p.starts_with(&(r + "\\"))
}
pub fn excluded_type(path: &Path, spec: &str) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    spec.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .any(|token| {
            let pattern = token.to_lowercase();
            if pattern.contains(['*', '?']) {
                wildcard_match(&name, &pattern)
            } else {
                ext == pattern.trim_start_matches('.')
            }
        })
}
fn wildcard_match(value: &str, pattern: &str) -> bool {
    let (v, p) = (value.as_bytes(), pattern.as_bytes());
    let (mut i, mut j, mut star, mut retry) = (0, 0, None, 0);
    while i < v.len() {
        if j < p.len() && (p[j] == b'?' || p[j] == v[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == b'*' {
            star = Some(j);
            j += 1;
            retry = i;
        } else if let Some(s) = star {
            retry += 1;
            i = retry;
            j = s + 1;
        } else {
            return false;
        }
    }
    while j < p.len() && p[j] == b'*' {
        j += 1;
    }
    j == p.len()
}
pub fn full_scope(s: &Settings, scope: &str) -> bool {
    scope.is_empty()
        || mode(scope, s) == "full"
        || s.roots
            .iter()
            .any(|r| r.mode == "full" && under(&r.path, scope))
}
pub fn mode<'a>(path: &str, s: &'a Settings) -> &'a str {
    s.roots
        .iter()
        .filter(|r| under(path, &r.path))
        .max_by_key(|r| r.path.len())
        .map(|r| r.mode.as_str())
        .unwrap_or("quick")
}
impl Settings {
    pub fn load(home: &Path) -> Result<Self> {
        let file = home.join("settings.json");
        let mut s: Self = if file.exists() {
            serde_json::from_slice(&fs::read(file)?)?
        } else {
            Self::default()
        };
        for value in [
            &mut s.remote_key,
            &mut s.hf_token,
            &mut s.github_token,
            &mut s.mcp_token,
        ] {
            *value = reveal(value)?;
        }
        Ok(s)
    }
    pub fn save(&self, home: &Path) -> Result<()> {
        static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = WRITES.lock().unwrap();
        let mut s = self.clone();
        for value in [
            &mut s.remote_key,
            &mut s.hf_token,
            &mut s.github_token,
            &mut s.mcp_token,
        ] {
            *value = protect(value)?;
        }
        let temp = home.join("settings.tmp");
        fs::write(&temp, serde_json::to_vec_pretty(&s)?)?;
        fs::rename(temp, home.join("settings.json"))?;
        Ok(())
    }
    pub fn legacy(home: &Path) -> Result<Self> {
        let v: serde_json::Value = serde_json::from_slice(&fs::read(home.join("settings.json"))?)?;
        let mut s = Self::default();
        let get = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let modes: HashMap<String, String> =
            serde_json::from_value(v["source_modes"].clone()).unwrap_or_default();
        let modes = modes
            .into_iter()
            .map(|(key, value)| (fold(key), value))
            .collect::<HashMap<_, _>>();
        s.roots = v["roots"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str())
            .map(|p| Source {
                path: p.into(),
                mode: if modes.get(&fold(p)).map(String::as_str) == Some("names") {
                    "quick"
                } else {
                    "full"
                }
                .into(),
            })
            .collect();
        s.skip_windows_churn = v["exclude_windows_churn"].as_bool().unwrap_or(false);
        s.excluded_sensitive = v["exclude_sensitive"].as_bool().unwrap_or(true);
        s.code_search = v["code_enabled"].as_bool().unwrap_or(false);
        if s.code_search {
            for path in v["code_roots"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str())
            {
                if !s.roots.iter().any(|r| fold(&r.path) == fold(path)) {
                    s.roots.push(Source {
                        path: path.into(),
                        mode: "full".into(),
                    });
                }
            }
        }
        s.index_images = v["index_images"].as_bool().unwrap_or(true);
        s.index_audio = v["index_audio"].as_bool().unwrap_or(true);
        s.index_video = v["index_video"].as_bool().unwrap_or(true);
        s.exclusions = serde_json::from_value(v["excluded_folders"].clone()).unwrap_or_default();
        s.exclusions.extend(
            serde_json::from_value::<Vec<String>>(v["search_excluded_files"].clone())
                .unwrap_or_default(),
        );
        s.server = get("server");
        s.model = get("model");
        s.projector = get("projector");
        s.device = get("device");
        s.hardware = get("mode");
        if s.hardware.is_empty() {
            s.hardware = "CPU / system RAM".into();
        }
        s.remote_url = if get("server_mode") == "Remote" {
            get("remote_url")
        } else {
            String::new()
        };
        s.remote_key = reveal(&get("remote_key"))?;
        s.hf_token = reveal(&get("hf_token"))?;
        s.github_token = reveal(&get("github_token"))?;
        s.encrypt_index = v["encrypt_index"].as_bool().unwrap_or(true);
        s.ocr = get("ocr_path");
        s.ffmpeg = get("ffmpeg_path");
        s.excluded_types = get("index_excluded_types");
        s.performance = get("performance_profile");
        if s.performance.is_empty() {
            s.performance = "Balanced".into();
        }
        s.theme = get("theme");
        if s.theme.is_empty() {
            s.theme = "System".into();
        }
        let color = |key: &str, fallback: [u8; 3]| {
            let raw = get(key);
            let value = raw.trim_start_matches('#');
            if value.len() != 6 {
                return fallback;
            }
            let parsed = u32::from_str_radix(value, 16).ok();
            parsed
                .map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8])
                .unwrap_or(fallback)
        };
        s.accent = color("accent_color", s.accent);
        s.popup_tone = color("popup_tone", s.popup_tone);
        s.popup_opacity = 1.0 - v["popup_transparency"].as_f64().unwrap_or(25.0) as f32 / 100.0;
        s.popup_gradient = v["popup_gradient"].as_f64().unwrap_or(20.0) as f32 / 100.0;
        s.sidebar_collapsed = v["sidebar_collapsed"].as_bool().unwrap_or(false);
        s.start_with_windows = v["startup"].as_bool().unwrap_or(false);
        s.start_hidden = v["start_hidden"].as_bool().unwrap_or(true);
        s.close_behavior = get("close_behavior");
        if s.close_behavior.is_empty() {
            s.close_behavior = "Hide to tray".into();
        }
        s.minimize_behavior = get("minimize_behavior");
        if s.minimize_behavior.is_empty() {
            s.minimize_behavior = "Taskbar".into();
        }
        s.hotkey = get("hotkey");
        if s.hotkey.is_empty() {
            s.hotkey = "Ctrl+Shift+Space".into();
        }
        s.schedule = get("schedule_mode");
        if s.schedule.is_empty() {
            s.schedule = "Manual".into();
        }
        let clock = get("schedule_time");
        let mut parts = clock.split(':');
        s.schedule_hour = parts.next().and_then(|v| v.parse().ok()).unwrap_or(2);
        s.schedule_minute = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        s.idle_seconds = v["idle_minutes"].as_u64().unwrap_or(5) as u32 * 60;
        s.check_cpu = v["check_cpu"].as_bool().unwrap_or(true);
        s.check_gpu = v["check_gpu"].as_bool().unwrap_or(false);
        s.check_user_idle = v["check_user_idle"].as_bool().unwrap_or(true);
        s.cpu_limit = v["cpu_limit"].as_f64().unwrap_or(25.0) as f32;
        s.gpu_limit = v["gpu_limit"].as_f64().unwrap_or(20.0) as f32;
        s.ac_only = v["ac_only"].as_bool().unwrap_or(true);
        s.idle_interval_minutes = v["idle_interval"].as_u64().unwrap_or(60) as u32;
        s.max_run_minutes = v["max_run_minutes"].as_u64().unwrap_or(120) as u32;
        if let Some(days) = v["schedule_days"].as_array() {
            s.schedule_days =
                std::array::from_fn(|i| days.iter().any(|d| d.as_u64() == Some(i as u64)));
        }
        s.video_interval = v["video_interval"].as_f64().unwrap_or(3.0) as f32;
        s.pdf_visuals = v["pdf_visuals"].as_bool().unwrap_or(false);
        s.visual_detail = get("visual_detail");
        if s.visual_detail.is_empty() {
            s.visual_detail = "Balanced".into();
        }
        s.default_search_mode = get("search_mode");
        if s.default_search_mode.is_empty() {
            s.default_search_mode = "Both".into();
        }
        s.search_excluded_types = get("search_excluded_types");
        s.include_programs = v["programs_enabled"].as_bool().unwrap_or(true);
        s.mcp_enabled = v["mcp_enabled"].as_bool().unwrap_or(false);
        s.mcp_lan = v["mcp_lan"].as_bool().unwrap_or(false);
        s.mcp_auth = v["mcp_auth"].as_bool().unwrap_or(true);
        s.mcp_port = v["mcp_port"].as_u64().unwrap_or(8790) as u16;
        let old_token = get("mcp_token");
        if !old_token.is_empty() {
            s.mcp_token = reveal(&old_token)?;
        }
        Ok(s)
    }
    /// One backend decision shared by setup and the hardware UI.
    pub fn backend(&self) -> &str {
        if self.hardware == "CPU / system RAM" {
            return "cpu";
        }
        match self.runtime_backend.as_str() {
            "CUDA" => "cuda-",
            "Vulkan" => "vulkan",
            _ => {
                if self.device.to_ascii_uppercase().starts_with("CUDA")
                    || (self.device.is_empty() && self.hardware == "Dedicated GPU")
                {
                    "cuda-"
                } else {
                    "vulkan"
                }
            }
        }
    }
    pub fn runtime_compatible(&self) -> bool {
        let path = self.server.to_ascii_lowercase();
        if self.backend() == "cpu" {
            return true;
        }
        if path.contains("-cpu") {
            return false;
        }
        if path.contains("vulkan") {
            return self.backend() == "vulkan";
        }
        if path.contains("cuda") {
            return self.backend() == "cuda-";
        }
        true
    }
    pub fn allowed(&self, p: &Path) -> bool {
        if self.exclusions.iter().any(|r| {
            if r.contains(['*', '?']) {
                excluded_type(p, r)
            } else {
                under(&p.to_string_lossy(), r)
            }
        }) {
            return false;
        }
        let ext = p
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if excluded_type(p, &self.excluded_types) {
            return false;
        }
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if name.starts_with('.')
            || [
                ".git",
                ".ssh",
                ".aws",
                "node_modules",
                "__pycache__",
                "system volume information",
                "$recycle.bin",
            ]
            .contains(&name.as_str())
            || ["password", "credential", "secret"]
                .iter()
                .any(|x| name.contains(x))
            || ["pem", "key", "pfx", "kdbx"].contains(&ext.as_str())
        {
            return false;
        }
        if self.skip_windows_churn
            && ["windows", "appdata", "temp", "cache", "caches"].contains(&name.as_str())
        {
            return false;
        }
        true
    }
}
#[cfg(windows)]
pub fn crypt(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>> {
    use windows::Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        if decrypt {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )?;
        } else {
            CryptProtectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )?;
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(result)
    }
}
#[cfg(not(windows))]
pub fn crypt(_: &[u8], _: bool) -> Result<Vec<u8>> {
    anyhow::bail!("Windows account protection is required")
}
pub fn protect(value: &str) -> Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    Ok(format!(
        "dpapi:{}",
        STANDARD.encode(crypt(value.as_bytes(), false)?)
    ))
}
pub fn reveal(value: &str) -> Result<String> {
    if let Some(b) = value.strip_prefix("dpapi:") {
        return Ok(String::from_utf8(crypt(&STANDARD.decode(b)?, true)?)?);
    }
    Ok(value.into())
}
pub fn read_key(home: &Path) -> Result<Vec<u8>> {
    let p = home.join("index.key");
    if p.exists() {
        let b = fs::read(p)?;
        let payload = b
            .strip_prefix(b"VDPROTECTED1\0")
            .context("Invalid account key")?;
        let key = crypt(payload, true)?;
        anyhow::ensure!(key.len() == 32, "Invalid encryption key length");
        Ok(key)
    } else {
        use rand::RngCore;
        let mut key = vec![0; 32];
        rand::rng().fill_bytes(&mut key);
        let mut out = b"VDPROTECTED1\0".to_vec();
        out.extend(crypt(&key, false)?);
        fs::write(home.join("index.key.tmp"), out)?;
        fs::rename(home.join("index.key.tmp"), p)?;
        Ok(key)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_source_wins() {
        let s = Settings {
            roots: vec![
                Source {
                    path: "C:\\".into(),
                    mode: "quick".into(),
                },
                Source {
                    path: "C:\\Documents".into(),
                    mode: "full".into(),
                },
            ],
            ..Default::default()
        };
        assert_eq!(mode("C:\\Documents\\a.txt", &s), "full");
        assert_eq!(mode("C:\\other.txt", &s), "quick");
        assert!(!under("C:\\Documents2", "C:\\Documents"));
    }
    #[test]
    fn manual_exclusions_apply_to_files_and_folders() {
        let s = Settings {
            exclusions: vec!["C:\\ignore".into(), "C:\\file.txt".into()],
            ..Default::default()
        };
        assert!(!s.allowed(Path::new("C:\\ignore\\a.txt")));
        assert!(!s.allowed(Path::new("C:\\file.txt")));
        assert!(s.allowed(Path::new("C:\\fine.txt")));
    }
}
#[cfg(test)]
mod migration_tests {
    use super::*;
    #[test]
    fn drive_modes_survive_import() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"),serde_json::json!({"roots":["C:\\","C:\\documents"],"source_modes":{"c:\\":"names","c:\\documents":"full"},"encrypt_index":false}).to_string()).unwrap();
        let s = Settings::legacy(dir.path()).unwrap();
        assert_eq!(mode("C:\\other.txt", &s), "quick");
        assert_eq!(mode("C:\\documents\\note.txt", &s), "full");
    }
}

#[cfg(test)]
mod restored_settings_tests {
    use super::*;
    #[test]
    fn nvidia_backend_not_vulkan_default() {
        let mut s = Settings::default();
        s.hardware = "Dedicated GPU".into();
        assert_eq!(s.backend(), "cuda-");
        s.device = "CUDA0".into();
        assert_eq!(s.backend(), "cuda-");
        s.runtime_backend = "Vulkan".into();
        assert_eq!(s.backend(), "vulkan");
        s.hardware = "CPU / system RAM".into();
        assert_eq!(s.backend(), "cpu");
    }
    #[test]
    fn setup_and_ocr_state_persist() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.setup_ocr = false;
        s.ocr_enabled = true;
        s.theme = "Dark".into();
        s.check_gpu = true;
        s.gpu_limit = 42.0;
        s.popup_tone = [10, 20, 30];
        s.save(dir.path()).unwrap();
        let loaded = Settings::load(dir.path()).unwrap();
        assert!(!loaded.setup_ocr);
        assert!(loaded.ocr_enabled);
        assert_eq!(loaded.theme, "Dark");
        assert!(loaded.check_gpu);
        assert_eq!(loaded.gpu_limit, 42.0);
        assert_eq!(loaded.popup_tone, [10, 20, 30]);
    }
}

#[cfg(test)]
mod exclusion_pattern_tests {
    use super::*;
    #[test]
    fn extension_and_wildcard_exclusions() {
        for spec in ["log", ".log", "*.log", "*.tmp, *.LOG"] {
            assert!(excluded_type(Path::new("report.Log"), spec));
        }
        assert!(excluded_type(Path::new("report.log.1"), "*.log.*"));
        assert!(excluded_type(Path::new("archive.tar.gz"), "*.tar.gz"));
        assert!(!excluded_type(Path::new("photo.jpg"), "*.log, .tmp"));
        assert!(!excluded_type(Path::new("catalog"), "*.log"));
    }
}
#[cfg(test)]
mod manual_exclusion_tests {
    use super::*;
    #[test]
    fn wildcard_in_manual_exclusion_list_is_effective() {
        let s = Settings {
            exclusions: vec!["*.log".into(), "*.tmp".into()],
            ..Default::default()
        };
        assert!(!s.allowed(Path::new("C:/Documents/output.log")));
        assert!(!s.allowed(Path::new("C:/Documents/temp.TMP")));
        assert!(s.allowed(Path::new("C:/Documents/note.txt")));
    }
}

pub fn same_preferences(a: &Settings, b: &Settings) -> bool {
    let mut a = a.clone();
    let mut b = b.clone();
    for s in [&mut a, &mut b] {
        s.last_success = 0;
        s.last_schedule_slot = 0;
        s.schedule_enabled_at = 0;
        s.schedule_history.clear();
    }
    a == b
}
#[cfg(test)]
mod preference_tests {
    use super::*;
    #[test]
    fn background_history_does_not_create_unsaved_changes() {
        let a = Settings::default();
        let mut b = a.clone();
        b.last_success = 100;
        b.schedule_history.push("background update".into());
        assert!(same_preferences(&a, &b));
        b.watch_changes = true;
        assert!(!same_preferences(&a, &b));
    }
}
