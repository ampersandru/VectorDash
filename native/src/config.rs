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
pub struct SavedSearch {
    pub name: String,
    pub query: String,
    pub mode: String,
    pub category: String,
    pub scope: String,
    pub excluded: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub roots: Vec<Source>,
    pub exclusions: Vec<String>,
    pub excluded_types: String,
    pub count_files: bool,
    pub cache_precision: String,
    pub rank_calibration: bool,
    pub index_order: String,
    #[serde(skip)]
    pub(crate) index_phase: String,
    pub skip_similar_frames: bool,
    pub picture_grid: bool,
    pub image_search_enabled: bool,
    pub image_search_auto: bool,
    pub watch_changes: bool,
    pub ntfs_journal: bool,
    pub saved_searches: Vec<SavedSearch>,
    pub auto_check_updates: bool,
    pub auto_update: bool,
    pub update_channel: String,
    pub skip_windows_churn: bool,
    pub include_hidden: bool,
    pub embedding_server: String,
    pub remote_model: String,
    pub remote_context: u32,
    pub remote_context_auto: bool,
    pub remote_warm_on_open: bool,
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
    pub language: String,
    pub theme: String,
    pub accent: [u8; 3],
    pub trail_colors: std::collections::BTreeMap<String, [[u8; 3]; 2]>,
    pub popup_opacity: f32,
    pub unload_idle: bool,
    pub unload_minutes: u32,
    pub pause_for_game: bool,
    pub pause_for_fullscreen: bool,
    pub defer_search_model: bool,
    pub show_model_notices: bool,
    pub game_processes: String,
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
    pub video_decoder: String,
    pub video_decode_device: String,
    pub video_gpu: String,
    pub video_ffmpeg_fallback: bool,
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
    pub always_admin: bool,
    pub start_hidden: bool,
    pub schedule: String,
    pub schedule_hour: u32,
    pub schedule_minute: u32,
    pub idle_seconds: u32,
    pub index_images: bool,
    pub index_raw: bool,
    pub raw_develop: bool,
    pub raw_max_mib: u64,
    pub index_audio: bool,
    pub index_video: bool,
    pub code_search: bool,
    pub ocr: String,
    pub ocr_languages: String,
    pub ffmpeg: String,
    pub dictation_server: String,
    pub dictation_url: String,
    pub dictation_key: String,
    pub dictation_model: String,
    pub voice_popup_tool: String,
    pub voice_read_aloud: bool,
    pub voice_response: String,
    pub dictation_quant: String,
    pub microphone: String,
    pub dictation_auto_finish: bool,
    pub dictation_silence_seconds: f32,
    pub voice_search_hotkey: String,
    pub dictation_device: String,
    pub dictation_hotkey: String,
    pub dictation_keep_loaded: bool,
    pub dictation_unload_minutes: u32,
    pub ai_web_default: bool,
    pub computer_decision_mode: crate::decision::Mode,
    pub browser_decision_mode: crate::decision::Mode,
    pub decision_model: String,
    pub decision_runtime: String,
    pub decision_url: String,
    pub decision_key: String,
    pub decision_remote_model: String,
    pub decision_confidence: f32,
    pub computer_use_enabled: bool,
    pub browser_use_enabled: bool,
    pub control_max_steps: u32,
    pub control_browser_path: String,
    pub control_browser_visible: bool,
    pub ai_capture_web: bool,
    pub ai_capture_clipboard: bool,
    pub ai_capture_response: String,
    pub llm_enabled: bool,
    pub llm_vision: bool,
    pub llm_url: String,
    pub llm_key: String,
    pub tts_enabled: bool,
    pub tts_url: String,
    pub tts_key: String,
    pub tts_model: String,
    pub tts_voice: String,
    pub tts_auto: bool,
    pub llm_model: String,
    pub llm_context: u32,
    pub llm_context_auto: bool,
    pub type_hotkey: String,
    pub llm_response: String,
    pub llm_hotkey: String,
    pub web: crate::web_search::Settings,
    pub search_tool_order: Vec<String>,
    pub extra_shortcuts: std::collections::BTreeMap<String, String>,
    pub forced_shortcuts: std::collections::BTreeSet<String>,
    pub clipboard_options: crate::launcher::ClipOptions,
    pub clipboard_enabled: bool,
    pub clipboard_image_embeddings: bool,
    pub clipboard_hours: u32,
    pub clipboard_excluded_apps: String,
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
            cache_precision: "FP32".into(),
            rank_calibration: false,
            index_order: "Quick first".into(),
            index_phase: String::new(),
            skip_similar_frames: true,
            image_search_enabled: true,
            image_search_auto: false,
            picture_grid: true,
            watch_changes: false,
            ntfs_journal: false,
            saved_searches: vec![],
            auto_check_updates: true,
            auto_update: false,
            update_channel: "Release".into(),
            skip_windows_churn: true,
            include_hidden: true,
            embedding_server: String::new(),
            remote_model: String::new(),
            remote_context: 2048,
            remote_context_auto: true,
            remote_warm_on_open: false,
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
            language: "system".into(),
            theme: "Dark".into(),
            accent: [133, 115, 255],
            trail_colors: Default::default(),
            popup_opacity: 0.8,
            unload_idle: true,
            unload_minutes: 5,
            pause_for_game: false,
            pause_for_fullscreen: false,
            defer_search_model: false,
            show_model_notices: true,
            game_processes: String::new(),
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
            video_decoder: "Windows (native)".into(),
            video_decode_device: "Auto".into(),
            video_gpu: String::new(),
            video_ffmpeg_fallback: true,
            pdf_visuals: false,
            visual_detail: "Balanced".into(),
            default_search_mode: "Both".into(),
            search_excluded_types: String::new(),
            include_programs: true,
            hotkey: "Ctrl+Alt+Space".into(),
            explorer_integration: true,
            explorer_scope: true,
            dialog_integration: true,
            type_to_search: false,
            close_to_tray: true,
            start_with_windows: false,
            always_admin: false,
            start_hidden: false,
            schedule: "Manual".into(),
            schedule_hour: 2,
            schedule_minute: 0,
            idle_seconds: 120,
            index_images: true,
            index_raw: false,
            raw_develop: false,
            raw_max_mib: 256,
            index_audio: true,
            index_video: true,
            code_search: false,
            ocr: String::new(),
            ocr_languages: "eng".into(),
            ffmpeg: String::new(),
            dictation_server: "Local".into(),
            dictation_url: "http://localhost:8080/v1".into(),
            dictation_key: String::new(),
            dictation_model: String::new(),
            voice_popup_tool: "Search".into(),
            voice_read_aloud: false,
            voice_response: "Brief".into(),
            dictation_quant: "Auto".into(),
            microphone: String::new(),
            dictation_auto_finish: true,
            dictation_silence_seconds: 1.2,
            voice_search_hotkey: "Ctrl+Shift+D".into(),
            dictation_device: "CPU".into(),
            dictation_hotkey: "Ctrl+Shift+D".into(),
            dictation_keep_loaded: false,
            dictation_unload_minutes: 5,
            ai_web_default: false,
            computer_decision_mode: crate::decision::Mode::Off,
            browser_decision_mode: crate::decision::Mode::Off,
            decision_model: String::new(),
            decision_runtime: String::new(),
            decision_url: String::new(),
            decision_key: String::new(),
            decision_remote_model: String::new(),
            decision_confidence: 0.9,
            computer_use_enabled: false,
            browser_use_enabled: false,
            control_max_steps: 48,
            control_browser_path: String::new(),
            control_browser_visible: false,
            ai_capture_web: false,
            ai_capture_clipboard: false,
            ai_capture_response: "Full".into(),
            llm_enabled: true,
            llm_vision: false,
            llm_url: "http://localhost:8080/v1".into(),
            llm_key: String::new(),
            tts_enabled: true,
            tts_url: String::new(),
            tts_key: String::new(),
            tts_model: String::new(),
            tts_voice: String::new(),
            tts_auto: false,
            llm_model: String::new(),
            llm_response: "Full".into(),
            llm_context: 4096,
            llm_context_auto: true,
            type_hotkey: "Ctrl+Tab".into(),
            llm_hotkey: "Tab".into(),
            web: crate::web_search::Settings::default(),
            search_tool_order: vec![],
            extra_shortcuts: Default::default(),
            forced_shortcuts: Default::default(),
            clipboard_options: Default::default(),
            clipboard_enabled: false,
            clipboard_image_embeddings: false,
            clipboard_hours: 168,
            clipboard_excluded_apps: "1Password.exe, KeePass.exe, KeePassXC.exe, Bitwarden.exe"
                .into(),
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
pub(crate) fn wildcard_match(value: &str, pattern: &str) -> bool {
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

/// Compile the visibility policy once per search, never once per vector.
pub struct SearchPolicy {
    roots: Vec<(String, String)>,
    exclusions: Vec<String>,
    patterns: Vec<String>,
    types: Vec<String>,
    extra: Vec<String>,
    scopes: Vec<String>,
    restricted: bool,
    category: String,
    churn: bool,
}
impl SearchPolicy {
    pub fn new(s: &Settings, category: &str, scope: &str, excluded: &str) -> Self {
        let scopes = if scope.starts_with('[') {
            serde_json::from_str::<Vec<String>>(scope)
                .unwrap_or_default()
                .into_iter()
                .take(32)
                .map(fold)
                .collect()
        } else if scope.is_empty() {
            vec![]
        } else {
            vec![fold(scope)]
        };
        let (mut exclusions, mut patterns) = (vec![], vec![]);
        for value in &s.exclusions {
            if value.contains(['*', '?']) {
                patterns.extend(split_patterns(value));
            } else {
                exclusions.push(fold(value));
            }
        }
        Self {
            roots: s
                .roots
                .iter()
                .map(|r| (fold(&r.path), r.mode.clone()))
                .collect(),
            exclusions,
            patterns,
            types: split_patterns(&s.excluded_types),
            extra: split_patterns(excluded),
            scopes,
            restricted: !scope.is_empty(),
            category: category.into(),
            churn: s.skip_windows_churn,
        }
    }
    pub fn allowed(&self, path: &str, kind: &str) -> bool {
        if ntfs_internal_path(path) {
            return false;
        }
        let path = fold(path);
        let Some((root, mode)) = self
            .roots
            .iter()
            .filter(|(r, _)| folded_under(&path, r))
            .max_by_key(|(r, _)| r.len())
        else {
            return false;
        };
        if !["file", "folder"].contains(&kind) && mode != "full" {
            return false;
        }
        if self.restricted && !self.scopes.iter().any(|r| folded_under(&path, r)) {
            return false;
        }
        if self.exclusions.iter().any(|r| folded_under(&path, r)) {
            return false;
        }
        let start = root.rfind('\\').map_or(0, |i| i + 1);
        for name in path[start..].split('\\') {
            if forbidden_name(name, self.churn && (mode == "full" || name != "appdata"))
                || name_matches(name, &self.types)
                || name_matches(name, &self.patterns)
            {
                return false;
            }
        }
        let name = path.rsplit('\\').next().unwrap_or("");
        if name_matches(name, &self.extra) {
            return false;
        }
        let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
        match self.category.as_str() {
            "Images" => crate::content::is_image(ext),
            "Videos" => crate::content::is_video(ext),
            "Documents" => [
                "txt", "md", "pdf", "docx", "xlsx", "pptx", "csv", "log", "html", "htm", "rtf",
            ]
            .contains(&ext),
            "Audio / video" => crate::content::is_audio(ext) || crate::content::is_video(ext),
            "Folders" => kind == "folder",
            "Code" => crate::content::is_code(ext),
            _ => true,
        }
    }
}
fn folded_under(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|tail| tail.starts_with('\\'))
}
fn split_patterns(value: &str) -> Vec<String> {
    value
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|x| !x.is_empty())
        .map(str::to_lowercase)
        .collect()
}
fn name_matches(name: &str, patterns: &[String]) -> bool {
    let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
    patterns.iter().any(|pattern| {
        if pattern.contains(['*', '?']) {
            wildcard_match(name, pattern)
        } else {
            ext == pattern.trim_start_matches('.')
        }
    })
}

