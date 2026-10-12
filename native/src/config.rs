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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub roots: Vec<Source>,
    pub exclusions: Vec<String>,
    pub excluded_types: String,
    pub count_files: bool,
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
    pub popup_gradient: f32,
    pub include_programs: bool,
    pub hotkey: String,
    pub explorer_integration: bool,
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
            popup_gradient: 0.4,
            include_programs: true,
            hotkey: "Ctrl+Shift+Space".into(),
            explorer_integration: true,
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
        Ok(s)
    }
    pub fn allowed(&self, p: &Path) -> bool {
        if self
            .exclusions
            .iter()
            .any(|r| under(&p.to_string_lossy(), r))
        {
            return false;
        }
        let ext = p
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if self
            .excluded_types
            .split([',', ';', ' '])
            .any(|x| !x.is_empty() && x.trim_start_matches('.').eq_ignore_ascii_case(&ext))
        {
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