/// Reserved metadata is meaningful only directly below a local volume root.
/// Do not hide ordinary user folders named `$Extend` deeper in a source.
pub fn ntfs_internal_path(path: &str) -> bool {
    let folded = fold(path);
    let path = folded.strip_prefix("\\\\?\\").unwrap_or(&folded);
    let bytes = path.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return false;
    }
    let first = path[3..].split('\\').next().unwrap_or("");
    [
        "$extend", "$mft", "$mftmirr", "$logfile", "$volume", "$attrdef", "$bitmap", "$boot",
        "$badclus", "$secure", "$upcase",
    ]
    .contains(&first)
}
fn forbidden_name(name: &str, churn: bool) -> bool {
    name.starts_with('.')
        || [
            "node_modules",
            "__pycache__",
            "system volume information",
            "$recycle.bin",
        ]
        .contains(&name)
        || [
            "password",
            "passwd",
            "credential",
            "secret",
            "id_rsa",
            "id_ed25519",
            "id_ecdsa",
            "id_dsa",
        ]
        .iter()
        .any(|key| name.contains(key))
        || ["pem", "key", "pfx", "p12", "p8", "kdbx", "jks", "keystore"]
            .contains(&name.rsplit_once('.').map_or("", |(_, ext)| ext))
        || churn
            && [
                "windows",
                "appdata",
                "temp",
                "cache",
                "caches",
                "localcache",
                "tempstate",
                "code cache",
                "gpucache",
                "shadercache",
                "dxcache",
                "d3dscache",
            ]
            .contains(&name)
}
impl Settings {
    pub fn warm_on_search_open(&self, mode: &str) -> bool {
        mode != "Names" && (!self.remote_embeddings() || self.remote_warm_on_open)
    }
    pub fn remote_embeddings(&self) -> bool {
        self.embedding_server == "Remote"
            || (self.embedding_server.is_empty() && !self.remote_url.trim().is_empty())
    }
    pub fn remote_dictation(&self) -> bool {
        self.dictation_server == "Remote"
    }
    pub fn load(home: &Path) -> Result<Self> {
        let file = home.join("settings.json");
        let mut s: Self = if file.exists() {
            serde_json::from_slice(&fs::read(file)?)?
        } else {
            Self::default()
        };
        s.voice_search_hotkey = s.dictation_hotkey.clone();
        if s.embedding_server.is_empty() {
            s.embedding_server = if s.remote_url.trim().is_empty() {
                "Local"
            } else {
                "Remote"
            }
            .into();
        }
        for value in [
            &mut s.web.searxng_key,
            &mut s.web.degoog_key,
            &mut s.web.brave_key,
            &mut s.llm_key,
            &mut s.decision_key,
            &mut s.tts_key,
            &mut s.remote_key,
            &mut s.dictation_key,
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
            &mut s.web.searxng_key,
            &mut s.web.degoog_key,
            &mut s.web.brave_key,
            &mut s.llm_key,
            &mut s.decision_key,
            &mut s.tts_key,
            &mut s.remote_key,
            &mut s.dictation_key,
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
        if self.hardware == "Integrated GPU" {
            return "vulkan";
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
    /// Search visibility follows saved sources immediately, independently of database cleanup.
    pub fn searched(&self, p: &Path) -> bool {
        let value = p.to_string_lossy();
        self.roots
            .iter()
            .filter(|r| under(&value, &r.path))
            .max_by_key(|r| r.path.len())
            .is_some_and(|root| {
                p.ancestors()
                    .take_while(|a| under(&a.to_string_lossy(), &root.path))
                    .all(|a| self.allowed(a))
            })
    }
    pub fn allowed(&self, p: &Path) -> bool {
        if ntfs_internal_path(&p.to_string_lossy()) {
            return false;
        }
        #[cfg(windows)]
        if !self.include_hidden {
            use std::os::windows::fs::MetadataExt;
            if std::fs::symlink_metadata(p).is_ok_and(|m| m.file_attributes() & 0x2 != 0) {
                return false;
            }
        }
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
            || [
                "password",
                "passwd",
                "credential",
                "secret",
                "id_rsa",
                "id_ed25519",
                "id_ecdsa",
                "id_dsa",
            ]
            .iter()
            .any(|x| name.contains(x))
            || ["pem", "key", "pfx", "p12", "p8", "kdbx", "jks", "keystore"].contains(&ext.as_str())
        {
            return false;
        }
        if self.skip_windows_churn
            && (mode(&p.to_string_lossy(), self) == "full" || name != "appdata")
            && [
                "windows",
                "appdata",
                "temp",
                "cache",
                "caches",
                "localcache",
                "tempstate",
                "code cache",
                "gpucache",
                "shadercache",
                "dxcache",
                "d3dscache",
            ]
            .contains(&name.as_str())
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
    fn integrated_gpu_does_not_reuse_cached_cuda_device_backend() {
        let s = Settings {
            hardware: "Integrated GPU".into(),
            device: "CUDA0".into(),
            runtime_backend: "Auto".into(),
            server: "runtime-cuda/llama-server.exe".into(),
            ..Default::default()
        };
        assert_eq!(s.backend(), "vulkan");
        assert!(!s.runtime_compatible());
    }
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

#[cfg(test)]
mod bookmark_tests {
    use super::*;
    #[test]
    fn saved_searches_and_old_settings_round_trip() {
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert!(old.saved_searches.is_empty() && !old.ntfs_journal);
        let mut s = old;
        s.saved_searches.push(SavedSearch {
            name: "Recent PDFs".into(),
            query: "ext:pdf modified:today".into(),
            mode: "Names".into(),
            category: "Documents".into(),
            scope: r#"["C:\\docs","D:\\notes"]"#.into(),
            excluded: "*.tmp".into(),
        });
        let saved: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s.saved_searches, saved.saved_searches);
    }
}

#[derive(Default, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowState {
    #[serde(default)]
    pub physical_main: Option<[i32; 2]>,
    #[serde(default)]
    pub physical_popup: Option<[i32; 2]>,
    pub main: Option<[f32; 2]>,
    pub popup: Option<[f32; 2]>,
    #[serde(default)]
    pub search_split: Option<f32>,
    #[serde(default)]
    pub grid_split: Option<f32>,
    #[serde(default)]
    pub clipboard_split: Option<f32>,
}
impl WindowState {
    pub fn load(home: &Path) -> Self {
        std::fs::read(home.join("window-state.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, home: &Path) -> Result<()> {
        let target = home.join("window-state.tmp");
        std::fs::write(&target, serde_json::to_vec(self)?)?;
        std::fs::rename(target, home.join("window-state.json"))?;
        Ok(())
    }
}

#[cfg(test)]
mod window_tests {
    #[test]
    fn positions_survive_restart() {
        let home = tempfile::tempdir().unwrap();
        let state = super::WindowState {
            physical_main: None,
            physical_popup: None,
            main: Some([-1200.0, 180.0]),
            popup: Some([300.0, 250.0]),
            search_split: Some(0.62),
            grid_split: Some(0.45),
            clipboard_split: Some(0.5),
        };
        state.save(home.path()).unwrap();
        assert_eq!(super::WindowState::load(home.path()), state);
    }
}

#[cfg(test)]
mod shortcut_persistence_tests {
    #[test]
    fn saved_quick_search_binding_is_not_migrated() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("settings.json"),
            r#"{"hotkey":"Ctrl+Shift+Space","voice_search_hotkey":"Ctrl+Alt+Space"}"#,
        )
        .unwrap();
        assert_eq!(
            super::Settings::load(home.path()).unwrap().hotkey,
            "Ctrl+Shift+Space"
        );
    }
}

impl Settings {
    pub fn ai_chat_available(&self) -> bool {
        self.llm_enabled && !self.llm_model.trim().is_empty()
    }
    pub fn tool_available(&self, tool: &str) -> bool {
        match tool {
            "AI" => self.ai_chat_available(),
            "Clipboard" => self.clipboard_enabled,
            "Web" => self.web.enabled,
            "Search" | "Emoji" => true,
            _ => false,
        }
    }
    pub fn ordered_search_tools(&self) -> Vec<&'static str> {
        const ALL: [&str; 4] = ["Search", "Clipboard", "AI", "Web"];
        let mut tools = Vec::with_capacity(ALL.len());
        for requested in &self.search_tool_order {
            if let Some(tool) = ALL.iter().copied().find(|tool| *tool == requested) {
                if !tools.contains(&tool) {
                    tools.push(tool);
                }
            }
        }
        for tool in ALL {
            if !tools.contains(&tool) {
                tools.push(tool);
            }
        }
        tools
    }
    pub fn search_tools(&self) -> Vec<&'static str> {
        self.ordered_search_tools()
            .into_iter()
            .filter(|tool| self.tool_available(tool))
            .collect()
    }
    pub fn next_search_tool(&self, current: &str, backwards: bool) -> &'static str {
        let tools = self.search_tools();
        match tools.iter().position(|tool| *tool == current) {
            Some(i) => tools[(i + if backwards { tools.len() - 1 } else { 1 }) % tools.len()],
            None => {
                if backwards {
                    tools[tools.len() - 1]
                } else {
                    tools[0]
                }
            }
        }
    }
}
#[cfg(test)]
mod tool_visibility_tests {
    use super::*;
    #[test]
    fn disabled_tools_are_hidden_and_skipped_in_both_directions() {
        for clipboard in [false, true] {
            for enabled in [false, true] {
                for model in ["", "fixture"] {
                    let s = Settings {
                        clipboard_enabled: clipboard,
                        llm_enabled: enabled,
                        llm_model: model.into(),
                        ..Default::default()
                    };
                    let tools = s.search_tools();
                    assert_eq!(tools.contains(&"Clipboard"), clipboard);
                    assert_eq!(tools.contains(&"AI"), enabled && !model.is_empty());
                    for backwards in [false, true] {
                        let mut current = "Search";
                        for _ in 0..tools.len() {
                            current = s.next_search_tool(current, backwards);
                            assert!(s.tool_available(current));
                        }
                        assert_eq!(current, "Search");
                    }
                }
            }
        }
    }
    #[test]
    fn legacy_ai_configuration_stays_enabled_and_new_toggle_persists() {
        let old: Settings = serde_json::from_str(r#"{"llm_model":"existing"}"#).unwrap();
        assert!(old.ai_chat_available());
        let dir = tempfile::tempdir().unwrap();
        let s = Settings {
            llm_enabled: false,
            ..old
        };
        s.save(dir.path()).unwrap();
        assert!(!Settings::load(dir.path()).unwrap().ai_chat_available());
    }
}

#[cfg(test)]
mod server_mode_tests {
    use super::*;
    #[test]
    fn explicit_local_preserves_remote_details_without_using_them() {
        let mut s = Settings {
            remote_url: "https://example.test/v1".into(),
            ..Default::default()
        };
        assert!(s.remote_embeddings());
        s.embedding_server = "Local".into();
        assert!(!s.remote_embeddings());
        s.embedding_server = "Remote".into();
        assert!(s.remote_embeddings());
        assert!(!s.remote_dictation());
        s.dictation_server = "Remote".into();
        assert!(s.remote_dictation());
    }
    #[test]
    fn remote_secret_and_voice_defaults_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let s = Settings {
            embedding_server: "Local".into(),
            dictation_key: "voice-secret".into(),
            voice_popup_tool: "AI".into(),
            voice_response: "Medium".into(),
            voice_read_aloud: true,
            ..Default::default()
        };
        s.save(temp.path()).unwrap();
        assert_eq!(Settings::load(temp.path()).unwrap(), s);
    }
}

#[cfg(test)]
mod ntfs_metadata_tests {
    use super::*;
    #[test]
    fn rooted_ntfs_metadata_filtered_in_scanning_and_search() {
        let mut s = Settings::default();
        s.include_hidden = true;
        s.skip_windows_churn = false;
        s.roots = vec![Source {
            path: "C:\\".into(),
            mode: "quick".into(),
        }];
        let policy = SearchPolicy::new(&s, "All files", "", "");
        for path in [
            "C:\\$Extend\\$Deleted\\abc",
            "c:/$MFT",
            "\\\\?\\C:\\$Extend\\$ObjId",
        ] {
            assert!(ntfs_internal_path(path));
            assert!(!s.allowed(Path::new(path)));
            assert!(!policy.allowed(path, "file"));
        }
        for path in [
            "C:\\Projects\\$Extend\\notes.txt",
            "C:\\$Budget.txt",
            "C:\\Users\\example\\AppData\\Local\\RHI\\nvngx.dll",
        ] {
            assert!(!ntfs_internal_path(path));
            assert!(s.allowed(Path::new(path)));
            assert!(policy.allowed(path, "file"));
        }
    }
}

#[cfg(all(test, windows))]
mod web_settings_tests {
    use super::*;
    #[test]
    fn web_keys_are_protected_and_disabled_web_is_hidden() {
        let home = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.web = crate::web_search::Settings {
            enabled: true,
            searxng_url: "https://example.org".into(),
            searxng_key: "searx-private-key".into(),
            degoog_key: "degoog-private-key".into(),
            brave_key: "brave-private-key".into(),
            ..Default::default()
        };
        s.save(home.path()).unwrap();
        let json = std::fs::read_to_string(home.path().join("settings.json")).unwrap();
        for key in [&s.web.searxng_key, &s.web.degoog_key, &s.web.brave_key] {
            assert!(!json.contains(key));
        }
        let loaded = Settings::load(home.path()).unwrap();
        assert_eq!(loaded.web, s.web);
        assert_eq!(loaded.search_tools().last(), Some(&"Web"));
        assert_eq!(loaded.next_search_tool("Web", false), "Search");
        s.web.enabled = false;
        assert!(!s.search_tools().contains(&"Web"));
        assert!(!s.tool_available("Web"));
    }
}

#[cfg(test)]
mod search_order_tests {
    use super::*;
    #[test]
    fn custom_order_persists_and_cycles_visible_modes_only() {
        let home = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.web.enabled = true;
        s.llm_enabled = true;
        s.llm_model = "fixture".into();
        s.search_tool_order = vec!["Web".into(), "AI".into(), "Web".into(), "unknown".into()];
        assert_eq!(
            s.ordered_search_tools(),
            vec!["Web", "AI", "Search", "Clipboard"]
        );
        assert_eq!(s.search_tools(), vec!["Web", "AI", "Search"]);
        assert_eq!(s.next_search_tool("Web", false), "AI");
        assert_eq!(s.next_search_tool("Web", true), "Search");
        s.save(home.path()).unwrap();
        let loaded = Settings::load(home.path()).unwrap();
        assert_eq!(loaded.search_tools(), s.search_tools());
        s.web.enabled = false;
        assert_eq!(s.search_tools(), vec!["AI", "Search"]);
        s.web.enabled = true;
        assert_eq!(s.search_tools(), vec!["Web", "AI", "Search"]);
    }
}

#[cfg(test)]
mod packaged_cache_tests {
    #[test]
    fn packaged_caches_are_excluded_before_queueing_but_quick_appdata_names_remain_searchable() {
        let mut s = super::Settings {
            skip_windows_churn: true,
            include_hidden: true,
            roots: vec![super::Source {
                path: "C:\\".into(),
                mode: "quick".into(),
            }],
            ..Default::default()
        };
        let cached = r"C:\Users\fixture\AppData\Local\Packages\OpenAI.Codex\LocalCache\Roaming\Codex\Default\Network\Cookies-journal";
        assert!(!s.searched(std::path::Path::new(cached)));
        assert!(!super::SearchPolicy::new(&s, "All files", "", "").allowed(cached, "file"));
        let game = r"C:\Users\fixture\AppData\Local\RHI\nvngx_dlssnr.dll";
        assert!(s.searched(std::path::Path::new(game)));
        assert!(super::SearchPolicy::new(&s, "All files", "", "").allowed(game, "file"));
        s.skip_windows_churn = false;
        assert!(s.searched(std::path::Path::new(cached)));
    }
}

#[cfg(test)]
mod ai_search_defaults_tests {
    use super::*;
    #[test]
    fn old_settings_keep_web_opt_in_and_capture_preferences_round_trip() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(
            !settings.ai_web_default && !settings.ai_capture_web && !settings.ai_capture_clipboard
        );
        assert_eq!(settings.ai_capture_response, "Full");
        settings.ai_web_default = true;
        settings.ai_capture_web = true;
        settings.ai_capture_clipboard = true;
        settings.ai_capture_response = "Medium".into();
        let restored: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert!(
            restored.ai_web_default && restored.ai_capture_web && restored.ai_capture_clipboard
        );
        assert_eq!(restored.ai_capture_response, "Medium");
        assert!(!restored.web.available());
        settings.web.enabled = true;
        assert!(!settings.web.available());
        settings.web.searxng_url = "https://example.test".into();
        assert!(settings.web.available());
        settings.web.enabled = false;
        assert!(!settings.web.available());
    }
}

impl Settings {
    pub fn activity_colors(&self, mode: &str) -> [[u8; 3]; 2] {
        self.trail_colors
            .get(mode)
            .copied()
            .unwrap_or_else(|| match mode {
                "Microphone" => [[255, 55, 75], [255, 180, 35]],
                "AI" => [[155, 85, 255], [55, 135, 255]],
                "Web" => [[169, 177, 62], [102, 125, 32]],
                _ => [[55, 145, 255], [45, 210, 255]],
            })
    }
}
#[cfg(test)]
mod trail_color_tests {
    use super::*;
    #[test]
    fn independent_colors_round_trip_and_missing_keys_keep_existing_defaults() {
        let mut s: Settings = serde_json::from_str("{}").unwrap();
        let ai = s.activity_colors("AI");
        assert_eq!(s.activity_colors("Microphone")[0], [255, 55, 75]);
        s.trail_colors
            .insert("Search".into(), [[10, 20, 30], [40, 50, 60]]);
        s.trail_colors
            .insert("Clipboard".into(), [[70, 80, 90], [100, 110, 120]]);
        let restored: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(
            restored.activity_colors("Search"),
            [[10, 20, 30], [40, 50, 60]]
        );
        assert_ne!(
            restored.activity_colors("Search"),
            restored.activity_colors("Clipboard")
        );
        assert_eq!(restored.activity_colors("AI"), ai);
    }
}
