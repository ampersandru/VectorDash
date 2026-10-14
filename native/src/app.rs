use crate::{
    config::{Settings, Source},
    db::{Database, Hit, SourceStats, Stats},
    engine::{Control, Engine, Event, Progress},
    models::Models,
    setup::Setup,
};
use anyhow::Result;
use eframe::egui::{self, Color32, RichText};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
const PAGES: [&str; 8] = [
    "Search",
    "Sources",
    "Models & hardware",
    "Scheduling",
    "Settings",
    "AI settings",
    "Logs",
    "About",
];
const CATEGORIES: [&str; 7] = [
    "All files",
    "Images",
    "Documents",
    "Audio / video",
    "Folders",
    "Code",
    "Programs",
];
pub struct App {
    home: PathBuf,
    s: Settings,
    saved_settings: Settings,
    engine: Engine,
    setup: Setup,
    rx: crossbeam_channel::Receiver<Event>,
    shutdown: Arc<AtomicBool>,
    scheduler_shutdown: Arc<AtomicBool>,
    mcp: Option<crate::mcp::Server>,
    page: usize,
    collapsed: bool,
    query: String,
    mode: String,
    category: String,
    scope: String,
    scope_indexed: Option<bool>,
    type_flash: Option<Instant>,
    logo: egui::TextureHandle,
    exclude_types: String,
    hits: Vec<Hit>,
    selected: usize,
    scroll_to_selection: bool,
    sequence: u64,
    changed: Instant,
    dirty: bool,
    searching: bool,
    search_pending: bool,
    search_ms: f64,
    search_error: Option<String>,
    indexing: bool,
    setup_busy: bool,
    progress: Progress,
    stats: Stats,
    source_stats: HashMap<String, SourceStats>,
    status: String,
    logs: Vec<(String, String)>,
    log_filter: [bool; 4],
    model_logs: bool,
    file_logs: bool,
    quick: bool,
    target: isize,
    large_preview: bool,
    texture: Option<(String, egui::TextureHandle)>,
    thumbs: HashMap<String, egui::TextureHandle>,
    thumb_pending: std::collections::HashSet<String>,
    thumb_failed: std::collections::HashSet<String>,
    preview_pending: Option<String>,
    preview_error: Option<String>,
    source_mode: String,
    source_selected: Vec<bool>,
    exclusion_selected: Vec<bool>,
    update_release: Option<crate::updates::Release>,
    update_installer: Option<PathBuf>,
    update_busy: bool,
    update_status: String,
    show_changelog: bool,
    watch_status: String,
    manual_exclude: String,
    runtime_setup: bool,
    model_setup: bool,
    encoder_setup: bool,
    ocr_setup: bool,
    devices: Vec<(String, String)>,
    uninstall_confirm: bool,
    delete_confirm: Option<Hit>,
    hotkey_capture: bool,
    startup: Instant,
    last_schedule: String,
    smoke: Option<PathBuf>,
    smoke_seconds: u64,
    frames: u64,
    max_frame_ms: f64,
    last_frame: Instant,
    material_key: Option<u64>,
    appearance_key: String,
    devices_server: String,
    devices_loading: bool,
    setup_progress: Option<(String, u64, u64)>,
    automatic: bool,
    schedule_status: String,
    schedule_started: Instant,
    schedule_retry: Instant,
    exiting: bool,
    qa_screenshot: Option<PathBuf>,
    qa_capture_sent: bool,
    qa_captured: bool,
    qa_hidden: bool,
    qa_hide_sent: bool,
    qa_hidden_reopened: bool,
    popup_size_state: Option<bool>,
    popup_had_focus: bool,
    qa_child_capture_sent: bool,
    qa_child_size: Option<[usize; 2]>,
    qa_selection: Option<usize>,
    result_scroll_offset: f32,
    child_theme_key: String,
    search_visible: bool,
    model_warm_attempted: bool,
    model_warm_error: Option<String>,
    last_search_mode: String,
    popup_model_panel: bool,
    clear_index_confirm: bool,
    index_clearing: bool,
    tool: String,
    tool_items: Vec<crate::launcher::Item>,
    ai_answer: String,
    ai_question: String,
    ai_error: Option<String>,
    ai_busy: bool,
    ai_job: u64,
    ai_cancel: Arc<AtomicBool>,
    llm_models: Vec<String>,
    llm_models_busy: bool,
    llm_hotkey_capture: bool,
    clipboard_clear_confirm: bool,
    save_search_open: bool,
    save_search_name: String,
    filter_open: bool,
    filter_anchor: egui::Pos2,
    markdown: egui_commonmark::CommonMarkCache,
    color_emoji: crate::color_emoji::Cache,
    dictation_submit_ai: bool,
    dictation_busy: bool,
    dictation_job: u64,
    dictation_stop: Arc<AtomicBool>,
    dictation_cancel: Arc<AtomicBool>,
    dictation_status: String,
    voice_hotkey_capture: bool,
    global_voice_capture: bool,
    microphones: Vec<String>,
    microphones_busy: bool,
    microphones_loaded: bool,
    component_stats: std::collections::HashMap<String, crate::resources::Usage>,
    component_stats_at: Instant,
    component_stats_busy: bool,
    remove_component: Option<String>,
    capture_preview_key: Option<(u32, bool)>,

    clipboard_preview_id: i64,
    clipboard_texture: Option<egui::TextureHandle>,
    capture_open: bool,
    capture_targets: Vec<crate::capture::Target>,
    capture_target: Option<crate::capture::Target>,
    capture_area: bool,
    capture_busy: bool,
    capture_bytes: Option<Vec<u8>>,
    capture_texture: Option<egui::TextureHandle>,
    capture_drag: Option<egui::Pos2>,
    capture_rect: Option<egui::Rect>,
    ai_images: Vec<String>,
    ai_attachment: Option<egui::TextureHandle>,
    window_state: crate::config::WindowState,
    position_mode: Option<bool>,
    position_settle: u8,
    ai_messages: Vec<crate::ai::Message>,
    ai_context_trimmed: bool,
    ai_scroll: f32,
    popup_outside_down: bool,
    select_query: bool,
    preview_ready_frame: bool,
    ai_follow_bottom: bool,
    ai_scroll_offset: f32,
    qa_ai_scrolled: bool,
    qa_ai_followup_sent: bool,
    qa_ai_scroll_before: f32,
    ai_metrics: crate::ai::Metrics,
    ai_image_view: Option<String>,
    tts_models: Vec<String>,
    tts_voices: Vec<String>,
    tts_catalog_busy: bool,
    tts_notice: String,
    tts_voice_job: u64,
    tts_voice_busy: bool,
    tts_voice_due: Option<Instant>,
    response_length: String,
    qa_toolbar_top: f32,
    qa_filter_top: Option<f32>,
    tts_status: String,
    tts_busy: bool,
    tts_job: u64,
    tts_cancel: Arc<AtomicBool>,
    read_aloud: bool,
    clipboard_thumbs: HashMap<i64, egui::TextureHandle>,
    clipboard_thumb_pending: std::collections::HashSet<i64>,
}
impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        home: PathBuf,
        smoke: Option<PathBuf>,
    ) -> Result<Self> {
        use raw_window_handle::HasWindowHandle;
        if let Ok(handle) = cc.window_handle() {
            if let raw_window_handle::RawWindowHandle::Win32(window) = handle.as_raw() {
                crate::platform::register_main_window(window.hwnd.get());
            }
        }
        let mut s = Settings::load(&home)?;
        if let Some(path) = crate::setup::find_ocr(&s.ocr) {
            s.ocr = path.to_string_lossy().into_owned();
        }
        crate::platform::set_game_names(&s.game_processes);
        crate::scheduling::start_monitor();
        let db = Database::new(&home, &s)?;
        let (tx, rx) = crossbeam_channel::bounded(512);
        let tx = crate::engine::NotifySender::new(tx, Some(cc.egui_ctx.clone()))
            .with_log(home.join("vectordash.log"));
        let engine = Engine {
            db,
            models: Models::default(),
            control: Control::default(),
            tx: tx.clone(),
            searching: Arc::new(AtomicBool::new(false)),
            vectors: crate::vector_cache::Cache::default(),
        };
        let setup = Setup::new(home.clone(), tx.clone());
        let shutdown = Arc::new(AtomicBool::new(false));
        if smoke.is_none() {
            crate::platform::hotkeys(tx.clone(), shutdown.clone(), s.clone());
        }
        *engine.control.schedule.lock().unwrap() = s.clone();
        let scheduler_shutdown = Arc::new(AtomicBool::new(false));
        if smoke.is_none() || std::env::args().any(|a| a == "--smoke-scheduler") {
            engine.start_scheduler(scheduler_shutdown.clone());
        }
        engine.refresh_stats(&s);
        if s.include_programs {
            crate::platform::warm_programs();
        }
        crate::appearance::apply(&cc.egui_ctx, &s);
        let mut fonts = egui::FontDefinitions::default();
        let system = std::path::PathBuf::from(
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()),
        )
        .join("Fonts");
        for name in ["segoeui.ttf", "seguiemj.ttf", "Nirmala.ttf", "msyh.ttc"] {
            if let Ok(bytes) = std::fs::read(system.join(name)) {
                fonts
                    .font_data
                    .insert(name.into(), egui::FontData::from_owned(bytes).into());
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts.families.entry(family).or_default().push(name.into());
                }
            }
        }
        if let Ok(bytes) = std::fs::read(system.join("segoeuib.ttf")) {
            fonts
                .font_data
                .insert("bold".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .insert(egui::FontFamily::Name("bold".into()), vec!["bold".into()]);
        }
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.retain(|name| name != "segoeui.ttf");
            family.insert(0, "segoeui.ttf".into());
        }
        let fallback = fonts.families[&egui::FontFamily::Proportional].clone();
        fonts
            .families
            .entry(egui::FontFamily::Name("bold".into()))
            .or_insert(fallback);
        cc.egui_ctx.set_fonts(fonts);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let saved_start_hidden = s.start_hidden;
        let window_state = crate::config::WindowState::load(&home);
        let read_aloud = s.tts_auto;
        let response_length = s.llm_response.clone();
        let mut app = Self {
            home,
            collapsed: s.sidebar_collapsed,
            mode: s.default_search_mode.clone(),
            exclude_types: s.search_excluded_types.clone(),
            runtime_setup: s.setup_runtime,
            model_setup: s.setup_model,
            encoder_setup: s.setup_encoder,
            ocr_setup: s.setup_ocr,
            saved_settings: s.clone(),
            s,
            engine,
            setup,
            rx,
            shutdown,
            scheduler_shutdown,
            mcp: None,
            page: 0,
            query: String::new(),
            category: "All files".into(),
            scope: String::new(),
            scope_indexed: None,
            type_flash: None,
            logo: {
                let image =
                    image::load_from_memory(include_bytes!("../assets/vectordash.png"))?.to_rgba8();
                cc.egui_ctx.load_texture(
                    "VectorDash logo",
                    egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    ),
                    Default::default(),
                )
            },
            hits: vec![],
            selected: 0,
            scroll_to_selection: false,
            sequence: 0,
            changed: Instant::now(),
            dirty: false,
            searching: false,
            search_pending: false,
            search_ms: 0.0,
            search_error: None,
            indexing: false,
            setup_busy: false,
            progress: Progress::default(),
            stats: Stats::default(),
            source_stats: HashMap::new(),
            status: "Ready — models load only when needed".into(),
            logs: vec![],
            log_filter: [false, true, true, true],
            model_logs: false,
            file_logs: false,
            quick: false,
            target: 0,
            large_preview: false,
            texture: None,
            thumbs: HashMap::new(),
            thumb_pending: std::collections::HashSet::new(),
            thumb_failed: std::collections::HashSet::new(),
            preview_pending: None,
            preview_error: None,
            source_mode: "quick".into(),
            source_selected: vec![],
            exclusion_selected: vec![],
            update_release: None,
            update_installer: None,
            update_busy: false,
            update_status: String::new(),
            show_changelog: false,
            watch_status: String::new(),
            manual_exclude: String::new(),
            devices: vec![],
            uninstall_confirm: false,
            delete_confirm: None,
            hotkey_capture: false,
            startup: Instant::now(),
            last_schedule: String::new(),
            smoke,
            smoke_seconds: 4,
            frames: 0,
            max_frame_ms: 0.0,
            last_frame: Instant::now(),
            material_key: None,
            appearance_key: String::new(),
            devices_server: String::new(),
            devices_loading: false,
            setup_progress: None,
            automatic: false,
            schedule_status: String::new(),
            schedule_started: Instant::now(),
            schedule_retry: Instant::now(),
            exiting: false,
            qa_screenshot: None,
            qa_capture_sent: false,
            qa_captured: false,
            qa_hidden: false,
            qa_hide_sent: false,
            qa_hidden_reopened: false,
            popup_size_state: None,
            popup_had_focus: false,
            qa_child_capture_sent: false,
            qa_child_size: None,
            qa_selection: None,
            result_scroll_offset: 0.0,
            child_theme_key: String::new(),
            search_visible: !(std::env::args().any(|a| a == "--startup") && saved_start_hidden),
            model_warm_attempted: false,
            model_warm_error: None,
            last_search_mode: String::new(),
            popup_model_panel: false,
            clear_index_confirm: false,
            index_clearing: false,
            tool: "Search".into(),
            tool_items: vec![],
            ai_answer: String::new(),
            ai_question: String::new(),
            ai_error: None,
            ai_busy: false,
            ai_job: 0,
            ai_cancel: Arc::new(AtomicBool::new(false)),
            llm_models: vec![],
            llm_models_busy: false,
            llm_hotkey_capture: false,
            clipboard_clear_confirm: false,
            save_search_open: false,
            save_search_name: String::new(),
            filter_open: false,
            filter_anchor: egui::Pos2::ZERO,
            markdown: egui_commonmark::CommonMarkCache::default(),
            color_emoji: Default::default(),
            dictation_submit_ai: false,
            dictation_busy: false,
            dictation_job: 0,
            dictation_stop: Arc::new(AtomicBool::new(false)),
            dictation_cancel: Arc::new(AtomicBool::new(false)),
            dictation_status: String::new(),
            voice_hotkey_capture: false,
            global_voice_capture: false,
            microphones: vec![],
            microphones_busy: false,
            microphones_loaded: false,
            component_stats: Default::default(),
            component_stats_at: Instant::now() - Duration::from_secs(30),
            component_stats_busy: false,
            remove_component: None,
            capture_preview_key: None,

            clipboard_preview_id: -1,
            clipboard_texture: None,
            capture_open: false,
            capture_targets: vec![],
            capture_target: None,
            capture_area: false,
            capture_busy: false,
            capture_bytes: None,
            capture_texture: None,
            capture_drag: None,
            capture_rect: None,
            ai_images: vec![],
            ai_attachment: None,
            window_state: window_state,
            position_mode: None,
            position_settle: 0,
            ai_messages: vec![],
            ai_context_trimmed: false,
            ai_scroll: 0.0,
            popup_outside_down: false,
            select_query: false,
            preview_ready_frame: false,
            ai_follow_bottom: false,
            ai_scroll_offset: 0.0,
            qa_ai_scrolled: false,
            qa_ai_followup_sent: false,
            qa_ai_scroll_before: 0.0,
            ai_metrics: crate::ai::Metrics::default(),
            ai_image_view: None,
            tts_models: vec![],
            tts_voices: vec![],
            tts_catalog_busy: false,
            tts_notice: String::new(),
            tts_voice_job: 0,
            tts_voice_busy: false,
            tts_voice_due: None,
            response_length,
            qa_toolbar_top: 0.0,
            qa_filter_top: None,
            tts_status: String::new(),
            tts_busy: false,
            tts_job: 0,
            tts_cancel: Arc::new(AtomicBool::new(false)),
            read_aloud,
            clipboard_thumbs: HashMap::new(),
            clipboard_thumb_pending: std::collections::HashSet::new(),
        };
        if app.smoke.is_none() {
            crate::launcher::monitor(
                app.home.clone(),
                app.engine.control.schedule.clone(),
                app.scheduler_shutdown.clone(),
                app.engine.tx.clone(),
                app.engine.models.clone(),
            );
        }
        crate::dictation::start_monitor(
            app.engine.control.schedule.clone(),
            app.scheduler_shutdown.clone(),
        );
        if let Ok(text) = tail_log(&app.home.join("vectordash.log")) {
            for line in text.lines() {
                let (level, text) = line.split_once(' ').unwrap_or(("Info", line));
                app.logs.push((level.into(), text.into()));
            }
        }
        if app.smoke.is_some() {
            let args = std::env::args().collect::<Vec<_>>();
            let get = |flag: &str| {
                args.iter()
                    .position(|a| a == flag)
                    .and_then(|i| args.get(i + 1))
                    .cloned()
            };
            app.smoke_seconds = get("--smoke-seconds")
                .and_then(|v| v.parse().ok())
                .unwrap_or(4);
            app.quick = args.iter().any(|a| a == "--smoke-quick");
            app.qa_hidden = args
                .iter()
                .any(|a| a == "--smoke-hidden" || a == "--smoke-tray");
            app.qa_screenshot = get("--smoke-screenshot").map(PathBuf::from);
            app.qa_selection = get("--smoke-selection").and_then(|v| v.parse().ok());
            if args.iter().any(|a| a == "--smoke-dirty") {
                app.s.unload_idle = !app.s.unload_idle;
            }
            app.page = get("--smoke-page")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if let Some(query) = get("--smoke-query") {
                app.query = query;
                app.mode = "Names".into();
                app.dirty = true;
            }
            if let Some(status) = get("--smoke-status") {
                app.status = status;
            }
            if let Some(style) = get("--smoke-response") {
                app.response_length = style;
            }
            if std::env::args().any(|a| a == "--smoke-tts-voices") {
                app.tts_voice_due = Some(Instant::now());
            }
            if let Some(tool) = get("--smoke-tool") {
                app.switch_tool(&tool);
            }
            if let Some(answer) = get("--smoke-ai-answer") {
                app.ai_answer = answer.clone();
                app.ai_messages.push(crate::ai::Message {
                    role: "assistant".into(),
                    content: answer,
                    images: vec![],
                });
            }
            if let Some(path) = get("--smoke-ai-image") {
                if let Ok(bytes) = std::fs::read(path) {
                    if let Ok(url) = crate::capture::vision(&bytes, None) {
                        app.ai_messages.insert(
                            0,
                            crate::ai::Message {
                                role: "user".into(),
                                content: "Explain this picture".into(),
                                images: vec![url],
                            },
                        );
                    }
                }
            }
            if std::env::args().any(|a| a == "--smoke-select-query") {
                app.select_query = true;
            }
            if let Some(path) = get("--smoke-preview") {
                app.hits = vec![Hit {
                    path,
                    kind: "image".into(),
                    score: 1.0,
                    text: String::new(),
                    reason: "Image fixture".into(),
                    page: 0,
                    seconds: -1.0,
                    line: 0,
                }];
                app.query = "Image fixture".into();
                app.large_preview = true;
                app.dirty = false;
                app.search_pending = false;
            }
        }
        if app.smoke.is_some() {
            // Hidden popups may stop receiving frames; use a background deadline.
            let tx = app.engine.tx.clone();
            let timeout = app.smoke_seconds.saturating_add(2);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(timeout));
                let _ = tx.send(Event::Quit);
            });
        }
        if app.s.mcp_enabled {
            app.apply_mcp();
        }
        Ok(app)
    }
    fn accent(&self) -> Color32 {
        Color32::from_rgb(self.s.accent[0], self.s.accent[1], self.s.accent[2])
    }
    fn save(&mut self) -> bool {
        let latest = self.engine.control.schedule.lock().unwrap().clone();
        self.s.last_success = self.s.last_success.max(latest.last_success);
        self.s.last_schedule_slot = self.s.last_schedule_slot.max(latest.last_schedule_slot);
        if latest.last_success > 0 && latest.last_success >= self.s.last_success {
            self.s.schedule_history = latest.schedule_history;
        }
        self.s.sidebar_collapsed = self.collapsed;
        self.s.setup_runtime = self.runtime_setup;
        self.s.setup_model = self.model_setup;
        self.s.setup_encoder = self.encoder_setup;
        self.s.setup_ocr = self.ocr_setup;
        let pages = match self.s.performance.as_str() {
            "Low memory" => 8192,
            "Fast search" => 65536,
            _ => 32768,
        };
        if self.engine.db.cache_kib != pages {
            self.engine.db.cache_kib = pages;
            self.engine.vectors.clear();
        }
        match self.s.save(&self.home) {
            Ok(()) => {
                self.engine.control.update_settings(self.s.clone());
                self.saved_settings = self.s.clone();
                self.read_aloud = self.s.tts_auto;
                crate::platform::set_game_names(&self.s.game_processes);
                self.model_warm_attempted = false;
                self.model_warm_error = None;
                self.status = "Settings saved".into();
                true
            }
            Err(e) => {
                self.status = e.to_string();
                false
            }
        }
    }
    fn pending_settings(&self) -> Settings {
        let mut s = self.s.clone();
        s.sidebar_collapsed = self.collapsed;
        s.setup_runtime = self.runtime_setup;
        s.setup_model = self.model_setup;
        s.setup_encoder = self.encoder_setup;
        s.setup_ocr = self.ocr_setup;
        s
    }
    fn undo_settings(&mut self, ctx: &egui::Context) {
        let live = self.engine.control.schedule.lock().unwrap().clone();
        self.s = self.saved_settings.clone();
        self.s.last_success = live.last_success;
        self.s.last_schedule_slot = live.last_schedule_slot;
        self.s.schedule_history = live.schedule_history;
        self.collapsed = self.s.sidebar_collapsed;
        self.runtime_setup = self.s.setup_runtime;
        self.model_setup = self.s.setup_model;
        self.encoder_setup = self.s.setup_encoder;
        self.ocr_setup = self.s.setup_ocr;
        crate::appearance::apply(ctx, &self.s);
        self.engine.control.update_settings(self.s.clone());
        self.status = "Unsaved changes undone".into();
    }
    fn apply_settings(&mut self, ctx: &egui::Context) {
        let previous = self.saved_settings.clone();
        let startup_changed = previous.start_with_windows != self.s.start_with_windows;
        let hotkeys_changed = (
            previous.hotkey.as_str(),
            previous.dictation_hotkey.as_str(),
            previous.explorer_integration,
            previous.explorer_scope,
            previous.dialog_integration,
        ) != (
            self.s.hotkey.as_str(),
            self.s.dictation_hotkey.as_str(),
            self.s.explorer_integration,
            self.s.explorer_scope,
            self.s.dialog_integration,
        );
        let mcp_changed = (
            previous.mcp_enabled,
            previous.mcp_lan,
            previous.mcp_auth,
            previous.mcp_port,
            previous.mcp_token.as_str(),
        ) != (
            self.s.mcp_enabled,
            self.s.mcp_lan,
            self.s.mcp_auth,
            self.s.mcp_port,
            self.s.mcp_token.as_str(),
        );
        if (
            previous.schedule.as_str(),
            previous.schedule_hour,
            previous.schedule_minute,
            previous.schedule_days,
        ) != (
            self.s.schedule.as_str(),
            self.s.schedule_hour,
            self.s.schedule_minute,
            self.s.schedule_days,
        ) {
            self.s.schedule_enabled_at = chrono::Local::now().timestamp();
            self.s.last_schedule_slot = 0;
        }
        if !self.save() {
            return;
        }
        if previous.llm_url != self.s.llm_url || previous.llm_key != self.s.llm_key {
            self.stop_ai();
            self.ai_messages.clear();
            self.ai_images.clear();
            self.ai_attachment = None;
            self.ai_answer.clear();
            self.ai_question.clear();
            self.ai_error = None;
            self.ai_context_trimmed = false;
            self.llm_models.clear();
        }
        if mcp_changed {
            self.apply_mcp();
        }
        if startup_changed {
            self.run(crate::platform::startup(self.s.start_with_windows));
        }
        if self.smoke.is_none() && hotkeys_changed {
            self.shutdown.store(true, Ordering::Relaxed);
            let tx = self.engine.tx.clone();
            let s = self.s.clone();
            let signal = Arc::new(AtomicBool::new(false));
            self.shutdown = signal.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                crate::platform::hotkeys(tx, signal, s);
            });
        }
        self.changed();
        crate::appearance::apply(ctx, &self.s);
    }
    fn log(&mut self, level: String, text: String) {
        let line = format!(
            "{} {text}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.home.join("vectordash.log"))
        {
            let _ = writeln!(f, "{level} {line}");
        }
        self.logs.push((level, line));
        if self.logs.len() > 3000 {
            self.logs.drain(..500);
        }
    }
    fn events(&mut self, ctx: &egui::Context) {
        for event in self.rx.try_iter().take(100).collect::<Vec<_>>() {
            let voice_summon = matches!(&event, Event::VoiceSummon(..));
            if voice_summon
                && self.page == 0
                && self.search_visible
                && crate::platform::main_window_visible()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.toggle_dictation();
                continue;
            }
            let event = if let Event::VoiceSummon(target) = event {
                Event::Summon(target, String::new())
            } else {
                event
            };
            let scoped_summon = matches!(&event, Event::ScopedSummon(..));
            match event {
                Event::VoiceSummon(_) => {}
                Event::Microphones(result) => {
                    self.microphones_loaded = true;
                    self.microphones_busy = false;
                    match result {
                        Ok(names) => self.microphones = names,
                        Err(e) => self.status = e,
                    }
                }
                Event::ComponentStats(stats) => {
                    self.component_stats = stats;
                    self.component_stats_busy = false;
                }
                Event::ComponentRemoved(key, result) => {
                    self.setup_busy = false;
                    self.component_stats_at = Instant::now() - Duration::from_secs(30);
                    match result {
                        Ok(()) => {
                            self.status = format!("{key} uninstalled");
                            if key == "all" {
                                for path in [
                                    &mut self.s.server,
                                    &mut self.s.model,
                                    &mut self.s.projector,
                                    &mut self.s.ocr,
                                    &mut self.s.ffmpeg,
                                ] {
                                    if !Path::new(path).exists() {
                                        path.clear();
                                    }
                                }
                                self.save();
                            }
                        }
                        Err(e) => self.status = e,
                    }
                }

                Event::DictationState(id, text) => {
                    if id == self.dictation_job {
                        self.dictation_status = text;
                    }
                }
                Event::DictationDone(id, result) => {
                    if id == self.dictation_job {
                        self.dictation_busy = false;
                        let submit_ai = std::mem::take(&mut self.dictation_submit_ai);
                        match result.map(|text|crate::dictation::query_text(&text)).and_then(|text| {
                            if text.is_empty() {Err("Parakeet heard no recognizable speech. Check the microphone level and try again".into())}else{Ok(text)}
                        }) {
                            Ok(text) => {
                                self.query = text;
                                self.changed();
                                ctx.memory_mut(|m| m.request_focus(egui::Id::new("query")));
                                self.dictation_status = if self.saved_settings.dictation_keep_loaded
                                {
                                    "Dictation complete · Parakeet retained until idle".into()
                                } else {
                                    "Dictation complete · Parakeet unloaded".into()
                                };
                                if submit_ai && self.tool == "AI" && !self.query.trim().is_empty() {
                                    self.ask_ai();
                                }
                            }
                            Err(e) => self.dictation_status = e,
                        }
                    }
                }
                Event::DictationInstalled(result) => {
                    self.setup_busy = false;
                    self.setup_progress = None;
                    self.run(result.map_err(anyhow::Error::msg));
                }
                Event::ClipboardImage(id, result) => {
                    self.clipboard_thumb_pending.remove(&id);
                    {
                        match result {
                            Ok(bytes) => match crate::capture::rgba(&bytes) {
                                Ok(image) => {
                                    let thumb = image::imageops::thumbnail(&image, 112, 112);
                                    let texture = ctx.load_texture(
                                        format!("clipboard-thumb-{id}"),
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            [thumb.width() as usize, thumb.height() as usize],
                                            thumb.as_raw(),
                                        ),
                                        egui::TextureOptions::LINEAR,
                                    );
                                    if self.clipboard_thumbs.len() >= 64 {
                                        self.clipboard_thumbs.clear();
                                    }
                                    self.clipboard_thumbs.insert(id, texture);
                                    if id == self.clipboard_preview_id {
                                        self.clipboard_texture = Some(ctx.load_texture(
                                            "clipboard-preview",
                                            egui::ColorImage::from_rgba_unmultiplied(
                                                [image.width() as usize, image.height() as usize],
                                                image.as_raw(),
                                            ),
                                            egui::TextureOptions::LINEAR,
                                        ));
                                    }
                                }
                                Err(e) => self.status = e.to_string(),
                            },
                            Err(e) => self.status = e,
                        }
                    }
                }
                Event::CaptureTargets(result) => {
                    self.capture_busy = false;
                    match result {
                        Ok(targets) => self.capture_targets = targets,
                        Err(e) => self.ai_error = Some(e),
                    }
                }
                Event::Screenshot(result) => {
                    self.capture_busy = false;
                    match result {
                        Ok(bytes) => {
                            match crate::capture::rgba(&bytes) {
                                Ok(image) => {
                                    self.capture_texture = Some(ctx.load_texture(
                                        "screenshot-selection",
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            [image.width() as usize, image.height() as usize],
                                            image.as_raw(),
                                        ),
                                        egui::TextureOptions::LINEAR,
                                    ))
                                }
                                Err(e) => self.ai_error = Some(e.to_string()),
                            };
                            self.capture_bytes = Some(bytes);
                        }
                        Err(e) => self.ai_error = Some(e),
                    }
                }

                Event::TtsCatalog(url, model, result) => {
                    self.tts_catalog_busy = false;
                    if url == self.s.tts_url {
                        match result {
                            Ok((models, voices, notice)) => {
                                self.tts_models = models;
                                if model == self.s.tts_model {
                                    self.tts_voices = voices;
                                    self.tts_notice = notice;
                                }
                            }
                            Err(e) => self.tts_notice = e,
                        }
                    }
                }
                Event::TtsVoices(job, url, model, result) => {
                    if job == self.tts_voice_job
                        && url == self.s.tts_url
                        && model == self.s.tts_model
                    {
                        self.tts_voice_busy = false;
                        match result {
                            Ok((voices, notice)) => {
                                self.tts_voices = voices;
                                self.tts_notice = notice;
                            }
                            Err(e) => self.tts_notice = e,
                        }
                    }
                }
                Event::TtsDone(id, result) => {
                    if id == self.tts_job {
                        self.tts_busy = false;
                        self.tts_status = result.err().unwrap_or_default();
                    }
                }
                Event::AiMetrics(id, metrics) => {
                    if id == self.ai_job {
                        if metrics.prompt.is_some() {
                            self.ai_metrics.prompt = metrics.prompt;
                        }
                        if metrics.decode.is_some() {
                            self.ai_metrics.decode = metrics.decode;
                        }
                    }
                }
                Event::AiChunk(id, text) => {
                    if id == self.ai_job
                        && self.ai_busy
                        && self.ai_answer.len() + text.len() <= 128 * 1024
                    {
                        self.ai_answer.push_str(&text);
                    }
                }
                Event::AiDone(id, result) => {
                    if id == self.ai_job {
                        self.ai_busy = false;
                        match result {
                            Ok(()) => {
                                if !self.ai_answer.is_empty() {
                                    self.ai_messages.push(crate::ai::Message {
                                        role: "assistant".into(),
                                        content: self.ai_answer.clone(),
                                        images: vec![],
                                    });
                                }
                                if self.read_aloud && !self.ai_answer.is_empty() {
                                    self.start_read_aloud(self.ai_answer.clone());
                                }
                                if self.query.trim() == self.ai_question {
                                    self.query.clear();
                                }
                            }
                            Err(e) => {
                                self.ai_error = Some(e);
                                if self.ai_messages.last().is_some_and(|m| m.role == "user") {
                                    self.ai_messages.pop();
                                }
                            }
                        }
                    }
                }
                Event::AiModels(url, result) => {
                    self.llm_models_busy = false;
                    match result {
                        Ok(models) => {
                            if url == self.s.llm_url {
                                self.llm_models = models;
                            }
                            self.status = format!("{} LLM models available", self.llm_models.len());
                        }
                        Err(e) => self.status = e,
                    }
                }
                Event::Launcher(id, result) => {
                    self.searching = false;
                    if id == self.sequence {
                        match result {
                            Ok(items) => {
                                self.selected = self.selected.min(items.len().saturating_sub(1));
                                self.search_error = None;
                                self.tool_items = items;
                            }
                            Err(e) => {
                                self.search_error = Some(e.clone());
                                self.status = e;
                            }
                        }
                    }
                    self.dirty |= self.search_pending;
                    self.search_pending = false;
                }
                Event::ClipboardChanged => {
                    if self.tool == "Clipboard" && self.search_visible && self.page == 0 {
                        self.sequence += 1;
                        self.dirty = true;
                        self.changed = Instant::now();
                    }
                }
                Event::ClipboardCleared(result) => {
                    self.run(result.map_err(anyhow::Error::msg));
                    self.changed();
                }
                Event::ModelReady(result) => {
                    self.model_warm_error = result.err();
                    self.dirty = true;
                }
                Event::IndexCleared(result) => {
                    self.index_clearing = false;
                    self.setup_busy = false;
                    self.sequence += 1;
                    self.hits.clear();
                    self.texture = None;
                    self.thumbs.clear();
                    self.thumb_pending.clear();
                    self.thumb_failed.clear();
                    self.preview_pending = None;
                    self.preview_error = None;
                    self.query.clear();
                    self.dirty = false;
                    self.status = result.unwrap_or_else(|e| e);
                }
                Event::Update(result) => {
                    self.update_busy = false;
                    match result {
                        crate::updates::Update::Checked(Ok(Some(release))) => {
                            self.update_status = format!("Version {} available", release.version);
                            self.update_release = Some(release);
                            if self.s.auto_update && !crate::updates::portable() {
                                self.download_update();
                            }
                        }
                        crate::updates::Update::Checked(Ok(None)) => {
                            self.update_status = "You are up to date".into();
                            self.update_release = None;
                        }
                        crate::updates::Update::Downloaded(Ok(path)) => {
                            self.update_installer = Some(path);
                            self.update_status = "Update verified and ready to install".into();
                        }
                        crate::updates::Update::Checked(Err(e))
                        | crate::updates::Update::Downloaded(Err(e)) => {
                            self.update_status = format!("Update failed: {e}");
                            self.engine.log("Warning", &self.update_status);
                        }
                    }
                }
                Event::AutomaticStarted => {
                    self.automatic = true;
                    self.indexing = true;
                    self.progress = Progress::default();
                }
                Event::Download(name, done, total) => {
                    self.setup_progress = Some((name, done, total))
                }
                Event::Devices(devices) => {
                    self.devices_loading = false;
                    self.devices = devices;
                    if self.s.device.is_empty() && self.devices.len() == 1 {
                        self.s.device = self.devices[0].0.clone();
                        self.save();
                    }
                    self.status = if self.devices.is_empty() {
                        "No GPU devices found in this runtime. Choose/download the CUDA or Vulkan backend first.".into()
                    } else {
                        "Select a device from the list".into()
                    };
                }
                Event::Thumbnail(path, result) => {
                    self.thumb_pending.remove(&path);
                    if result.is_err() {
                        if self.thumb_failed.len() > 200 {
                            self.thumb_failed.clear();
                        }
                        self.thumb_failed.insert(path.clone());
                    }
                    if let Ok(bytes) = result {
                        if let Ok(image) = image::load_from_memory(&bytes) {
                            let rgba = image.to_rgba8();
                            let pixels = egui::ColorImage::from_rgba_unmultiplied(
                                [rgba.width() as usize, rgba.height() as usize],
                                &rgba,
                            );
                            if self.thumbs.len() >= 200 {
                                self.thumbs.clear();
                            }
                            self.thumbs.insert(
                                path,
                                ctx.load_texture("thumbnail", pixels, egui::TextureOptions::LINEAR),
                            );
                        }
                    }
                }
                Event::Converted(result) => {
                    self.setup_busy = false;
                    match result {
                        Ok(db) => {
                            self.engine.db = db;
                            self.s.encrypt_index = self.engine.db.key.is_some();
                            self.save();
                            self.apply_mcp();
                            self.status="Converted copy activated. Original retained; plaintext backups remain readable until you remove them.".into();
                        }
                        Err(e) => self.status = e,
                    }
                }
                Event::Quit => {
                    self.dictation_cancel.store(true, Ordering::Release);
                    self.exiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Event::Full => {
                    if !self.search_visible {
                        self.model_warm_attempted = false;
                        self.model_warm_error = None;
                        self.engine
                            .models
                            .session_override
                            .store(false, Ordering::Release);
                    }
                    self.search_visible = true;
                    self.quick = false;
                    self.page = 0;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        egui::WindowLevel::Normal,
                    ));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1120.0, 740.0,
                    )));
                }
                Event::WatchStatus(status) => self.watch_status = status,
                Event::ScopeStatus(seq, indexed) => {
                    if seq == self.sequence {
                        self.scope_indexed = Some(indexed);
                    }
                }
                Event::Progress(p) => self.progress = p,
                Event::IndexDone(result) => {
                    self.indexing = false;
                    if result.is_ok() {
                        self.s.last_success = chrono::Local::now().timestamp();
                    }
                    if self.automatic {
                        let latest = self.engine.control.schedule.lock().unwrap().clone();
                        self.s.last_success = latest.last_success;
                        self.s.last_schedule_slot = latest.last_schedule_slot;
                        self.s.schedule_history = latest.schedule_history;
                        self.automatic = false;
                    }
                    self.status = result.unwrap_or_else(|e| e);
                }
                Event::Stats(stats, sources) => {
                    self.stats = stats;
                    self.source_stats = sources;
                }
                Event::Search(seq, result, ms) => {
                    self.searching = false;
                    if seq == self.sequence {
                        match result {
                            Ok(h) => {
                                self.hits = h;
                                self.selected = self
                                    .qa_selection
                                    .unwrap_or(0)
                                    .min(self.hits.len().saturating_sub(1));
                                self.scroll_to_selection = true;
                                self.texture = None;
                                self.preview_error = None;
                                self.preview_pending = None;
                                self.search_error = None;
                                self.search_ms = ms;
                                self.status = format!("{} results · {ms:.1} ms", self.hits.len());
                            }
                            Err(e) => {
                                self.search_error = Some(e.clone());
                                self.status = e;
                            }
                        }
                    }
                    if self.search_pending {
                        self.search_pending = false;
                        self.dispatch();
                    }
                }
                Event::Log(level, text) => {
                    let line = format!(
                        "{} {text}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                    );
                    self.logs.push((level, line));
                    if self.logs.len() > 3000 {
                        self.logs.drain(..500);
                    }
                }
                Event::Setup(result) => {
                    self.setup_progress = None;
                    self.setup_busy = false;
                    match result {
                        Ok(paths) => {
                            for (k, p) in paths {
                                match k.as_str() {
                                    "server" => {
                                        self.s.server = p;
                                        if self.s.backend() == "cuda-"
                                            && !self
                                                .s
                                                .device
                                                .to_ascii_uppercase()
                                                .starts_with("CUDA")
                                            || self.s.backend() == "vulkan"
                                                && self
                                                    .s
                                                    .device
                                                    .to_ascii_uppercase()
                                                    .starts_with("CUDA")
                                        {
                                            self.s.device.clear();
                                        }
                                    }
                                    "model" => self.s.model = p,
                                    "projector" => self.s.projector = p,
                                    "ocr" => self.s.ocr = p,
                                    _ => {}
                                }
                            }
                            self.save();
                            self.status =
                                "Installed and verified. Models remain unloaded until needed."
                                    .into();
                        }
                        Err(e) => self.status = e,
                    }
                }
                Event::Summon(target, text) | Event::ScopedSummon(target, text) => {
                    self.stop_ai();
                    self.tool = "Search".into();
                    self.tool_items.clear();
                    if self.qa_hidden {
                        self.qa_hidden_reopened = true;
                    }
                    self.search_visible = true;
                    self.model_warm_attempted = false;
                    self.model_warm_error = None;
                    self.engine
                        .models
                        .session_override
                        .store(false, Ordering::Release);
                    self.large_preview = false;
                    self.quick = true;
                    self.popup_size_state = None;
                    self.popup_had_focus = false;
                    self.page = 0;
                    self.target = target;
                    if scoped_summon {
                        self.scope = text;
                        self.query.clear();
                    } else {
                        self.scope.clear();
                        self.query = text;
                    }
                    self.hits.clear();
                    self.dirty = true;
                    self.changed = Instant::now();
                    ctx.memory_mut(|m| {
                        m.move_focus(egui::FocusDirection::None);
                        m.request_focus(egui::Id::new("query"));
                    });
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        940.0, 245.0,
                    )));
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        egui::WindowLevel::AlwaysOnTop,
                    ));
                    if voice_summon {
                        self.toggle_dictation();
                    }
                }
                Event::Preview(path, result) => {
                    self.preview_pending = None;
                    if self.hits.get(self.selected).is_some_and(|h| h.path == path) {
                        match result {
                            Ok(bytes) => {
                                if let Ok(image) = image::load_from_memory(&bytes) {
                                    let rgba = image.to_rgba8();
                                    let pixels = egui::ColorImage::from_rgba_unmultiplied(
                                        [rgba.width() as usize, rgba.height() as usize],
                                        &rgba,
                                    );
                                    self.texture = Some((
                                        path,
                                        ctx.load_texture(
                                            "preview",
                                            pixels,
                                            egui::TextureOptions::LINEAR,
                                        ),
                                    ));
                                }
                            }
                            Err(e) => {
                                self.preview_pending = Some(path);
                                self.preview_error = Some(e.clone());
                                self.log("Warning".into(), format!("Preview: {e}"));
                            }
                        }
                    }
                }
                Event::Imported(result) => {
                    self.setup_busy = false;
                    match result {
                        Ok(mut s) => {
                            s.encrypt_index = self.s.encrypt_index;
                            self.s = s;
                            if let Ok(d) = Database::new(&self.home, &self.s) {
                                self.engine.db = d;
                                self.engine.vectors.clear();
                            }
                            self.save();
                            self.engine.refresh_stats(&self.s);
                            self.status =
                                "Imported a copy; the installed app and its index are unchanged"
                                    .into();
                        }
                        Err(e) => self.status = e,
                    }
                }
            }
        }
    }
    fn warm_search_model(&mut self) {
        if self.dictation_busy {
            return;
        }

        if self.last_search_mode != self.mode {
            self.last_search_mode = self.mode.clone();
            self.model_warm_attempted = false;
        }
        if !self.search_visible || self.page != 0 || self.mode == "Names" || self.tool != "Search" {
            self.engine
                .models
                .session_override
                .store(false, Ordering::Release);
            return;
        }
        if self.engine.models.search_hold(&self.s).is_some() {
            self.model_warm_attempted = false;
            self.model_warm_error = None;
            return;
        }
        if self.engine.models.loaded.load(Ordering::Acquire)
            && !self.engine.models.loading.load(Ordering::Acquire)
        {
            self.model_warm_error = None;
        }
        if self.model_warm_attempted
            || !self.s.remote_url.is_empty()
            || self.engine.models.search_hold(&self.s).is_some()
            || self.engine.models.loading.load(Ordering::Acquire)
            || self.index_clearing
        {
            return;
        }
        if !Path::new(&self.s.server).is_file() || !Path::new(&self.s.model).is_file() {
            return;
        }
        self.model_warm_attempted = true;
        let models = self.engine.models.clone();
        let s = self.s.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let result = models.start(&s, false).map_err(|e| e.to_string());
            let _ = tx.send(Event::ModelReady(result));
        });
    }
    fn model_search_panel(&mut self, ui: &mut egui::Ui) {
        if self.mode == "Names" || !self.s.remote_url.is_empty() {
            return;
        }
        if self.engine.models.loading.load(Ordering::Acquire) {
            let phase = self.engine.models.loading_phase();
            let (rect, response) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 7.0, ui.visuals().widgets.inactive.bg_fill);
            let span = rect.width() * 0.22;
            let t = (ui.input(|i| i.time) * 0.8).fract() as f32;
            let left = rect.left() - span + t * (rect.width() + span);
            let moving = egui::Rect::from_min_size(
                egui::pos2(left, rect.top()),
                egui::vec2(span, rect.height()),
            )
            .intersect(rect);
            ui.painter()
                .rect_filled(moving, 7.0, self.accent().gamma_multiply(0.45));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                phase,
                egui::FontId::proportional(12.0),
                ui.visuals().text_color(),
            );
            response.on_hover_text(
                "llama-server reports readiness rather than an exact loading percentage.",
            );
            return;
        }
        let held = self.engine.models.search_hold(&self.s);
        if held.is_some()
            || (self.s.show_model_notices && !self.engine.models.loaded.load(Ordering::Acquire))
        {
            ui.horizontal_wrapped(|ui| {
                let notice = if let Some(reason) = held {
                    format!("EmbeddingGemma 2 {}: {reason}", if self.engine.models.loaded.load(Ordering::Acquire) {"paused"} else {"unloaded"})
                } else { "EmbeddingGemma 2 is unloaded. Hybrid/Semantic searches may need a few seconds to load it.".into() };
                ui.label(RichText::new(notice).color(crate::appearance::component_color(ui.visuals().dark_mode,false,true)));
                if held.is_some() && ui.button("Load anyway").clicked() {
                    self.engine.models.session_override.store(true, Ordering::Release);
                    self.model_warm_attempted = false; self.model_warm_error = None;
                    self.dirty = true;
                }
            });
        }
        if let Some(error) = &self.model_warm_error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
    }
    fn dispatch(&mut self) {
        if self.index_clearing || !self.search_visible || self.page != 0 {
            return;
        }
        if self.tool == "AI" {
            return;
        }
        if self.tool == "Emoji" {
            self.tool_items = crate::launcher::emoji(&self.query);
            return;
        }
        if self.tool == "Clipboard" {
            if self.searching {
                self.search_pending = true;
                return;
            }
            self.searching = true;
            let store = crate::launcher::Clipboard::new(&self.home);
            let q = self.query.clone();
            let settings = self.saved_settings.clone();
            let models = self.engine.models.clone();
            let id = self.sequence;
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let result = store
                    .search_with_images(&q, &settings, &models)
                    .map_err(|e| e.to_string());
                let _ = tx.send(Event::Launcher(id, result));
            });
            return;
        }
        if self.mode != "Names" && self.engine.models.loading.load(Ordering::Acquire) {
            self.dirty = true;
            return;
        }
        if self.query.trim().is_empty() {
            if self.quick && self.changed.elapsed() < Duration::from_millis(200) {
                self.dirty = true;
                return;
            }
            self.hits.clear();
            return;
        }
        if self.searching {
            self.search_pending = true;
            return;
        }
        self.searching = true;
        self.engine.search(
            self.sequence,
            self.query.clone(),
            self.s.clone(),
            self.mode.clone(),
            self.category.clone(),
            self.scope.clone(),
            self.exclude_types.clone(),
        );
    }
    fn changed(&mut self) {
        self.search_error = None;
        self.scope_indexed = None;
        self.sequence += 1;
        self.changed = Instant::now();
        self.dirty = true;
    }
    fn start_index(&mut self, force: bool) {
        if self.indexing || self.setup_busy {
            return;
        }
        if !self.save() {
            return;
        }
        self.indexing = true;
        self.progress = Progress::default();
        self.engine.index(self.s.clone(), force);
    }
    fn check_update(&mut self) {
        if self.update_busy {
            return;
        }
        self.update_busy = true;
        self.update_status = "Checking GitHub releases…".into();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Update(crate::updates::Update::Checked(
                crate::updates::check().map_err(|e| e.to_string()),
            )));
        });
    }
    fn download_update(&mut self) {
        if self.update_busy {
            return;
        }
        if let Some(release) = self.update_release.clone() {
            self.update_busy = true;
            self.update_status = "Downloading and verifying update…".into();
            let home = self.home.clone();
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Event::Update(crate::updates::Update::Downloaded(
                    crate::updates::download(&release, &home).map_err(|e| e.to_string()),
                )));
            });
        }
    }
    fn install_update(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.update_installer.take() {
            if let Some(release) = &self.update_release {
                if let Err(e) = crate::updates::verify(&path, release) {
                    self.update_status = e.to_string();
                    return;
                }
            }
            let _ = self.engine.models.stop();
            match crate::updates::install(&path) {
                Ok(()) => {
                    self.exiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(e) => self.update_status = e.to_string(),
            }
        }
    }
    fn apply_mcp(&mut self) {
        self.mcp = None;
        if self.s.mcp_enabled {
            match crate::mcp::Server::start(self.engine.clone(), self.s.clone()) {
                Ok(server) => {
                    self.mcp = Some(server);
                    self.status = "MCP server started".into();
                }
                Err(e) => self.status = e.to_string(),
            }
        }
    }
    fn run(&mut self, result: Result<()>) {
        if let Err(e) = result {
            self.status = e.to_string();
        }
    }
    fn close_window(&mut self, ctx: &egui::Context) {
        self.dictation_cancel.store(true, Ordering::Release);
        self.dictation_busy = false;
        self.dictation_job += 1;
        self.capture_open = false;
        self.capture_bytes = None;
        self.capture_texture = None;

        self.stop_ai();
        self.tool_items.clear();
        self.texture = None;
        self.clipboard_texture = None;
        self.preview_pending = None;
        let _ = self.window_state.save(&self.home);
        self.search_visible = false;
        self.engine
            .models
            .session_override
            .store(false, Ordering::Release);
        self.large_preview = false;
        if self.quick {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            return;
        }
        match self.s.close_behavior.as_str() {
            "Exit" => {
                self.exiting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close)
            }
            "Minimize to taskbar" => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            _ => ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false)),
        }
    }
    fn titlebar(&mut self, ctx: &egui::Context) {
        use crate::icons::{self, Icon};
        egui::TopBottomPanel::top("title")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(20, 12)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.logo).fit_to_exact_size(egui::vec2(28.0, 28.0)));
                    let response = ui.add(
                        egui::Label::new(
                            RichText::new("VectorDash")
                                .strong()
                                .size(if self.quick { 19.0 } else { 24.0 })
                                .color(self.accent()),
                        )
                        .sense(egui::Sense::click_and_drag()),
                    );
                    crate::platform::add_drag_region(
                        [
                            response.rect.min.x,
                            response.rect.min.y,
                            response.rect.max.x,
                            response.rect.max.y,
                        ],
                        ctx.pixels_per_point(),
                    );
                    if response.drag_started()
                        || ctx.input(|i| {
                            i.pointer.primary_pressed()
                                && i.pointer
                                    .interact_pos()
                                    .is_some_and(|p| response.rect.contains(p))
                        })
                    {
                        crate::platform::drag_main_window();
                    }
                    if !self.quick
                        && ui
                            .add(
                                egui::Button::image(icons::image(Icon::Menu, self.accent(), 20.0))
                                    .frame(true),
                            )
                            .on_hover_text(if self.collapsed {
                                "Expand sidebar"
                            } else {
                                "Collapse sidebar"
                            })
                            .clicked()
                    {
                        self.collapsed = !self.collapsed;
                    }
                    if !self.quick
                        && self.page != 0
                        && ui.button("Back to search · Ctrl+F").clicked()
                    {
                        self.page = 0;
                        ctx.memory_mut(|m| m.request_focus(egui::Id::new("query")));
                    }
                    let reserve = if self.quick { 220.0 } else { 112.0 };
                    let (_, response) = ui.allocate_exact_size(
                        egui::vec2((ui.available_width() - reserve).max(0.0), 30.0),
                        egui::Sense::click_and_drag(),
                    );
                    crate::platform::add_drag_region(
                        [
                            response.rect.min.x,
                            response.rect.min.y,
                            response.rect.max.x,
                            response.rect.max.y,
                        ],
                        ctx.pixels_per_point(),
                    );
                    if response.drag_started()
                        || ctx.input(|i| {
                            i.pointer.primary_pressed()
                                && i.pointer
                                    .interact_pos()
                                    .is_some_and(|p| response.rect.contains(p))
                        })
                    {
                        crate::platform::drag_main_window();
                    }
                    if response.double_clicked() && !self.quick {
                        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let color = ui.visuals().text_color();
                        if ui
                            .add(
                                egui::Button::image(icons::image(Icon::Close, color, 18.0))
                                    .frame(true),
                            )
                            .on_hover_text("Close")
                            .clicked()
                        {
                            self.close_window(ctx);
                        }
                        if !self.quick
                            && ui
                                .add(
                                    egui::Button::image(icons::image(Icon::Maximize, color, 18.0))
                                        .frame(true),
                                )
                                .on_hover_text("Maximize / restore")
                                .clicked()
                        {
                            let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }
                        if ui
                            .add(
                                egui::Button::image(icons::image(Icon::Minimize, color, 18.0))
                                    .frame(true),
                            )
                            .on_hover_text("Minimize")
                            .clicked()
                        {
                            ctx.send_viewport_cmd(if self.s.minimize_behavior == "Tray" {
                                egui::ViewportCommand::Visible(false)
                            } else {
                                egui::ViewportCommand::Minimized(true)
                            });
                        }
                        if self.quick && ui.button("Open full app").clicked() {
                            self.quick = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                                780.0, 540.0,
                            )));
                            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                                1120.0, 740.0,
                            )));
                            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                                egui::WindowLevel::Normal,
                            ));
                        }
                    });
                });
            });
    }
    fn search_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.quick {
            ui.heading("What are you looking for?");
            ui.label("Find files, meanings, programs and Windows settings.");
            ui.add_space(8.0);
        }
        let mut query = self.query.clone();
        let recording = crate::dictation::recording()
            || self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-recording");
        if recording {
            query.clear();
        }
        ui.horizontal(|ui| {
            let drag = ui.add(
                crate::icons::image(crate::icons::Icon::Search, self.accent(), 26.0)
                    .sense(egui::Sense::drag()),
            );
            if self.quick {
                crate::platform::add_drag_region(
                    [
                        drag.rect.min.x,
                        drag.rect.min.y,
                        drag.rect.max.x,
                        drag.rect.max.y,
                    ],
                    ctx.pixels_per_point(),
                );
            }
            if self.quick
                && (drag.drag_started()
                    || ctx.input(|i| {
                        i.pointer.primary_pressed()
                            && i.pointer
                                .interact_pos()
                                .is_some_and(|p| drag.rect.contains(p))
                    }))
            {
                crate::platform::drag_main_window();
            }
            let response = ui.add_sized(
                [
                    (ui.available_width()
                        - if self.tool == "Search" && !self.query.trim().is_empty() {
                            212.0
                        } else if self.tool == "AI" && !self.ai_images.is_empty() {
                            190.0
                        } else {
                            96.0
                        })
                    .max(100.0),
                    42.0,
                ],
                egui::TextEdit::singleline(&mut query)
                    .lock_focus(true)
                    .interactive(!recording)
                    .text_color(if recording {
                        Color32::TRANSPARENT
                    } else {
                        ui.visuals().text_color()
                    })
                    .hint_text(if recording {
                        ""
                    } else {
                        match self.tool.as_str() {
                            "AI" => "Ask AI",
                            "Emoji" => "Search emoji",
                            "Clipboard" => "Search clipboard history",
                            _ => "Search Anything",
                        }
                    })
                    .font(egui::TextStyle::Heading)
                    .id(egui::Id::new("query")),
            );
            if recording {
                self.paint_waveform(ui, response.rect);
            }
            if self.select_query && !recording {
                self.select_query = false;
                let id = egui::Id::new("query");
                let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(self.query.chars().count()),
                    )));
                state.store(ctx, id);
                response.request_focus();
            }
            if self.quick && self.startup.elapsed() < Duration::from_secs(2) {
                response.request_focus();
            }
            if response.changed() {
                self.query = query;
                if self.tool == "Search" {
                    if self.query.trim_start().starts_with("emoji:") {
                        self.tool = "Emoji".into();
                    } else if self.saved_settings.clipboard_enabled
                        && self.query.trim_start().starts_with("clipboard:")
                    {
                        self.tool = "Clipboard".into();
                    }
                }
                self.changed();
            }
            if self.tool == "AI" && !self.ai_images.is_empty() {
                let chip = if let Some(texture) = &self.ai_attachment {
                    ui.add(egui::Button::image_and_text(
                        egui::Image::new(texture).fit_to_exact_size(egui::vec2(36.0, 28.0)),
                        "×",
                    ))
                } else {
                    ui.button("Screenshot ×")
                };
                chip.clone().on_hover_ui(|ui| {
                    if let Some(texture) = &self.ai_attachment {
                        ui.add(egui::Image::new(texture).max_size(egui::vec2(240.0, 160.0)));
                    }
                    ui.label("Screenshot attached. Click to remove.");
                });
                if chip.clicked() {
                    self.ai_images.clear();
                    self.ai_attachment = None;
                }
            }
            if self.tool == "Search"
                && !self.query.trim().is_empty()
                && ui.button("Save Search").clicked()
            {
                self.save_search_name = self.query.clone();
                self.save_search_open = true;
            }
            if ui
                .add_sized(
                    [36.0, 36.0],
                    egui::Button::image(crate::icons::image(
                        crate::icons::Icon::Emoji,
                        self.accent(),
                        22.0,
                    ))
                    .selected(self.tool == "Emoji"),
                )
                .on_hover_text("Emoji")
                .clicked()
            {
                self.switch_tool("Emoji");
            }
            let (mic_rect, mic) =
                ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::click());
            let red = if mic.hovered() {
                Color32::from_rgb(225, 45, 65)
            } else {
                Color32::from_rgb(195, 35, 55)
            };
            ui.painter().circle_filled(mic_rect.center(), 18.0, red);
            ui.painter().circle_stroke(
                mic_rect.center(),
                18.0,
                egui::Stroke::new(1.0, self.accent()),
            );
            crate::icons::image(crate::icons::Icon::Microphone, Color32::WHITE, 20.0)
                .paint_at(ui, mic_rect.shrink(8.0));
            if mic
                .on_hover_text(if self.dictation_busy {
                    "Finish dictation"
                } else {
                    "Dictate · shortcut configurable in Models & hardware"
                })
                .clicked()
            {
                self.toggle_dictation();
            }
        });
        let old = (
            self.mode.clone(),
            self.category.clone(),
            self.scope.clone(),
            self.exclude_types.clone(),
        );
        self.qa_toolbar_top = ui.cursor().min.y;
        self.qa_filter_top = None;
        ui.horizontal_top(|ui| {
            for label in self.saved_settings.search_tools() {
                if ui
                    .add(
                        egui::Button::image_and_text(
                            crate::icons::image(
                                match label {
                                    "Search" => crate::icons::Icon::Search,
                                    "Emoji" => crate::icons::Icon::Emoji,
                                    "Clipboard" => crate::icons::Icon::Clipboard,
                                    _ => crate::icons::Icon::Robot,
                                },
                                self.accent(),
                                18.0,
                            ),
                            label,
                        )
                        .selected(self.tool == label),
                    )
                    .clicked()
                {
                    self.switch_tool(label);
                }
            }
            if self.tool == "AI" {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(88.0, 54.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.spacing_mut().interact_size.y = 16.0;
                            ui.spacing_mut().icon_width = 12.0;
                            ui.spacing_mut().icon_width_inner = 8.0;
                            if self.ai_busy {
                                if ui
                                    .add_sized([88.0, 34.0], egui::Button::new("Stop"))
                                    .clicked()
                                {
                                    self.stop_ai();
                                    self.ai_error = Some("Request stopped".into());
                                }
                            } else if ui
                                .add_enabled(
                                    !self.query.trim().is_empty() && !self.dictation_busy,
                                    egui::Button::new("Send").min_size(egui::vec2(88.0, 34.0)),
                                )
                                .clicked()
                            {
                                self.ask_ai();
                            }
                            ui.add(egui::Checkbox::new(
                                &mut self.read_aloud,
                                RichText::new("Read Aloud").size(11.0),
                            ))
                            .on_hover_text(
                                "Automatically read completed AI replies using your TTS server",
                            );
                        },
                    );
                    if ui
                        .add_enabled(!self.ai_busy, egui::Button::new("New Conversation"))
                        .clicked()
                    {
                        self.new_conversation();
                    }
                egui::ComboBox::from_id_salt("ai-response-length").width(125.0).selected_text(format!("Response: {}",self.response_length)).show_ui(ui,|ui|{for style in ["Brief","Medium","Full"]{ui.selectable_value(&mut self.response_length,style.into(),style);}}).response.on_hover_text("Response length: Brief (3–4 sentences), Medium (4–6), Full (model default)");
                    if ui
                        .add_enabled(
                            !self.ai_busy && self.saved_settings.llm_vision,
                            egui::Button::image_and_text(
                                crate::icons::image(
                                    crate::icons::Icon::Camera,
                                    self.accent(),
                                    18.0,
                                ),
                                "Screenshot",
                            ),
                        )
                        .on_hover_text("Requires a vision model enabled in AI settings")
                        .clicked()
                    {
                        self.capture_open = true;
                        self.capture_preview_key = None;
                        self.capture_busy = true;
                        self.capture_bytes = None;
                        self.capture_texture = None;
                        self.capture_rect = None;
                        let tx = self.engine.tx.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(Event::CaptureTargets(
                                crate::capture::targets().map_err(|e| e.to_string()),
                            ));
                        });
                    }
                });
            }
            if self.tool != "Search" {
                return;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                let mode=combo(
                    ui,
                    "search_mode",
                    &mut self.mode,
                    &["Both", "Names", "Semantic"],
                );
                self.qa_filter_top=Some(mode.rect.top());
                let filtered = self.category != "All files"
                    || !self.scope.is_empty()
                    || !self.exclude_types.trim().is_empty();
                let filter = ui.button(if filtered { "Filter •" } else { "Filter" });
                if filter.clicked() {
                    self.filter_open = !self.filter_open;
                    self.filter_anchor = filter.rect.left_bottom();
                }
                let mut filter_open = self.filter_open;
                egui::Window::new("Filter")
                    .id(egui::Id::new("search-filter-panel"))
                    .open(&mut filter_open)
                    .collapsible(false)
                    .resizable(false)
                    .movable(false)
                    .fixed_pos(self.filter_anchor)
                    .show(ctx, |ui| {
                        ui.set_min_width(240.0);
                        ui.label("File type");
                        let response = combo(ui, "category", &mut self.category, &CATEGORIES);
                        if self
                            .type_flash
                            .is_some_and(|t| t.elapsed() < Duration::from_millis(700))
                        {
                            ui.painter().rect_stroke(
                                response.rect.expand(1.0),
                                9.0,
                                egui::Stroke::new(2.0, self.accent()),
                                egui::StrokeKind::Outside,
                            );
                        }
                        ui.separator();
                        ui.label("Sources");
                        let mut scopes: Vec<String> = if self.scope.starts_with('[') {
                            serde_json::from_str(&self.scope).unwrap_or_default()
                        } else if self.scope.is_empty() {
                            vec![]
                        } else {
                            vec![self.scope.clone()]
                        };
                        egui::ComboBox::from_id_salt("scope")
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .selected_text(if scopes.is_empty() {
                                "All sources".into()
                            } else if scopes.len() == 1 {
                                std::path::Path::new(scopes[0].trim_end_matches(['\\', '/']))
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| scopes[0].clone())
                            } else {
                                format!("{} sources", scopes.len())
                            })
                            .width(180.0)
                            .truncate()
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(scopes.is_empty(), "All sources")
                                    .clicked()
                                {
                                    scopes.clear();
                                }
                                ui.separator();
                                let mut choices = self
                                    .s
                                    .roots
                                    .iter()
                                    .map(|r| r.path.clone())
                                    .collect::<Vec<_>>();
                                for path in &scopes {
                                    if !choices.contains(path) {
                                        choices.push(path.clone());
                                    }
                                }
                                for path in choices {
                                    let mut selected = scopes.contains(&path);
                                    if ui.checkbox(&mut selected, &path).changed() {
                                        if selected {
                                            scopes.push(path);
                                        } else {
                                            scopes.retain(|p| p != &path);
                                        }
                                    }
                                }
                            });
                        self.scope = match scopes.len() {
                            0 => String::new(),
                            1 => scopes.remove(0),
                            _ => serde_json::to_string(&scopes).unwrap_or_default(),
                        };
                        ui.separator();
                        ui.label("Exclude file types");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.exclude_types)
                                .hint_text("Exclude types: *.zip, *.log")
                                .desired_width(150.0),
                        );
                    });
                self.filter_open = filter_open;
                let mut selected = None;
                egui::ComboBox::from_id_salt("saved-searches")
                    .selected_text("Saved searches")
                    .width(120.0)
                    .truncate()
                    .show_ui(ui, |ui| {
                        for (i, search) in self.s.saved_searches.iter().enumerate() {
                            if ui.selectable_label(false, &search.name).clicked() {
                                selected = Some(i);
                            }
                        }
                    });
                if let Some(i) = selected {
                    let saved = self.s.saved_searches[i].clone();
                    self.query = saved.query;
                    self.mode = saved.mode;
                    self.category = saved.category;
                    self.scope = saved.scope;
                    self.exclude_types = saved.excluded;
                    self.changed();
                }
            });
        });
        if self.tool != "Search" {
            self.tools_ui(ui, ctx);
            return;
        }
        if old
            != (
                self.mode.clone(),
                self.category.clone(),
                self.scope.clone(),
                self.exclude_types.clone(),
            )
        {
            self.changed();
        }
        self.model_search_panel(ui);
        if self.quick
            && self.query.is_empty()
            && ctx.animate_bool_with_time(egui::Id::new("popup-expansion"), false, 0.18) <= 0.0
        {
            return;
        }
        if !self.scope.is_empty()
            && !self.scope.starts_with('[')
            && (self.scope_indexed == Some(false)
                || !self
                    .s
                    .roots
                    .iter()
                    .any(|r| crate::config::under(&self.scope, &r.path)))
        {
            ui.label(
                RichText::new(
                    "This folder or drive has not been indexed. Add it in Sources to search it.",
                )
                .color(self.accent()),
            );
        }
        ui.label(
            RichText::new(if self.searching {
                "Searching…".into()
            } else {
                format!("{} results · {:.1} ms", self.hits.len(), self.search_ms)
            })
            .weak(),
        );
        let available = ui.available_size();
        let width = available.x;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(width * 0.58, available.y - 38.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let mut scroll = egui::ScrollArea::vertical().id_salt("results");
                    if self.scroll_to_selection {
                        let current =
                            egui::scroll_area::State::load(ctx, ui.make_persistent_id("results"))
                                .map(|s| s.offset.y)
                                .unwrap_or(0.0);
                        let offset = selection_offset(
                            self.selected,
                            62.0 + ui.spacing().item_spacing.y,
                            current,
                            (available.y - 38.0).max(62.0),
                        );
                        scroll = scroll.vertical_scroll_offset(offset);
                        self.scroll_to_selection = false;
                    }
                    let output = scroll.show_rows(ui, 62.0, self.hits.len(), |ui, range| {
                        for i in range {
                            let hit = self.hits[i].clone();
                            let name = Path::new(&hit.path)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned();
                            let title = if hit.kind == "calculator" {
                                hit.path.clone()
                            } else if hit.kind == "settings" || hit.kind == "application" {
                                hit.text.clone()
                            } else {
                                name
                            };
                            if !["settings", "calculator", "folder"].contains(&hit.kind.as_str())
                                && !self.thumbs.contains_key(&hit.path)
                                && !self.thumb_pending.contains(&hit.path)
                                && !self.thumb_failed.contains(&hit.path)
                            {
                                if crate::previews::thumbnail(
                                    hit.path.clone(),
                                    self.s.clone(),
                                    self.engine.tx.clone(),
                                )
                                .is_ok()
                                {
                                    self.thumb_pending.insert(hit.path.clone());
                                }
                            }
                            let row_width = ui.available_width();
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(row_width, 62.0),
                                egui::Sense::click(),
                            );
                            let selected = self.selected == i;
                            let dark = crate::appearance::dark(ctx, &self.s);
                            let accent = self.accent();
                            let fill = if selected {
                                Color32::from_rgba_unmultiplied(
                                    self.s.accent[0],
                                    self.s.accent[1],
                                    self.s.accent[2],
                                    if dark { 65 } else { 38 },
                                )
                            } else if response.hovered() {
                                Color32::from_rgba_unmultiplied(
                                    self.s.accent[0],
                                    self.s.accent[1],
                                    self.s.accent[2],
                                    if dark { 38 } else { 22 },
                                )
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter().rect_filled(rect, 9.0, fill);
                            if selected {
                                ui.painter().rect_stroke(
                                    rect,
                                    9.0,
                                    egui::Stroke::new(1.0, accent.gamma_multiply(0.7)),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            let icon_rect = egui::Rect::from_min_size(
                                rect.min + egui::vec2(10.0, 9.0),
                                egui::vec2(42.0, 42.0),
                            );
                            if let Some(texture) = self.thumbs.get(&hit.path) {
                                ui.put(
                                    icon_rect,
                                    egui::Image::new(texture).fit_to_exact_size(icon_rect.size()),
                                );
                            } else {
                                ui.put(
                                    icon_rect,
                                    crate::icons::image(
                                        crate::icons::result(&hit.kind, &hit.path),
                                        accent,
                                        28.0,
                                    ),
                                );
                            }
                            let title_rect = egui::Rect::from_min_size(
                                rect.min + egui::vec2(64.0, 8.0),
                                egui::vec2((row_width - 76.0).max(1.0), 23.0),
                            );
                            let mut title_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(title_rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Min)),
                            );
                            title_ui.add(
                                egui::Label::new(
                                    RichText::new(&title)
                                        .strong()
                                        .size(14.0)
                                        .color(ui.visuals().text_color()),
                                )
                                .truncate()
                                .selectable(false)
                                .sense(egui::Sense::hover()),
                            );
                            let location = if hit.kind == "application" {
                                "Windows application".into()
                            } else if hit.kind == "settings" {
                                hit.path.clone()
                            } else {
                                Path::new(&hit.path)
                                    .parent()
                                    .unwrap_or(Path::new(""))
                                    .display()
                                    .to_string()
                            };
                            let tag = match hit.kind.as_str() {
                                "application" => "App",
                                "settings" => "Settings",
                                "folder" => "Folder",
                                _ => hit.kind.as_str(),
                            };
                            let mut job = egui::text::LayoutJob::default();
                            job.append(
                                tag,
                                0.0,
                                egui::TextFormat {
                                    font_id: egui::FontId::new(
                                        12.0,
                                        egui::FontFamily::Name("bold".into()),
                                    ),
                                    color: accent,
                                    ..Default::default()
                                },
                            );
                            job.append(
                                &format!(" · {location}"),
                                0.0,
                                egui::TextFormat {
                                    font_id: egui::FontId::proportional(12.0),
                                    color: ui.visuals().weak_text_color(),
                                    ..Default::default()
                                },
                            );
                            let metadata_rect = egui::Rect::from_min_size(
                                rect.min + egui::vec2(64.0, 33.0),
                                egui::vec2((row_width - 76.0).max(1.0), 20.0),
                            );
                            let mut metadata_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(metadata_rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Min)),
                            );
                            metadata_ui.add(
                                egui::Label::new(job)
                                    .truncate()
                                    .selectable(false)
                                    .sense(egui::Sense::hover()),
                            );
                            if response.clicked() {
                                ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("query")));
                                self.selected = i;
                                self.texture = None;
                                self.preview_error = None;
                                self.preview_pending = None;
                            }
                            if response.double_clicked() {
                                self.selected = i;
                                self.open_selected(ctx);
                            }
                            response.context_menu(|ui| {
                                if ui.button("Open").clicked() {
                                    self.selected = i;
                                    self.open_selected(ctx);
                                    ui.close();
                                }
                                if ui.button("Open file location").clicked() {
                                    self.run(crate::platform::reveal(&hit.path));
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(
                                        !["calculator", "settings"].contains(&hit.kind.as_str()),
                                        egui::Button::new("Copy file"),
                                    )
                                    .clicked()
                                {
                                    self.run(crate::platform::copy_file(&hit.path));
                                    ui.close();
                                }
                                if ui.button("Copy path").clicked() {
                                    ctx.copy_text(hit.path.clone());
                                    ui.close();
                                }
                                if ui.button("Exclude from future searches").clicked() {
                                    self.s.exclusions.push(hit.path.clone());
                                    self.save();
                                    self.changed();
                                    ui.close();
                                }
                                if ui.button("Properties").clicked() {
                                    self.run(crate::platform::properties(&hit.path));
                                    ui.close();
                                }
                                if ui.button("Delete…").clicked() {
                                    self.delete_confirm = Some(hit.clone());
                                    ui.close();
                                }
                            });
                        }
                    });
                    self.result_scroll_offset = output.state.offset.y;
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(width * 0.38, available.y - 38.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.preview(ui, ctx),
            );
        });
    }
    fn preview(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let Some(hit) = self.hits.get(self.selected).cloned() else {
            ui.allocate_ui_with_layout(
                ui.available_size(),
                egui::Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    ui.label("Select a result to preview it.");
                },
            );
            return;
        };
        if content_image(&hit.path)
            || hit.path.to_lowercase().ends_with(".pdf")
            || hit.kind == "pdf"
            || hit.kind == "application"
            || crate::content::is_video(&crate::content::extension(Path::new(&hit.path)))
        {
            if self.texture.as_ref().is_none_or(|(p, _)| p != &hit.path)
                && self.preview_pending.as_deref() != Some(&hit.path)
            {
                let path = hit.path.clone();
                self.preview_pending = Some(path.clone());
                let s = self.s.clone();
                let query = self.query.clone();
                let application = hit.kind == "application";
                let tx = self.engine.tx.clone();
                std::thread::spawn(move || {
                    let result = if application {
                        crate::platform::icon_png(&path)
                    } else if crate::content::is_video(&crate::content::extension(Path::new(&path)))
                    {
                        crate::content::video_frame(Path::new(&path), hit.seconds, &s)
                    } else if path.to_lowercase().ends_with(".pdf") {
                        crate::pdf::preview(Path::new(&path), hit.page.max(1), &query)
                    } else {
                        crate::content::decode(Path::new(&path), &s).and_then(|i| {
                            crate::content::jpeg(&i, 1024.min(i.width().max(i.height())))
                        })
                    }
                    .map_err(|e| e.to_string());
                    let _ = tx.send(Event::Preview(path, result));
                });
            }
            if let Some((path, texture)) = &self.texture {
                if path == &hit.path {
                    let image = egui::Image::new(texture).max_size(egui::vec2(
                        ui.available_width(),
                        if self.large_preview {
                            (ui.available_height() - 105.0).max(80.0)
                        } else {
                            ui.available_height() * 0.65
                        },
                    ));
                    ui.add(if hit.kind == "application" {
                        image.fit_to_exact_size(egui::vec2(
                            if self.large_preview { 192.0 } else { 72.0 },
                            if self.large_preview { 192.0 } else { 72.0 },
                        ))
                    } else {
                        image
                    });
                }
            } else if let Some(error) = &self.preview_error {
                ui.label(format!("Preview unavailable: {error}"));
            } else {
                ui.label("Loading preview…");
            }
        } else if hit.kind == "calculator" {
            ui.heading(&hit.path);
        }
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ["application", "settings"].contains(&hit.kind.as_str()) {
                ui.label(
                    RichText::new(if hit.kind == "application" {
                        "App"
                    } else {
                        "Settings"
                    })
                    .strong()
                    .color(self.accent()),
                );
            }
            ui.label(RichText::new(&hit.path).weak());
        });
        ui.label(format!("Match score: {:.3} (ranking score)", hit.score));
        ui.label(&hit.reason);
        if hit.page > 0 {
            ui.label(format!("Page {}", hit.page));
        }
        if hit.seconds >= 0.0 {
            if ui.button("Play from matched moment").clicked() {
                match crate::platform::play_at(&hit.path, hit.seconds) {
                    Ok(()) => {
                        self.status = format!(
                            "Playing from {}:{:02}",
                            hit.seconds as u32 / 60,
                            hit.seconds as u32 % 60
                        )
                    }
                    Err(e) => self.run(Err(e)),
                }
            }
            if ui.button("Stop playback").clicked() {
                crate::platform::stop_playback();
                self.status = "Playback stopped".into();
            }
            ui.label(format!(
                "Moment {}:{:02}",
                hit.seconds as u32 / 60,
                hit.seconds as u32 % 60
            ));
        }
        if !crate::content::is_image(&crate::content::extension(Path::new(&hit.path))) {
            egui::ScrollArea::vertical()
                .id_salt("preview_text")
                .show(ui, |ui| {
                    ui.label(&hit.text);
                });
        }
    }
    fn open_selected(&mut self, ctx: &egui::Context) {
        if self.tool == "AI" {
            self.ask_ai();
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("query")));
            return;
        }
        if self.tool != "Search" {
            if let Some(item) = self.tool_items.get(self.selected).cloned() {
                self.copy_tool_item(ctx, item.clone());
            }
            return;
        }
        if let Some(hit) = self.hits.get(self.selected).cloned() {
            if hit.kind == "calculator" {
                ctx.copy_text(hit.path);
                return;
            }
            if self.target != 0 && self.s.dialog_integration {
                match crate::platform::send_to_dialog(self.target, &hit.path) {
                    Ok(true) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        return;
                    }
                    Err(e) => {
                        self.status = e.to_string();
                        return;
                    }
                    _ => {}
                }
            }
            self.run(crate::platform::open(&hit.path));
        }
    }
    fn sources(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your sources");
        ui.label("Quick names for entire drives; Full index for content and media.");
        ui.group(|ui| {
            ui.heading("Index status");
            ui.label(format!(
                "{} files · {} Full · {} Quick · {} vectors",
                self.stats.files, self.stats.full, self.stats.quick, self.stats.vectors
            ));
            ui.label(format!(
                "{} · {} found · {} done · {} unchanged · {} skipped",
                self.progress.phase,
                self.progress.found,
                self.progress.completed,
                self.progress.reused,
                self.progress.skipped
            ));
            if self.indexing {
                if let Some(total) = self.progress.total {
                    ui.add(
                        egui::ProgressBar::new(
                            self.progress.completed as f32 / total.max(1) as f32,
                        )
                        .show_percentage(),
                    );
                } else {
                    ui.spinner();
                }
                ui.label(&self.progress.phase);
                if self.progress.phase.starts_with("EmbeddingGemma") {
                    ui.label(format!(
                        "{} chunks embedded in this file",
                        self.progress.content_chunks
                    ));
                }
                if self.engine.control.automatic.load(Ordering::Relaxed) {
                    ui.label(&self.schedule_status);
                }
                ui.label(&self.progress.current);
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.indexing && !self.setup_busy,
                        egui::Button::new("Update index"),
                    )
                    .clicked()
                {
                    self.start_index(false);
                }
                if ui
                    .add_enabled(
                        self.indexing,
                        egui::Button::new(if self.engine.control.paused.load(Ordering::Relaxed) {
                            "Resume"
                        } else {
                            "Pause"
                        }),
                    )
                    .clicked()
                {
                    self.engine
                        .control
                        .paused
                        .fetch_xor(true, Ordering::Relaxed);
                }
                if ui
                    .add_enabled(self.indexing, egui::Button::new("Cancel indexing"))
                    .clicked()
                {
                    self.engine.control.cancel_by_user();
                    self.status = "Cancelled; automatic work paused until Update index or Resume automatic indexing".into();
                }
                if ui
                    .add_enabled(!self.indexing, egui::Button::new("Rebuild full content"))
                    .clicked()
                {
                    self.start_index(true);
                }
            });
        });
        if ui
            .add_enabled(
                !self.setup_busy,
                egui::Button::new("Clear entire index").stroke(egui::Stroke::new(
                    1.0,
                    crate::appearance::component_color(ui.visuals().dark_mode, false, false),
                )),
            )
            .clicked()
        {
            self.clear_index_confirm = true;
        }
        if self.index_clearing {
            ui.spinner();
            ui.label("Stopping indexing and clearing the active database…");
        }
        ui.checkbox(
            &mut self.s.retry_failed,
            "Retry previously skipped files on the next update",
        );
        ui.checkbox(
            &mut self.s.count_files,
            "Count files before indexing (off starts processing immediately)",
        );
        ui.checkbox(
            &mut self.s.skip_windows_churn,
            "Exclude common Windows/cache folders",
        );
        ui.horizontal(|ui| {
            combo(ui, "add_mode", &mut self.source_mode, &["quick", "full"]);
            if ui.button("Add folder / drive").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.s.roots.push(Source {
                        path: path.to_string_lossy().into_owned(),
                        mode: self.source_mode.clone(),
                    });
                }
            }
            if ui.button("Add files").clicked() {
                if let Some(paths) = rfd::FileDialog::new().pick_files() {
                    for path in paths {
                        self.s.roots.push(Source {
                            path: path.to_string_lossy().into_owned(),
                            mode: self.source_mode.clone(),
                        });
                    }
                }
            }
            if ui.button("Select/Deselect all").clicked() {
                let selected = !self.source_selected.iter().all(|v| *v);
                self.source_selected = vec![selected; self.s.roots.len()];
            }
            if ui.button("Remove selected").clicked() {
                self.source_selected.resize(self.s.roots.len(), false);
                for i in (0..self.s.roots.len()).rev() {
                    if self.source_selected[i] {
                        self.s.roots.remove(i);
                        self.source_selected.remove(i);
                    }
                }
            }
            if ui.button("Apply mode to selected").clicked() {
                for (i, root) in self.s.roots.iter_mut().enumerate() {
                    if self.source_selected.get(i) == Some(&true) {
                        root.mode = self.source_mode.clone();
                    }
                }
            }
        });
        self.source_selected.resize(self.s.roots.len(), false);
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("inclusion-list")
            .max_width(ui.available_width() * 0.88)
            .max_height(220.0)
            .min_scrolled_height(0.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, root) in self.s.roots.clone().iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.source_selected[i], "");
                        ui.allocate_ui_with_layout(
                            egui::vec2((ui.available_width() - 360.0).max(100.0), 22.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| ui.add(egui::Label::new(&root.path).truncate()),
                        )
                        .response
                        .on_hover_text(&root.path);
                        ui.label(RichText::new(&root.mode).color(self.accent()));
                        if let Some(n) = self.source_stats.get(&root.path) {
                            ui.label(format!(
                                "{} files · +{} in past hour",
                                n.files, n.added_hour
                            ));
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
            });
        if let Some(i) = remove {
            self.s.roots.remove(i);
            self.source_selected.remove(i);
        }
        ui.separator();
        ui.heading("Exclude from indexing and searches");
        ui.horizontal(|ui| {
            if ui.button("Exclude folder").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.s.exclusions.push(p.to_string_lossy().into_owned());
                }
            }
            if ui.button("Exclude files").clicked() {
                if let Some(files) = rfd::FileDialog::new().pick_files() {
                    self.s
                        .exclusions
                        .extend(files.iter().map(|p| p.to_string_lossy().into_owned()));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.manual_exclude)
                    .hint_text("Enter a file or folder path"),
            );
            if ui.button("Add exclusion").clicked() && !self.manual_exclude.is_empty() {
                self.s
                    .exclusions
                    .push(std::mem::take(&mut self.manual_exclude));
            }
        });
        self.exclusion_selected
            .resize(self.s.exclusions.len(), false);
        ui.horizontal(|ui| {
            if ui.button("Select/Deselect all").clicked() {
                let selected = !self.exclusion_selected.iter().all(|v| *v);
                self.exclusion_selected.fill(selected);
            }
            if ui.button("Remove selected").clicked() {
                for i in (0..self.s.exclusions.len()).rev() {
                    if self.exclusion_selected[i] {
                        self.s.exclusions.remove(i);
                        self.exclusion_selected.remove(i);
                    }
                }
            }
        });
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("exclusion-list")
            .max_width(ui.available_width() * 0.88)
            .max_height(220.0)
            .min_scrolled_height(0.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (i, p) in self.s.exclusions.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.exclusion_selected[i], "");
                        ui.add_sized(
                            [(ui.available_width() - 100.0).max(100.0), 22.0],
                            egui::Label::new(p).truncate(),
                        )
                        .on_hover_text(p);
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
            });
        if let Some(i) = remove {
            self.exclusion_selected.remove(i);
            self.s.exclusions.remove(i);
        }
        ui.checkbox(
            &mut self.s.excluded_sensitive,
            "Skip content with potential credentials",
        );
        field(
            ui,
            "Exclude file types (e.g. *.tmp, *.log, *.bak)",
            &mut self.s.excluded_types,
        );
        ui.horizontal(|ui| {
            for extension in ["tmp", "log", "bak", "temp"] {
                let token = format!("*.{extension}");
                let mut selected = self
                    .s
                    .excluded_types
                    .split([',', ';', ' '])
                    .any(|p| p == token);
                if ui.checkbox(&mut selected, token.clone()).changed() {
                    let mut values = self
                        .s
                        .excluded_types
                        .split([',', ';', ' '])
                        .filter(|p| !p.is_empty() && *p != token)
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    if selected {
                        values.push(token);
                    }
                    self.s.excluded_types = values.join(", ");
                }
            }
        });
        ui.checkbox(&mut self.s.include_programs, "Search Start menu programs");
        ui.checkbox(
            &mut self.s.code_search,
            "Include source code in Full indexing",
        );
        ui.checkbox(
            &mut self.s.include_programs,
            "Search and launch Windows applications",
        );
        ui.checkbox(&mut self.s.index_images, "Index image meaning");
        ui.checkbox(&mut self.s.index_audio, "Index audio moments");
        ui.checkbox(&mut self.s.index_video, "Index video frames");
        if self.s.index_images {
            combo(
                ui,
                "visual_detail",
                &mut self.s.visual_detail,
                &["Balanced", "Detailed"],
            );
        }
        ui.checkbox(
            &mut self.s.pdf_visuals,
            "Embed PDF page images as well as page text",
        );
        if self.s.index_video {
            ui.add(
                egui::Slider::new(&mut self.s.video_interval, 1.0..=60.0)
                    .text("Video frame sampling interval (seconds)"),
            );
        }
        ui.add(
            egui::Slider::new(&mut self.s.max_file_mib, 1..=256)
                .text("Maximum document extraction size (MiB)"),
        );
        ui.label("Full-index extraction changes apply to changed files; use Rebuild Full to refresh existing content.");
        ui.separator();
        if ui
            .add_enabled(
                !self.indexing && !self.setup_busy,
                egui::Button::new("Import a copy of the existing VectorDash index"),
            )
            .clicked()
        {
            let source =
                PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("VectorDash");
            let d = self.engine.db.clone();
            let tx = self.engine.tx.clone();
            self.setup_busy = true;
            std::thread::spawn(move || {
                let result = (|| -> Result<Settings> {
                    let s = Settings::legacy(&source)?;
                    d.import(&source, s.encrypt_index, |done, total| {
                        let _ = tx.send(Event::Log(
                            "Info".into(),
                            format!("Import {done}/{total} database pages"),
                        ));
                    })?;
                    Ok(s)
                })()
                .map_err(|e| e.to_string());
                let _ = tx.send(Event::Imported(result));
            });
        }
    }
    fn refresh_devices(&mut self) {
        if self.devices_loading || self.s.server.is_empty() {
            return;
        }
        self.devices_loading = true;
        self.devices_server = self.s.server.clone();
        self.devices.clear();
        let server = self.s.server.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let mut command = std::process::Command::new(server);
            command.arg("--list-devices");
            crate::content::hidden(&mut command);
            let text = command
                .output()
                .map(|o| {
                    format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    )
                })
                .unwrap_or_else(|e| e.to_string());
            let devices = crate::platform::parse_devices(&text);
            let _ = tx.send(Event::Devices(devices));
            let _ = tx.send(Event::Log("Info".into(), text));
        });
    }

    fn paint_waveform(&self, ui: &mut egui::Ui, input: egui::Rect) {
        let rect = input.shrink2(egui::vec2(12.0, 6.0));
        let mut wave = crate::dictation::waveform();
        if self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-recording") {
            let time = ui.input(|i| i.time) as f32;
            for (i, value) in wave.iter_mut().enumerate() {
                *value = ((i as f32 * 0.45 + time * 3.0).sin() * 0.5 + 0.5) * 0.5;
            }
        }
        let center = rect.center().y;
        ui.painter().line_segment(
            [
                egui::pos2(rect.left(), center),
                egui::pos2(rect.right(), center),
            ],
            egui::Stroke::new(1.0, self.accent().gamma_multiply(0.3)),
        );
        for (i, v) in wave.iter().enumerate() {
            let x = rect.left() + i as f32 / (wave.len() - 1) as f32 * rect.width();
            let h = (v.sqrt() * 22.0).clamp(0.5, 12.0);
            ui.painter().line_segment(
                [egui::pos2(x, center - h), egui::pos2(x, center + h)],
                egui::Stroke::new(1.5, self.accent()),
            );
        }
    }
    fn footer_text(&self) -> String {
        if self.quick {
            if self.ai_busy {
                "AI request in progress…".into()
            } else if let Some(error) = &self.search_error {
                error.clone()
            } else if self.status == "Copied"
                || self.status.starts_with("Playing from")
                || self.status.starts_with("Playback")
            {
                self.status.clone()
            } else if self.searching {
                "Searching…".into()
            } else if self.tool == "Search" && !self.query.trim().is_empty() {
                format!("{} results · {:.1} ms", self.hits.len(), self.search_ms)
            } else if self.tool == "Clipboard" {
                format!("{} items", self.tool_items.len())
            } else if self.tool == "Emoji" {
                format!("{} emoji", self.tool_items.len())
            } else {
                String::new()
            }
        } else if self.ai_busy {
            "AI request in progress…".into()
        } else {
            self.status.clone()
        }
    }
    fn refresh_microphones(&mut self) {
        self.microphones_busy = true;
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Microphones(
                crate::dictation::microphones().map_err(|e| e.to_string()),
            ));
        });
    }
    fn uninstall_downloads(&mut self) {
        self.uninstall_confirm = false;
        self.setup_busy = true;
        self.dictation_cancel.store(true, Ordering::Release);
        self.engine.control.cancel_by_user();
        let home = self.home.clone();
        let models = self.engine.models.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                models.stop()?;
                let end = Instant::now() + Duration::from_secs(10);
                while crate::dictation::in_use() && Instant::now() < end {
                    std::thread::sleep(Duration::from_millis(50));
                }
                anyhow::ensure!(
                    !crate::dictation::in_use(),
                    "Dictation is still stopping; retry uninstall shortly"
                );
                crate::dictation::stop();
                crate::resources::uninstall_all(&home)
            })()
            .map_err(|e| e.to_string());
            let _ = tx.send(Event::ComponentRemoved("all".into(), result));
        });
    }
    fn install_component(&mut self, key: &str) {
        if self.setup_busy {
            return;
        }
        self.setup_busy = true;
        self.setup_progress = None;
        self.setup.cancel.store(false, Ordering::Release);
        if key == "dictation" {
            let setup = self.setup.clone();
            let settings = self.s.clone();
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Event::DictationInstalled(
                    crate::dictation::install(&setup, &settings).map_err(|e| e.to_string()),
                ));
            });
        } else {
            self.setup.install(
                self.s.clone(),
                key == "runtime",
                key == "embedding",
                key == "encoder",
                key == "ocr",
            );
        }
    }
    fn components_ui(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui|{
            ui.heading("Status");
            let loaded=self.engine.models.loaded.load(Ordering::Acquire);
            for(key,name)in [("runtime","Local llama-server"),("embedding","EmbeddingGemma 2"),("encoder","Photos / audio / video encoder"),("ocr","Tesseract OCR"),("dictation","Parakeet dictation"),("ffmpeg","FFmpeg"),("pdf","PDF renderer")]{
                let usage=self.component_stats.get(key).cloned().unwrap_or_default();
                let installed=if key=="dictation"{crate::dictation::ready(&self.home,&self.s)}else if key=="ffmpeg"||key=="pdf"{usage.disk>0}else{Path::new(match key{"runtime"=>&self.s.server,"embedding"=>&self.s.model,"encoder"=>&self.s.projector,_=>&self.s.ocr}).is_file()};
                let active=if key=="dictation"{crate::dictation::pid()!=0}else if key=="encoder"{self.engine.models.media_loaded()}else if key=="runtime"||key=="embedding"{loaded}else{usage.ram>0};
                let state=if active{"Loaded"}else if installed{"Installed · on demand"}else{"Not installed"};
                let color=crate::appearance::component_color(ui.visuals().dark_mode, active, installed);
                ui.horizontal_wrapped(|ui|{
                    ui.label(name);
                    ui.label(RichText::new(state).color(color));
                    if self.component_stats_busy&&self.component_stats.is_empty(){ui.weak("Measuring…");}
                    else if key=="pdf"{ui.weak(format!("RAM shared with app · Disk {}",crate::resources::format_bytes(usage.disk)));}
                    else{ui.weak(format!("{}RAM {} · VRAM {} · Disk {}",if usage.shared{"Shared " }else{""},crate::resources::format_bytes(usage.ram),usage.vram.map(crate::resources::format_bytes).unwrap_or_else(||"Unavailable".into()),crate::resources::format_bytes(usage.disk)));}
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                        let managed=crate::resources::managed(&self.home,&self.s,key);
                        if installed{if ui.add_enabled(!self.setup_busy&&managed,egui::Button::new("Uninstall")).on_hover_text(if managed{"Remove this downloaded item"}else{"External or bundled: managed by its installer"}).clicked(){self.remove_component=Some(key.into());}}
                        else if ui.add_enabled(!self.setup_busy&&!["ffmpeg","pdf"].contains(&key),egui::Button::new("Install")).clicked(){self.install_component(key);}
                    });
                });
            }
            ui.weak("EmbeddingGemma, encoder and llama-server share one inference process; shared RAM/VRAM totals are not separate allocations. VRAM is dedicated GPU memory sampled by Windows.");
            ui.horizontal(|ui|{
                let loaded=self.engine.models.loaded.load(Ordering::Acquire);
                if ui.add_enabled(!self.setup_busy&&!loaded,egui::Button::new("Start")).clicked(){
                    self.save();let settings=self.s.clone();let models=self.engine.models.clone();let media=Path::new(&self.s.projector).is_file();let tx=self.engine.tx.clone();self.setup_busy=true;
                    std::thread::spawn(move||{let _=tx.send(Event::Setup(models.start(&settings,media).map(|_|vec![]).map_err(|e|e.to_string())));});
                }
                if ui.add_enabled(loaded||crate::dictation::pid()!=0,egui::Button::new("Stop / unload models")).clicked(){let models=self.engine.models.clone();std::thread::spawn(move||{let _=models.stop();crate::dictation::stop();});}
                if self.setup_busy&&ui.button("Cancel setup").clicked(){self.setup.cancel.store(true,Ordering::Release);}
            });
            if self.setup_busy{if let Some((name,done,total))=&self.setup_progress{ui.label(name);if *total>0{ui.add(egui::ProgressBar::new(*done as f32/ *total as f32).show_percentage());}}}
        });
        ui.add_space(12.0);
    }
    fn models(&mut self, ui: &mut egui::Ui) {
        if !self.component_stats_busy && self.component_stats_at.elapsed() > Duration::from_secs(3)
        {
            self.component_stats_busy = true;
            self.component_stats_at = Instant::now();
            let home = self.home.clone();
            let settings = self.s.clone();
            let pid = self.engine.models.pid();
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Event::ComponentStats(crate::resources::sample(
                    &home, &settings, pid,
                )));
            });
        }
        self.components_ui(ui);

        ui.checkbox(&mut self.s.unload_idle, "Unload local models when idle");
        if self.s.unload_idle {
            ui.add(
                egui::Slider::new(&mut self.s.unload_minutes, 1..=120)
                    .text("Idle unload (minutes)"),
            );
        }

        ui.label("An unloaded model may take a few seconds to load for the next Hybrid or Semantic search. File/Folder Names search needs no model.");
        ui.checkbox(
            &mut self.s.pause_for_game,
            "Unload local models and pause indexing when a game is detected",
        );
        ui.checkbox(
            &mut self.s.pause_for_fullscreen,
            "Unload local models and pause indexing for full-screen applications",
        );
        ui.horizontal(|ui| {
            ui.label("Additional game executable names");
            ui.text_edit_singleline(&mut self.s.game_processes).on_hover_text("Comma-separated names, such as forza.exe, cs2.exe. Windows detection also recognizes full-screen Direct3D and common game install paths.");
        });
        ui.checkbox(
            &mut self.s.defer_search_model,
            "Delay search model loading while CPU/GPU or power limits are not met",
        );
        ui.label("Uses Scheduling resource limits; keyboard/mouse activity is ignored for searches. Load anyway applies only while the current search window remains open.");
        ui.checkbox(
            &mut self.s.show_model_notices,
            "Show unloaded-model notices in search",
        );

        ui.label("OCR and media helpers run only while needed. Remote servers are managed by their host.");
        ui.heading("Inference hardware");
        combo(
            ui,
            "hardware",
            &mut self.s.hardware,
            &["CPU / system RAM", "Dedicated GPU", "Integrated GPU"],
        );
        if self.s.hardware != "CPU / system RAM" {
            combo(
                ui,
                "backend",
                &mut self.s.runtime_backend,
                &["Auto", "CUDA", "Vulkan"],
            );
            ui.label(format!("Setup backend: {}. CUDA is recommended for NVIDIA; Vulkan supports Intel/AMD and compatible NVIDIA builds.",if self.s.backend()=="cuda-"{"CUDA"}else{"Vulkan"}));
            if self.devices_server != self.s.server && !self.devices_loading {
                self.refresh_devices();
            }
            let text = self
                .devices
                .iter()
                .find(|(id, _)| *id == self.s.device)
                .map(|(id, name)| format!("{id} — {name}"))
                .unwrap_or_else(|| {
                    if self.s.device.is_empty() {
                        if self.devices_loading {
                            "Detecting devices…".into()
                        } else {
                            "Select GPU / iGPU".into()
                        }
                    } else {
                        self.s.device.clone()
                    }
                });
            egui::ComboBox::from_id_salt("device_choice")
                .selected_text(text)
                .width(ui.available_width().min(620.0))
                .show_ui(ui, |ui| {
                    for (id, name) in &self.devices {
                        ui.selectable_value(
                            &mut self.s.device,
                            id.clone(),
                            format!("{id} — {name}"),
                        );
                    }
                });
            if ui
                .add_enabled(!self.devices_loading, egui::Button::new("Refresh devices"))
                .clicked()
            {
                self.refresh_devices();
            }
            if self.s.device.to_ascii_uppercase().starts_with("CUDA") {
                ui.label("Selected device backend: CUDA");
            } else if !self.s.device.is_empty() {
                ui.label("Selected device backend: Vulkan");
            }
        }
        ui.collapsing("Advanced device identifier", |ui| {
            field(ui, "Device identifier", &mut self.s.device);
        });
        combo(ui, "quant", &mut self.s.quant, &["Q8_0", "BF16"]);
        path_field(ui, "llama-server", &mut self.s.server);
        path_field(ui, "EmbeddingGemma 2 GGUF", &mut self.s.model);
        path_field(ui, "Matching mmproj", &mut self.s.projector);
        ui.add_space(12.0);
        ui.heading("Remote model server");
        field(
            ui,
            "Remote llama-server URL (blank uses local)",
            &mut self.s.remote_url,
        );
        secret(ui, "Remote bearer key", &mut self.s.remote_key);
        ui.collapsing("Optional download tokens", |ui| {
            secret(ui, "GitHub download token", &mut self.s.github_token);
            secret(ui, "Hugging Face token", &mut self.s.hf_token);
        });
        ui.add_space(12.0);
        ui.heading("OCR and media tools");
        ui.checkbox(
            &mut self.s.ocr_enabled,
            "Enable OCR text extraction for images and scanned PDF pages",
        );
        path_field(ui, "Tesseract OCR", &mut self.s.ocr);
        if self.s.ocr_enabled {
            field(ui, "OCR languages", &mut self.s.ocr_languages);
        }
        path_field(ui, "FFmpeg (bundled by default)", &mut self.s.ffmpeg);
        ui.add_space(12.0);
        ui.heading("Dictation");
        ui.label("Parakeet TDT 0.6B v2 · English speech-to-text");
        combo(
            ui,
            "dictation-device",
            &mut self.s.dictation_device,
            &["CPU", "NVIDIA CUDA"],
        );
        egui::ComboBox::from_id_salt("dictation-quant").selected_text(match crate::dictation::quant(&self.s){"fp16"=>"fp16",_=>"8-bit"}).show_ui(ui,|ui|{
            ui.selectable_value(&mut self.s.dictation_quant,"fp16".into(),"fp16");
            ui.selectable_value(&mut self.s.dictation_quant,"8-bit".into(),"8-bit");
        }).response.on_hover_text("Quantization is independent of device. CUDA can fall back to CPU for unsupported 8-bit operators.");

        ui.checkbox(
            &mut self.s.dictation_keep_loaded,
            "Keep Parakeet loaded between transcriptions",
        );
        if self.s.dictation_keep_loaded {
            ui.add(
                egui::Slider::new(&mut self.s.dictation_unload_minutes, 1..=120)
                    .text("Unload after idle (minutes)"),
            );
        } else {
            ui.weak("Unloads immediately after each transcription.");
        }
        ui.horizontal(|ui| {
            ui.label("Text batch size");
            ui.add(egui::Slider::new(&mut self.s.batch_size, 1..=16));
        });
        if crate::resources::has_downloads(&self.home, &self.s)
            && ui
                .add(
                    egui::Button::new("Uninstall all downloaded models and runtimes…").stroke(
                        egui::Stroke::new(
                            1.0,
                            crate::appearance::component_color(
                                ui.visuals().dark_mode,
                                false,
                                false,
                            ),
                        ),
                    ),
                )
                .clicked()
        {
            self.uninstall_confirm = true;
        }
    }
    fn settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Settings");
        ui.collapsing("Saved searches", |ui| {
            let mut remove = None;
            for (i, search) in self.s.saved_searches.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut search.name));
                    if ui.button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.s.saved_searches.remove(i);
            }
        });

        ui.heading("Search and dictation");
        if !self.microphones_loaded && !self.microphones_busy {
            self.refresh_microphones();
        }
        ui.horizontal(|ui| {
            ui.label("Microphone");
            egui::ComboBox::from_id_salt("microphone")
                .selected_text(if self.s.microphone.is_empty() {
                    "Windows default"
                } else {
                    &self.s.microphone
                })
                .width(320.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.s.microphone, String::new(), "Windows default");
                    for name in &self.microphones {
                        ui.selectable_value(&mut self.s.microphone, name.clone(), name);
                    }
                });
            if ui
                .add_enabled(!self.microphones_busy, egui::Button::new("Refresh"))
                .clicked()
            {
                self.refresh_microphones();
            }
        });
        ui.checkbox(
            &mut self.s.dictation_auto_finish,
            "Transcribe automatically after I stop speaking",
        );
        if self.s.dictation_auto_finish {
            ui.add(
                egui::Slider::new(&mut self.s.dictation_silence_seconds, 0.6..=5.0)
                    .text("Silence delay (seconds)"),
            );
        }
        ui.horizontal(|ui| {
            ui.label("Cycle Search / Clipboard / AI");
            if ui
                .button(if self.llm_hotkey_capture {
                    "Press shortcut…"
                } else {
                    &self.s.llm_hotkey
                })
                .clicked()
            {
                self.llm_hotkey_capture = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Dictate (current search or global quick search)");
            if ui
                .button(if self.global_voice_capture {
                    "Press shortcut…"
                } else {
                    &self.s.voice_search_hotkey
                })
                .clicked()
            {
                self.global_voice_capture = true;
            }
        });
        if self.llm_hotkey_capture || self.voice_hotkey_capture || self.global_voice_capture {
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if key == egui::Key::Escape {
                        self.llm_hotkey_capture = false;
                        self.voice_hotkey_capture = false;
                        self.global_voice_capture = false;
                    } else if key == egui::Key::Tab
                        && !self.global_voice_capture
                        && !self.voice_hotkey_capture
                        || modifiers.ctrl
                        || modifiers.alt
                    {
                        let value = crate::ai::shortcut(key, modifiers);
                        if self.llm_hotkey_capture {
                            self.s.llm_hotkey = value.clone();
                        }
                        if self.voice_hotkey_capture {
                            self.s.dictation_hotkey = value.clone();
                            self.s.voice_search_hotkey = value.clone();
                        }
                        if self.global_voice_capture {
                            self.s.voice_search_hotkey = value.clone();
                            self.s.dictation_hotkey = value;
                        }
                        self.llm_hotkey_capture = false;
                        self.voice_hotkey_capture = false;
                        self.global_voice_capture = false;
                    }
                }
            }
        }
        ui.weak("Tab cycles enabled search modes. Emoji stays available by its button.");
        ui.heading("Appearance");
        combo(ui, "theme", &mut self.s.theme, &["System", "Dark", "Light"]);
        ui.horizontal(|ui| {
            ui.label("App theme color");
            ui.color_edit_button_srgb(&mut self.s.accent);
        });
        ui.horizontal(|ui| {
            ui.label("Popup color tone");
            ui.color_edit_button_srgb(&mut self.s.popup_tone);
        });
        let mut transparency = (1.0 - self.s.popup_opacity) * 100.0;
        if ui
            .add(egui::Slider::new(&mut transparency, 0.0..=90.0).text("Popup transparency (%)"))
            .changed()
        {
            self.s.popup_opacity = 1.0 - transparency / 100.0;
        }
        ui.add(
            egui::Slider::new(&mut self.s.popup_gradient, 0.0..=1.0)
                .text("Popup gradient strength"),
        );
        ui.label("Higher transparency reveals more of the blurred desktop. Appearance changes preview immediately; click Save to keep them.");
        if ui.button("Reset appearance").clicked() {
            self.s.accent = [133, 115, 255];
            self.s.popup_tone = [138, 115, 168];
            self.s.popup_opacity = 0.75;
            self.s.popup_gradient = 0.2;
        }
        ui.add_space(12.0);
        ui.heading("Startup and window behavior");
        ui.checkbox(
            &mut self.s.explorer_integration,
            "Double Ctrl in File Explorer opens quick search",
        );
        if self.s.explorer_integration {
            ui.checkbox(
                &mut self.s.explorer_scope,
                "Explorer double Ctrl: search current folder/drive and subfolders",
            );
        }
        ui.checkbox(
            &mut self.s.dialog_integration,
            "Double Ctrl in Open/Save dialogs; send selected paths to the dialog",
        );
        ui.label("Standard Windows dialogs are supported. Custom and elevated dialogs may require Copy path.");
        combo(
            ui,
            "close_behavior",
            &mut self.s.close_behavior,
            &["Hide to tray", "Minimize to taskbar", "Exit"],
        );
        combo(
            ui,
            "minimize_behavior",
            &mut self.s.minimize_behavior,
            &["Taskbar", "Tray"],
        );
        ui.label("Closing to tray keeps the global shortcut and scheduled indexing running. Exit stops the application and owned model server.");
        ui.checkbox(&mut self.s.start_with_windows, "Start with Windows");
        ui.checkbox(&mut self.s.start_hidden, "Start hidden at startup");
        ui.horizontal(|ui| {
            ui.label("Global shortcut");
            if ui
                .button(if self.hotkey_capture {
                    "Press shortcut…"
                } else {
                    &self.s.hotkey
                })
                .clicked()
            {
                self.hotkey_capture = true;
            }
        });
        if self.hotkey_capture {
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if key == egui::Key::Escape {
                        self.hotkey_capture = false;
                    } else {
                        self.s.hotkey = format!(
                            "{}{}{}{:?}",
                            if modifiers.ctrl { "Ctrl+" } else { "" },
                            if modifiers.alt { "Alt+" } else { "" },
                            if modifiers.shift { "Shift+" } else { "" },
                            key
                        );
                        self.hotkey_capture = false;
                    }
                }
            }
        }
        combo(
            ui,
            "performance",
            &mut self.s.performance,
            &["Low memory", "Balanced", "Fast search"],
        );
        ui.label("Database and filename catalog remain on disk. Low memory: 8 MiB pages and streamed vectors. Balanced: 32 MiB pages and up to 128 MiB compact vectors. Fast search: 64 MiB pages and up to 512 MiB vectors. These are caps, not reserved memory; models/workers use additional RAM.");
        ui.separator();
        ui.heading("Clipboard history");
        ui.checkbox(
            &mut self.s.clipboard_enabled,
            "Save copied text, links and images",
        );
        ui.checkbox(
            &mut self.s.clipboard_image_embeddings,
            "Use EmbeddingGemma 2 to search copied images by meaning",
        );
        if self.s.clipboard_image_embeddings {
            ui.weak("Loads the image encoder when idle conditions allow. Copied images and vectors expire with clipboard history; remote model servers receive the image if configured.");
        }
        egui::ComboBox::from_id_salt("clipboard-retention")
            .selected_text(match self.s.clipboard_hours {
                1 => "1 hour",
                24 => "1 day",
                168 => "1 week",
                720 => "1 month",
                2160 => "3 months",
                8760 => "1 year",
                _ => "Custom",
            })
            .show_ui(ui, |ui| {
                for (hours, label) in [
                    (1, "1 hour"),
                    (24, "1 day"),
                    (168, "1 week"),
                    (720, "1 month"),
                    (2160, "3 months"),
                    (8760, "1 year"),
                ] {
                    ui.selectable_value(&mut self.s.clipboard_hours, hours, label);
                }
            });
        ui.add(
            egui::DragValue::new(&mut self.s.clipboard_hours)
                .range(1..=8760)
                .prefix("Keep for: ")
                .suffix(" hours"),
        );
        ui.label("Excluded applications (executable names, comma separated)");
        ui.text_edit_singleline(&mut self.s.clipboard_excluded_apps);
        ui.weak("Off by default. Up to 500 items / 128 MiB on disk, protected by your Windows account. Text: 16 KiB; images: 4 MiB / 16 megapixels. Expired entries are removed automatically. Privacy-marked items and excluded apps are skipped. File-copy formats are not captured.");
        if ui.button("Clear clipboard history").clicked() {
            self.clipboard_clear_confirm = true;
        }
        ui.separator();
        ui.checkbox(&mut self.s.encrypt_index, "Encrypt database");
        ui.label("Encryption protects filenames, text and vectors using your Windows account; it adds disk/CPU overhead. Portable encrypted data is account-bound. Turning encryption off makes database contents readable.");
        if self.engine.db.key.is_some() != self.s.encrypt_index {
            ui.label("Encryption selection is pending. Create a converted copy to apply it. The original is retained for recovery; an old plaintext copy remains readable.");
            if ui
                .add_enabled(
                    !self.indexing && !self.setup_busy,
                    egui::Button::new("Create and activate converted copy"),
                )
                .clicked()
            {
                self.setup_busy = true;
                let db = self.engine.db.clone();
                let encrypt = self.s.encrypt_index;
                let tx = self.engine.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Event::Converted(
                        db.converted_copy(encrypt).map_err(|e| e.to_string()),
                    ));
                });
            }
        }
    }

    fn switch_tool(&mut self, tool: &str) {
        if !self.saved_settings.tool_available(tool) {
            return;
        }
        self.filter_open = false;
        self.save_search_open = false;
        self.clipboard_texture = None;
        self.clipboard_preview_id = -1;
        self.tool = tool.into();
        self.large_preview = false;
        if tool == "Search" {
            self.model_warm_attempted = false;
        }
        self.selected = 0;
        self.tool_items.clear();
        self.capture_open = false;
        self.capture_bytes = None;
        self.capture_texture = None;
        self.sequence += 1;
        self.search_pending = false;
        self.changed();
    }
    fn stop_ai(&mut self) {
        self.ai_cancel.store(true, Ordering::Release);
        if self.ai_busy && self.ai_messages.last().is_some_and(|m| m.role == "user") {
            self.ai_messages.pop();
        }
        self.ai_busy = false;
        self.ai_job += 1;
    }
    fn ask_ai(&mut self) {
        if !self.saved_settings.ai_chat_available() || self.ai_busy || self.dictation_busy {
            return;
        }
        self.ai_error = None;
        self.ai_answer.clear();
        self.ai_metrics = crate::ai::Metrics::default();
        self.ai_follow_bottom = true;
        self.stop_read_aloud();
        self.markdown = egui_commonmark::CommonMarkCache::default();
        self.ai_question = self.query.trim().into();
        let mut selected_settings = self.saved_settings.clone();
        selected_settings.llm_response = self.response_length.clone();
        match crate::ai::prepare(&selected_settings, &self.ai_messages, &self.ai_question) {
            Ok(mut messages) => {
                if let Err(e) =
                    crate::ai::attach(&selected_settings, &mut messages, self.ai_images.clone())
                {
                    self.ai_error = Some(e.to_string());
                    return;
                }
                self.ai_images.clear();
                self.ai_attachment = None;
                self.ai_context_trimmed = messages.len() < self.ai_messages.len() + 1;
                self.ai_messages = messages;
            }
            Err(error) => {
                self.ai_error = Some(error.to_string());
                return;
            }
        }
        self.ai_job += 1;
        let id = self.ai_job;
        self.ai_cancel = Arc::new(AtomicBool::new(false));
        self.ai_busy = true;
        let cancel = self.ai_cancel.clone();
        let s = selected_settings;
        let messages = self.ai_messages.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let result = crate::ai::ask_details(
                &s,
                &messages,
                &cancel,
                |text| {
                    let _ = tx.send(Event::AiChunk(id, text));
                },
                |metrics| {
                    let _ = tx.send(Event::AiMetrics(id, metrics));
                },
            )
            .map_err(|e| e.to_string());
            let _ = tx.send(Event::AiDone(id, result));
        });
    }

    fn copy_tool_item(&mut self, ctx: &egui::Context, item: crate::launcher::Item) {
        if item.image {
            let home = self.home.clone();
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let result = crate::launcher::Clipboard::new(&home)
                    .copy_image(item.id)
                    .map_err(|e| e.to_string());
                let _ = tx.send(Event::ClipboardCleared(result));
            });
        } else {
            ctx.copy_text(item.value);
            self.status = "Copied".into();
        }
    }
    fn stop_read_aloud(&mut self) {
        self.tts_cancel.store(true, Ordering::Release);
        if self.tts_busy {
            crate::platform::stop_playback();
        }
        self.tts_job += 1;
        self.tts_busy = false;
    }
    fn start_read_aloud(&mut self, text: String) {
        self.stop_read_aloud();
        self.tts_job += 1;
        let id = self.tts_job;
        self.tts_busy = true;
        self.tts_status = String::new();
        self.tts_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.tts_cancel.clone();
        let s = self.saved_settings.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let result = crate::tts::speak(&s, &text, &cancel).map_err(|e| e.to_string());
            let _ = tx.send(Event::TtsDone(id, result));
        });
    }
    fn new_conversation(&mut self) {
        self.stop_read_aloud();
        self.ai_messages.clear();
        self.ai_images.clear();
        self.ai_attachment = None;
        self.ai_answer.clear();
        self.ai_question.clear();
        self.ai_error = None;
        self.ai_context_trimmed = false;
        self.ai_metrics = crate::ai::Metrics::default();
        self.markdown = egui_commonmark::CommonMarkCache::default();
    }
    fn toggle_dictation(&mut self) {
        if self.dictation_busy {
            self.dictation_stop.store(true, Ordering::Release);
            return;
        }
        self.dictation_job += 1;
        let id = self.dictation_job;
        self.dictation_stop = Arc::new(AtomicBool::new(false));
        self.dictation_cancel = Arc::new(AtomicBool::new(false));
        self.dictation_busy = true;
        self.dictation_submit_ai = self.tool == "AI";
        if self.smoke.is_some() {
            let args = std::env::args().collect::<Vec<_>>();
            if let Some(text) = args
                .iter()
                .position(|a| a == "--smoke-dictation-text")
                .and_then(|i| args.get(i + 1))
            {
                let _ = self
                    .engine
                    .tx
                    .send(Event::DictationDone(id, Ok(text.clone())));
                return;
            }
        }
        self.dictation_status = "Opening microphone…".into();
        let stop = self.dictation_stop.clone();
        let cancel = self.dictation_cancel.clone();
        let home = self.home.clone();
        let settings = self.saved_settings.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let result = crate::dictation::run(&home, &settings, stop, cancel, |text| {
                let _ = tx.send(Event::DictationState(id, text.into()));
            })
            .map_err(|e| e.to_string());
            let _ = tx.send(Event::DictationDone(id, result));
        });
    }

    fn capture_ui(&mut self, ctx: &egui::Context) {
        if !self.capture_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Screenshot for AI").open(&mut open).default_width(860.0).resizable(true).show(ctx,|ui|{
            ui.label("Choose a window or monitor and review its preview. Attach a screenshot; Send submits it to the selected vision model.");
            if ui.add_enabled(!self.capture_busy,egui::Button::new("Select area anywhere on screen")).clicked(){
                self.capture_busy=true;self.capture_target=None;self.capture_preview_key=None;self.capture_texture=None;self.capture_bytes=None;
                let tx=self.engine.tx.clone();
                crate::platform::hide_main_window();
                std::thread::spawn(move||{
                    let result=crate::capture::select_area().map_err(|e|e.to_string());
                    crate::platform::wake_main_window();
                    let _=tx.send(Event::Screenshot(result));
                });
            }
            ui.columns(2,|columns|{
                egui::ScrollArea::vertical().id_salt("capture-choices").max_height(330.0).show(&mut columns[0],|ui|{
                    for target in &self.capture_targets{
                        if ui.add_enabled(!self.capture_busy, egui::Button::new(&target.label).selected(self.capture_target.as_ref().is_some_and(|t|t.id==target.id&&t.window==target.window))).clicked(){self.capture_target=Some(target.clone());self.capture_bytes=None;self.capture_texture=None;}
                    }
                });
                let ui=&mut columns[1];
                if let Some(texture)=&self.capture_texture{ui.add(egui::Image::new(texture).max_size(egui::vec2(ui.available_width(),330.0)));}
                else if self.capture_busy{ui.spinner();}else{ui.centered_and_justified(|ui|{ui.label("Select a source to preview.");});}
            });
            if let Some(target)=self.capture_target.clone(){
                let key=(target.id,target.window);
                if self.capture_preview_key!=Some(key)&&!self.capture_busy{
                    self.capture_preview_key=Some(key);self.capture_busy=true;self.capture_texture=None;self.capture_bytes=None;
                    let tx=self.engine.tx.clone();std::thread::spawn(move||{let _=tx.send(Event::Screenshot(crate::capture::grab(&target).map_err(|e|e.to_string())));});
                }
            }
            if ui.add_enabled(!self.capture_busy&&self.capture_bytes.is_some(),egui::Button::new("Attach screenshot")).clicked(){
                if let Some(bytes)=&self.capture_bytes{match crate::capture::vision(bytes,None){Ok(url)=>{self.ai_images=vec![url];self.ai_attachment=self.capture_texture.clone();self.capture_open=false;},Err(e)=>self.ai_error=Some(e.to_string())}}
            }
        });
        self.capture_open &= open;
        if !self.capture_open {
            self.capture_bytes = None;
            self.capture_texture = None;
            self.capture_preview_key = None;
        }
    }
    fn tools_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.tool == "AI" {
            let scroll = self.ai_scroll;
            self.ai_scroll = 0.0;
            let output = egui::ScrollArea::vertical()
                .id_salt("ai-answer")
                .stick_to_bottom(self.ai_busy)
                .max_height(ui.available_height().max(80.0))
                .show(ui, |ui| {
                    for message in &self.ai_messages {
                        ui.label(
                            RichText::new(if message.role == "user" { "You" } else { "AI" })
                                .size(20.0)
                                .family(egui::FontFamily::Name("bold".into()))
                                .strong(),
                        );
                        for image in &message.images {
                            if let Ok(bytes) = image_bytes(image) {
                                let uri = format!("bytes://ai-screenshot-{}.jpg", image_key(image));
                                let response = ui.add(
                                    egui::Image::from_bytes(uri, bytes)
                                        .max_size(egui::vec2(200.0, 130.0))
                                        .sense(egui::Sense::click()),
                                );
                                if response.clicked() {
                                    self.ai_image_view = Some(image.clone());
                                }
                                response.context_menu(|ui| {
                                    if ui.button("Copy screenshot").clicked() {
                                        let _ = copy_ai_image(image);
                                        ui.close();
                                    }
                                });
                            }
                        }
                        egui_commonmark::CommonMarkViewer::new().show(
                            ui,
                            &mut self.markdown,
                            &self.color_emoji.markdown(ctx, &message.content),
                        );
                        ui.add_space(10.0);
                    }
                    if self.ai_busy || self.ai_error.is_some() {
                        ui.label(
                            RichText::new("AI")
                                .size(20.0)
                                .family(egui::FontFamily::Name("bold".into()))
                                .strong(),
                        );
                        egui_commonmark::CommonMarkViewer::new().show(
                            ui,
                            &mut self.markdown,
                            &self.color_emoji.markdown(ctx, &self.ai_answer),
                        );
                    }
                    if !self.ai_busy && !self.ai_answer.is_empty() {
                        ui.horizontal(|ui| {
                            if ui
                                .button("Copy")
                                .on_hover_text("Copy last response")
                                .clicked()
                            {
                                ctx.copy_text(self.ai_answer.clone());
                            }
                            if ui
                                .add(egui::Button::image(crate::icons::image(
                                    crate::icons::Icon::Speaker,
                                    self.accent(),
                                    20.0,
                                )))
                                .on_hover_text(if self.tts_busy {
                                    "Stop read aloud"
                                } else {
                                    "Read last response aloud"
                                })
                                .clicked()
                            {
                                if self.tts_busy {
                                    self.stop_read_aloud();
                                } else {
                                    self.start_read_aloud(self.ai_answer.clone());
                                }
                            }
                            ui.weak(self.ai_metrics.label());
                        });
                    }
                    if self.ai_follow_bottom {
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                        self.ai_follow_bottom = false;
                    }
                    if scroll != 0.0 {
                        ui.scroll_with_delta(egui::vec2(0.0, scroll));
                    }
                });
            self.ai_scroll_offset = output.state.offset.y;
            return;
        }
        if self.tool == "Emoji" {
            let columns = (ui.available_width() / 62.0).floor().max(1.0) as usize;
            let rows = self.tool_items.len().div_ceil(columns);
            egui::ScrollArea::vertical()
                .id_salt("emoji-grid")
                .max_height(ui.available_height())
                .show_rows(ui, 58.0, rows, |ui, range| {
                    for row in range {
                        ui.horizontal(|ui| {
                            for col in 0..columns {
                                let i = row * columns + col;
                                if let Some(item) = self.tool_items.get(i).cloned() {
                                    let response = ui
                                        .add_sized(
                                            [54.0, 54.0],
                                            if let Some(uri) =
                                                self.color_emoji.uri(ctx, &item.value)
                                            {
                                                egui::Button::image(
                                                    egui::Image::from_uri(uri)
                                                        .fit_to_exact_size(egui::vec2(30.0, 30.0)),
                                                )
                                            } else {
                                                egui::Button::new(
                                                    RichText::new(&item.value).size(30.0),
                                                )
                                            }
                                            .selected(self.selected == i),
                                        )
                                        .on_hover_text(&item.label);
                                    if response.clicked() {
                                        self.selected = i;
                                        self.copy_tool_item(ctx, item);
                                    }
                                }
                            }
                        });
                    }
                });
            return;
        }
        if self.tool == "Clipboard" {
            ui.weak(format!(
                "{} items · Enter / Ctrl+C to copy",
                self.tool_items.len()
            ));
            if !self.saved_settings.clipboard_enabled {
                ui.weak("Capture is off · enable it in Settings");
            }
            let selected = self.tool_items.get(self.selected).cloned();
            if let Some(item) = &selected {
                if item.id != self.clipboard_preview_id {
                    self.clipboard_preview_id = item.id;
                    self.clipboard_texture = None;
                    if item.image {
                        let home = self.home.clone();
                        let id = item.id;
                        let tx = self.engine.tx.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(Event::ClipboardImage(
                                id,
                                crate::launcher::Clipboard::new(&home)
                                    .image(id)
                                    .map_err(|e| e.to_string()),
                            ));
                        });
                    }
                }
            } else {
                self.clipboard_texture = None;
                self.clipboard_preview_id = -1;
            }
            let height = ui.available_height().max(90.0);
            ui.columns(2, |columns| {
                let ui = &mut columns[0];
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("launcher-results")
                    .max_height(height);
                if self.scroll_to_selection {
                    let current = egui::scroll_area::State::load(
                        ctx,
                        ui.make_persistent_id("launcher-results"),
                    )
                    .map(|s| s.offset.y)
                    .unwrap_or(0.0);
                    scroll = scroll.vertical_scroll_offset(selection_offset(
                        self.selected,
                        72.0 + ui.spacing().item_spacing.y,
                        current,
                        height,
                    ));
                    self.scroll_to_selection = false;
                }
                scroll.show_rows(ui, 72.0, self.tool_items.len(), |ui, range| {
                    for i in range {
                        let item = self.tool_items[i].clone();
                        if item.image
                            && !self.clipboard_thumbs.contains_key(&item.id)
                            && self.clipboard_thumb_pending.insert(item.id)
                        {
                            let home = self.home.clone();
                            let tx = self.engine.tx.clone();
                            let id = item.id;
                            std::thread::spawn(move || {
                                let _ = tx.send(Event::ClipboardImage(
                                    id,
                                    crate::launcher::Clipboard::new(&home)
                                        .image(id)
                                        .map_err(|e| e.to_string()),
                                ));
                            });
                        }
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 72.0),
                            egui::Sense::click(),
                        );
                        let fill = if self.selected == i {
                            ui.visuals().selection.bg_fill
                        } else if response.hovered() {
                            ui.visuals().widgets.hovered.bg_fill
                        } else {
                            ui.visuals().widgets.inactive.bg_fill
                        };
                        ui.painter().rect(
                            rect,
                            8.0,
                            fill,
                            ui.visuals().widgets.inactive.bg_stroke,
                            egui::StrokeKind::Inside,
                        );
                        let mut left = rect.left() + 10.0;
                        if item.image {
                            let thumb = egui::Rect::from_min_size(
                                egui::pos2(left, rect.top() + 8.0),
                                egui::vec2(56.0, 56.0),
                            );
                            if let Some(texture) = self.clipboard_thumbs.get(&item.id) {
                                let size = texture.size_vec2();
                                let fit = size * (56.0 / size.x).min(56.0 / size.y);
                                let dst = egui::Rect::from_center_size(thumb.center(), fit);
                                ui.painter().image(
                                    texture.id(),
                                    dst,
                                    egui::Rect::from_min_max(
                                        egui::Pos2::ZERO,
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    Color32::WHITE,
                                );
                            } else {
                                crate::icons::image(crate::icons::Icon::Image, self.accent(), 30.0)
                                    .paint_at(ui, thumb);
                            }
                            left += 66.0;
                        }
                        let text_rect = egui::Rect::from_min_max(
                            egui::pos2(left, rect.top() + 7.0),
                            egui::pos2(rect.right() - 10.0, rect.top() + 45.0),
                        );
                        let galley = ui.painter().layout(
                            item.label.clone(),
                            egui::FontId::proportional(14.0),
                            ui.visuals().text_color(),
                            text_rect.width(),
                        );
                        ui.painter()
                            .with_clip_rect(text_rect.intersect(ui.clip_rect()))
                            .galley(text_rect.min, galley, ui.visuals().text_color());
                        ui.painter().text(
                            egui::pos2(left, rect.bottom() - 17.0),
                            egui::Align2::LEFT_CENTER,
                            &item.detail,
                            egui::FontId::proportional(11.0),
                            ui.visuals().weak_text_color(),
                        );
                        if response.clicked() {
                            self.selected = i;
                        }
                        if response.double_clicked() {
                            self.copy_tool_item(ctx, item.clone());
                        }
                        response.context_menu(|ui| {
                            if ui.button("Copy").clicked() {
                                self.copy_tool_item(ctx, item.clone());
                                ui.close();
                            }
                            if ui.button("Remove from history").clicked() {
                                let store = crate::launcher::Clipboard::new(&self.home);
                                let tx = self.engine.tx.clone();
                                std::thread::spawn(move || {
                                    let _ = tx.send(Event::ClipboardCleared(
                                        store.remove(item.id).map_err(|e| e.to_string()),
                                    ));
                                });
                                ui.close();
                            }
                        });
                    }
                });
                let ui = &mut columns[1];
                if let Some(item) = selected {
                    ui.weak(&item.detail);
                    if item.image {
                        if let Some(texture) = &self.clipboard_texture {
                            ui.add(
                                egui::Image::new(texture)
                                    .max_size(egui::vec2(ui.available_width(), height - 35.0)),
                            );
                        } else {
                            ui.spinner();
                        }
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("clipboard-text-preview")
                            .max_height(height - 35.0)
                            .show(ui, |ui| {
                                ui.add(egui::Label::new(&item.value).selectable(true).wrap());
                            });
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("Select a clipboard item to preview it.");
                    });
                }
            });
        }
    }
    fn open_settings_page(&mut self, page: usize, ctx: &egui::Context) {
        if self.quick {
            self.quick = false;
            self.target = 0;
            self.material_key = None;
            self.popup_size_state = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                780.0, 540.0,
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1120.0, 740.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::Normal,
            ));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.page = page;
    }
    fn llm(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("AI settings");
        ui.checkbox(&mut self.s.llm_enabled, "Enable AI chat");
        ui.weak("Select a model and save settings to show AI in search. MCP works independently.");
        ui.label("Connect your own OpenAI-compatible server. AI requests run only when you submit a question.");
        ui.label("Base URL");
        ui.text_edit_singleline(&mut self.s.llm_url);
        ui.weak("Example: http://localhost:8080/v1 (include /v1)");
        secret(ui, "API key (optional)", &mut self.s.llm_key);
        ui.horizontal(|ui| {
            ui.label("Model");
            egui::ComboBox::from_id_salt("llm-model")
                .selected_text(if self.s.llm_model.is_empty() {
                    "Select a model"
                } else {
                    &self.s.llm_model
                })
                .show_ui(ui, |ui| {
                    for model in &self.llm_models {
                        ui.selectable_value(&mut self.s.llm_model, model.clone(), model);
                    }
                });
            if ui
                .add_enabled(!self.llm_models_busy, egui::Button::new("Refresh models"))
                .clicked()
            {
                self.llm_models_busy = true;
                let s = self.s.clone();
                let tx = self.engine.tx.clone();
                std::thread::spawn(move || {
                    let result = crate::ai::models(&s).map_err(|e| e.to_string());
                    let _ = tx.send(Event::AiModels(s.llm_url.clone(), result));
                });
            }
            if self.llm_models_busy {
                ui.spinner();
            }
        });
        ui.label("Or enter a model ID");
        ui.text_edit_singleline(&mut self.s.llm_model);
        ui.checkbox(
            &mut self.s.llm_vision,
            "Selected model supports vision (screenshots)",
        );
        ui.weak("Screenshots are attached only after capture and pressing Send / Enter. Your configured server receives the image.");

        ui.add(
            egui::DragValue::new(&mut self.s.llm_context)
                .range(256..=1048576)
                .prefix("Max context: ")
                .suffix(" tokens"),
        );
        ui.weak("Estimated conversation + response budget, not a server context resize. Tokenizers differ. Old turns are removed automatically when needed.");
        ui.separator();
        ui.heading("Text to speech");
        let previous_tts = (self.s.tts_url.clone(), self.s.tts_model.clone());
        let previous_voice = self.s.tts_voice.clone();
        ui.label("API base URL");
        ui.text_edit_singleline(&mut self.s.tts_url);
        ui.weak("Example: http://localhost:8880/v1 (include /v1)");
        secret(ui, "TTS API key (optional)", &mut self.s.tts_key);
        if ui
            .add_enabled(
                !self.tts_catalog_busy,
                egui::Button::new("Refresh TTS models and voices"),
            )
            .clicked()
        {
            self.tts_catalog_busy = true;
            self.tts_notice.clear();
            let s = self.s.clone();
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let result = crate::tts::catalog(&s).map_err(|e| e.to_string());
                let _ = tx.send(Event::TtsCatalog(s.tts_url, s.tts_model, result));
            });
        }
        if self.tts_catalog_busy {
            ui.spinner();
        }
        egui::ComboBox::from_id_salt("tts-model")
            .selected_text(if self.s.tts_model.is_empty() {
                "Select TTS model"
            } else {
                &self.s.tts_model
            })
            .show_ui(ui, |ui| {
                for model in &self.tts_models {
                    ui.selectable_value(&mut self.s.tts_model, model.clone(), model);
                }
            });
        ui.label("Model ID");
        ui.text_edit_singleline(&mut self.s.tts_model);
        egui::ComboBox::from_id_salt("tts-voice")
            .selected_text(if self.s.tts_voice.is_empty() {
                "Select voice"
            } else {
                &self.s.tts_voice
            })
            .show_ui(ui, |ui| {
                for voice in &self.tts_voices {
                    ui.selectable_value(&mut self.s.tts_voice, voice.clone(), voice);
                }
            });
        ui.label("Voice ID");
        ui.text_edit_singleline(&mut self.s.tts_voice);
        ui.checkbox(&mut self.s.tts_auto, "Read AI responses aloud by default");
        ui.weak("Generated voice audio. Answers are sent to your TTS server only when read aloud is enabled or requested. Some servers do not offer voice discovery; enter the voice ID manually.");
        if previous_tts != (self.s.tts_url.clone(), self.s.tts_model.clone()) {
            self.tts_voice_job += 1;
            self.tts_voice_busy = false;
            self.tts_voices.clear();
            self.tts_notice.clear();
            if previous_tts.1 != self.s.tts_model {
                self.s.tts_voice.clear();
            }
            self.tts_voice_due = Some(Instant::now() + Duration::from_millis(350));
            ctx.request_repaint_after(Duration::from_millis(350));
        } else if previous_voice != self.s.tts_voice && !self.s.tts_voice.trim().is_empty() {
            self.tts_notice.clear();
        }
        if self.tts_voice_busy {
            ui.spinner();
            ui.weak("Refreshing voices…");
        }
        if !self.tts_notice.is_empty() && self.s.tts_voice.trim().is_empty() {
            ui.weak(&self.tts_notice);
        }
        ui.separator();
        ui.heading("MCP server");
        ui.checkbox(&mut self.s.mcp_enabled, "Enable read-only agent access");
        ui.checkbox(&mut self.s.mcp_lan, "Allow local-network access");
        ui.checkbox(&mut self.s.mcp_auth, "Require HTTP bearer authentication");
        ui.add(
            egui::DragValue::new(&mut self.s.mcp_port)
                .range(1024..=65535)
                .prefix("Port "),
        );
        secret(ui, "Bearer token", &mut self.s.mcp_token);
        if ui.button("Generate token").clicked() {
            self.s.mcp_token = crate::config::random_key();
        }
        if ui.button("Copy MCP connection").clicked() {
            let host = if self.s.mcp_lan {
                crate::platform::local_ip()
            } else {
                "127.0.0.1".into()
            };
            let mut v = serde_json::json!({"url":format!("http://{host}:{}/mcp",self.s.mcp_port),"transport":"streamable-http"});
            if self.s.mcp_auth {
                v["headers"] =
                    serde_json::json!({"Authorization":format!("Bearer {}",self.s.mcp_token)});
            }
            ctx.copy_text(v.to_string());
        }
        ui.label("LAN HTTP is unencrypted. Use trusted clients and keep authentication enabled.");
    }

    fn about(&mut self, ui: &mut egui::Ui) {
        ui.heading("About VectorDash");
        ui.label(format!(
            "Version {} · Native Rust · Windows 11 x64",
            env!("CARGO_PKG_VERSION")
        ));
        ui.label("Local universal file search and launcher, powered by EmbeddingGemma 2.");
        ui.label("License: MIT · VectorDash contributors");
        ui.hyperlink_to(
            "GitHub and support",
            "https://github.com/ampersandru/VectorDash",
        );
        ui.hyperlink_to(
            "License and acknowledgements",
            "https://github.com/ampersandru/VectorDash/blob/main/ACKNOWLEDGEMENTS.md",
        );
        ui.label("Developed with AI assistance; verify important results.");
        ui.separator();
        ui.heading("Updates");
        ui.label(format!(
            "Installed version: {} · Official Rust release",
            env!("CARGO_PKG_VERSION")
        ));
        ui.checkbox(
            &mut self.s.auto_check_updates,
            "Check for updates at startup",
        );
        let portable = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("portable.flag").exists()))
            .unwrap_or(false);
        if !portable {
            ui.checkbox(
                &mut self.s.auto_update,
                "Automatically download and install updates when indexing is idle",
            );
        } else {
            ui.label("Portable updates: download the new portable package from the release page.");
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.update_busy, egui::Button::new("Check for updates"))
                .clicked()
            {
                self.check_update();
            }
            if self.update_release.is_some()
                && !portable
                && self.update_installer.is_none()
                && ui
                    .add_enabled(!self.update_busy, egui::Button::new("Download update"))
                    .clicked()
            {
                self.download_update();
            }
            if self.update_installer.is_some()
                && ui
                    .add_enabled(
                        !self.indexing && !self.setup_busy,
                        egui::Button::new("Install update and restart"),
                    )
                    .clicked()
            {
                self.install_update(ui.ctx());
            }
            if self.update_release.is_some() && ui.button("View changelog").clicked() {
                self.show_changelog = true;
            }
            if let Some(release) = &self.update_release {
                ui.hyperlink_to("Release notes", &release.page);
            }
        });
        ui.label(&self.update_status);
        ui.label("Updates come from the VectorDash GitHub repository and must pass SHA-256 verification. The installer is unsigned.");
        ui.separator();
    }
    fn scheduling(&mut self, ui: &mut egui::Ui) {
        ui.heading("Scheduling");
        ui.checkbox(
            &mut self.s.watch_changes,
            "Automatically index changed files and folders (respects idle rules)",
        );
        if self.s.watch_changes {
            ui.label(&self.watch_status);
        } else {
            ui.label("File monitoring is off");
        }
        if self.engine.control.automatic_hold.load(Ordering::Acquire)
            && !self.engine.control.sources_pending.load(Ordering::Acquire)
            && !self
                .engine
                .control
                .restart_requested
                .load(Ordering::Acquire)
            && ui.button("Resume automatic indexing").clicked()
        {
            self.engine
                .control
                .cancelled_by_user
                .store(false, Ordering::SeqCst);
            self.engine
                .control
                .automatic_hold
                .store(false, Ordering::SeqCst);
        }

        ui.label("Automatic file changes obey the idle conditions below, including when the periodic scan schedule is Manual. Manual only disables periodic scans.");
        ui.checkbox(
            &mut self.s.ntfs_journal,
            "Use NTFS change journal when available",
        );
        ui.label("Requires volume-read permission and an existing NTFS journal. Unsupported or inaccessible drives keep using folder notifications and recovery scans.");
        ui.separator();
        combo(
            ui,
            "schedule",
            &mut self.s.schedule,
            &["Manual", "At selected time", "When idle", "Time + idle"],
        );
        let timed = ["At selected time", "Time + idle"].contains(&self.s.schedule.as_str());
        let idle = self.s.watch_changes
            || ["When idle", "Time + idle"].contains(&self.s.schedule.as_str());
        if timed {
            ui.horizontal(|ui| {
                ui.label("Time (this PC's local time)");
                ui.add(egui::DragValue::new(&mut self.s.schedule_hour).range(0..=23));
                ui.label(":");
                ui.add(egui::DragValue::new(&mut self.s.schedule_minute).range(0..=59));
            });
            ui.horizontal(|ui| {
                for (i, name) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                    .iter()
                    .enumerate()
                {
                    ui.checkbox(&mut self.s.schedule_days[i], *name);
                }
            });
        }
        if self.s.schedule == "When idle" {
            ui.add(
                egui::Slider::new(&mut self.s.idle_interval_minutes, 5..=1440)
                    .text("Minimum minutes between idle updates"),
            );
        }
        if idle {
            ui.heading("Idle conditions");
            ui.checkbox(
                &mut self.s.check_user_idle,
                "Require keyboard/mouse inactivity",
            );
            if self.s.check_user_idle {
                ui.add(
                    egui::Slider::new(&mut self.s.idle_seconds, 30..=7200).text("Inactive seconds"),
                );
            }
            ui.checkbox(&mut self.s.check_cpu, "Require low CPU usage");
            if self.s.check_cpu {
                ui.add(
                    egui::Slider::new(&mut self.s.cpu_limit, 1.0..=100.0)
                        .text("CPU usage below (%)"),
                );
            }
            ui.checkbox(&mut self.s.check_gpu, "Require low GPU usage");
            if self.s.check_gpu {
                ui.add(
                    egui::Slider::new(&mut self.s.gpu_limit, 1.0..=100.0)
                        .text("GPU usage below (%)"),
                );
            }
            ui.label("Thresholds measure other apps, excluding VectorDash, its model server and extraction helpers. Conditions are checked between files and embedding batches. If GPU usage cannot be read, turn off the GPU condition to allow scheduled indexing.");
        }
        if self.s.schedule != "Manual" || self.s.watch_changes {
            ui.checkbox(&mut self.s.ac_only, "Automatic indexing only on AC power");
            ui.add(
                egui::Slider::new(&mut self.s.max_run_minutes, 5..=1440)
                    .text("Maximum automatic run (minutes)"),
            );
        }
        ui.add_space(12.0);
        ui.heading("Schedule status");
        ui.label(&self.schedule_status);
        if idle {
            let metrics = crate::scheduling::metrics();
            ui.label(format!(
                "Other apps: CPU {} · GPU {} · {}",
                metrics
                    .cpu
                    .map(|v| format!("{v:.0}%"))
                    .unwrap_or("unavailable".into()),
                metrics
                    .gpu
                    .map(|v| format!("{v:.0}%"))
                    .unwrap_or("unavailable".into()),
                if self.s.check_user_idle {
                    format!("Keyboard/mouse inactive {} seconds", metrics.idle)
                } else {
                    "Keyboard/mouse inactivity ignored".into()
                }
            ));
        }
        if self.s.last_success > 0 {
            if let Some(date) = chrono::DateTime::from_timestamp(self.s.last_success, 0) {
                ui.label(format!(
                    "Last successful update: {}",
                    date.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")
                ));
            }
        }
        ui.collapsing("Recent automatic runs", |ui| {
            for item in &self.s.schedule_history {
                ui.label(item);
            }
        });
        if ui
            .add_enabled(!self.indexing, egui::Button::new("Update now"))
            .clicked()
        {
            self.start_index(false);
        }
        ui.label("VectorDash must remain running. Closing to tray keeps schedules active; it does not wake a sleeping PC.");
    }
    fn logs(&mut self, ui: &mut egui::Ui) {
        ui.heading("Logs");
        ui.horizontal(|ui| {
            if ui
                .selectable_label(!self.model_logs && !self.file_logs, "VectorDash logs")
                .clicked()
            {
                self.model_logs = false;
                self.file_logs = false;
            }
            if ui.selectable_label(self.model_logs, "Model logs").clicked() {
                self.model_logs = true;
                self.file_logs = false;
            }
            if ui.selectable_label(self.file_logs, "File log").clicked() {
                self.model_logs = false;
                self.file_logs = true;
            }
        });
        if self.model_logs || self.file_logs {
            let mut text = tail_log(&self.home.join(if self.file_logs {
                "files.log"
            } else {
                "model.log"
            }))
            .unwrap_or_else(|_| {
                if self.file_logs {
                    "No file changes recorded yet".into()
                } else {
                    "No model has loaded yet".into()
                }
            });
            if self.model_logs {
                if let Ok(speech) = tail_log(&self.home.join("dictation.log")) {
                    text.push_str("\nParakeet\n");
                    text.push_str(&speech);
                }
            }
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.monospace(
                        text.chars()
                            .rev()
                            .take(64000)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect::<String>(),
                    );
                });
        } else {
            ui.horizontal(|ui| {
                for (i, name) in ["Verbose", "Info", "Warning", "Error"].iter().enumerate() {
                    ui.checkbox(&mut self.log_filter[i], *name);
                }
            });
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for (level, text) in &self.logs {
                        let i = match level.as_str() {
                            "Debug" => 0,
                            "Info" => 1,
                            "Warning" => 2,
                            _ => 3,
                        };
                        if self.log_filter[i] {
                            ui.label(RichText::new(format!("{level} {text}")).monospace().color(
                                if i == 2 {
                                    crate::appearance::component_color(
                                        ui.visuals().dark_mode,
                                        false,
                                        true,
                                    )
                                } else if i == 3 {
                                    Color32::LIGHT_RED
                                } else {
                                    ui.visuals().text_color()
                                },
                            ));
                        }
                    }
                });
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        use raw_window_handle::HasWindowHandle;
        if self.smoke.is_some() && self.tool == "AI" {
            let args = std::env::args().collect::<Vec<_>>();
            if let Some(question) = args
                .iter()
                .position(|v| v == "--smoke-followup")
                .and_then(|i| args.get(i + 1))
            {
                if self.startup.elapsed() > Duration::from_secs(2) && !self.qa_ai_scrolled {
                    self.qa_ai_scrolled = true;
                    self.ai_scroll = 10000.0;
                }
                if self.startup.elapsed() > Duration::from_secs(3) && !self.qa_ai_followup_sent {
                    self.qa_ai_followup_sent = true;
                    self.qa_ai_scroll_before = self.ai_scroll_offset;
                    self.query = question.clone();
                    self.ask_ai();
                    self.qa_capture_sent = false;
                    self.qa_captured = false;
                }
            }
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::F)) {
            self.page = 0;
            self.select_query = true;
            self.filter_open = false;
            self.save_search_open = false;
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("query")));
        }
        if self.page == 0
            && !self.hotkey_capture
            && !self.llm_hotkey_capture
            && !self.voice_hotkey_capture
            && !self.global_voice_capture
            && !self.filter_open
            && !self.save_search_open
            && !self.capture_open
        {
            let cycle=ctx.input_mut(|i| {
                let found=i.events.iter().any(|e|matches!(e,egui::Event::Key{key,pressed:true,modifiers,..} if (*key==egui::Key::Tab&&(modifiers.is_none()||*modifiers==egui::Modifiers::SHIFT))||crate::ai::shortcut(*key,*modifiers)==self.saved_settings.llm_hotkey));
                if found{i.events.retain(|e|!matches!(e,egui::Event::Key{key,modifiers,..} if (*key==egui::Key::Tab&&(modifiers.is_none()||*modifiers==egui::Modifiers::SHIFT))||crate::ai::shortcut(*key,*modifiers)==self.saved_settings.llm_hotkey));}found
            });
            if cycle {
                let backwards = ctx.input(|i| i.modifiers.shift);
                let next = self.saved_settings.next_search_tool(&self.tool, backwards);
                if next != self.tool {
                    self.switch_tool(next);
                }
                ctx.memory_mut(|m| {
                    m.move_focus(egui::FocusDirection::None);
                    m.request_focus(egui::Id::new("query"));
                });
            }
        }
        if self.page == 0 && !self.voice_hotkey_capture {
            let invoke=ctx.input_mut(|i|{
                let found=i.events.iter().any(|e|matches!(e,egui::Event::Key{key,pressed:true,modifiers,..} if crate::ai::shortcut(*key,*modifiers)==self.saved_settings.dictation_hotkey));
                if found{i.events.retain(|e|!matches!(e,egui::Event::Key{key,modifiers,..} if crate::ai::shortcut(*key,*modifiers)==self.saved_settings.dictation_hotkey));}found
            });
            if invoke {
                self.toggle_dictation();
            }
        }
        if self.page == 0 && self.tool == "AI" {
            let up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
            let down =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
            if up || down {
                self.ai_scroll = ctx.content_rect().height() * 0.6 * if up { 1.0 } else { -1.0 };
            }
        }
        for error in crate::platform::playback_errors() {
            self.status = error.clone();
            self.log("Error".into(), error);
        }
        if self.status.starts_with("Playing from") && !crate::platform::playback_active() {
            self.status = "Playback finished".into();
        }
        let appearance = format!(
            "{}{:?}{:?}{}{}",
            self.s.theme,
            self.s.accent,
            self.s.popup_tone,
            self.s.popup_opacity,
            self.s.popup_gradient
        ) + &format!("{}", ctx.style().visuals.dark_mode);
        if appearance != self.appearance_key {
            crate::appearance::apply(ctx, &self.s);
            self.appearance_key = appearance;
        }
        if crate::platform::main_window_visible() {
            crate::appearance::background(ctx, &self.s, self.quick);
        }
        if self.smoke.is_none()
            && !self.exiting
            && ctx.input(|i| i.viewport().close_requested())
            && self.s.close_behavior != "Exit"
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_window(ctx);
        }
        let key = self.s.popup_opacity.to_bits() as u64
            | ((self.quick as u64) << 32)
            | ((self.s.popup_tone[0] as u64) << 33)
            | ((self.s.popup_tone[1] as u64) << 41)
            | ((self.s.popup_tone[2] as u64) << 49)
            | ((ctx.style().visuals.dark_mode as u64) << 57);
        let material_changed = self.material_key != Some(key);
        if material_changed {
            if let Ok(handle) = frame.window_handle() {
                if let raw_window_handle::RawWindowHandle::Win32(win) = handle.as_raw() {
                    crate::platform::material(
                        win.hwnd.get(),
                        self.quick,
                        ctx.style().visuals.dark_mode,
                        self.s.popup_tone,
                        self.s.popup_opacity,
                    );
                    self.material_key = Some(key);
                }
            }
        }
        // Apply after material changes and native resizing; validate the real region.
        if let Ok(handle) = frame.window_handle() {
            if let raw_window_handle::RawWindowHandle::Win32(win) = handle.as_raw() {
                crate::platform::clip_window(
                    win.hwnd.get(),
                    16.0 * ctx.pixels_per_point(),
                    material_changed,
                );
            }
        }
        if self.smoke.is_some() {
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot {
                    image, viewport_id, ..
                } = event
                {
                    if viewport_id != egui::ViewportId::ROOT {
                        self.qa_child_size = Some(image.size);
                        if let Some(path) = &self.qa_screenshot {
                            let bytes: Vec<u8> =
                                image.pixels.iter().flat_map(|p| p.to_array()).collect();
                            if let Some(rgba) = image::RgbaImage::from_raw(
                                image.width() as u32,
                                image.height() as u32,
                                bytes,
                            ) {
                                let _ = rgba.save(path.with_file_name("quick-look.png"));
                            }
                        }
                    }
                    if viewport_id == egui::ViewportId::ROOT {
                        if let Some(path) = &self.qa_screenshot {
                            let pixels: Vec<u8> =
                                image.pixels.iter().flat_map(|p| p.to_array()).collect();
                            if let Some(image) = image::RgbaImage::from_raw(
                                image.width() as u32,
                                image.height() as u32,
                                pixels,
                            ) {
                                self.qa_captured = image.save(path).is_ok();
                            }
                        }
                    }
                }
            }
            if self.qa_hidden
                && !self.qa_hide_sent
                && self.startup.elapsed() > Duration::from_millis(300)
            {
                self.qa_hide_sent = true;
                self.close_window(ctx);
                if !std::env::args().any(|a| a == "--smoke-tray") {
                    let tx = self.engine.tx.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(700));
                        let _ = tx.send(Event::Summon(0, "settings: display".into()));
                    });
                }
            }
            if self.qa_screenshot.is_some()
                && !self.qa_capture_sent
                && self.startup.elapsed() > Duration::from_secs(2)
            {
                self.qa_capture_sent = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
        let now = Instant::now();
        let frame_ms = now.duration_since(self.last_frame).as_secs_f64() * 1000.0;
        if frame_ms > 1000.0
            && self.frames > 0
            && self.search_visible
            && ctx.input(|i| i.viewport().focused.unwrap_or(false))
        {
            self.engine.log(
                "Warning",
                format!(
                    "UI frame delayed {frame_ms:.0} ms; indexing={}, search={}, page={}",
                    self.engine.control.running.load(Ordering::Relaxed),
                    self.searching,
                    self.page
                ),
            );
        }
        self.max_frame_ms = self
            .max_frame_ms
            .max(now.duration_since(self.last_frame).as_secs_f64() * 1000.0);
        self.last_frame = now;
        self.frames += 1;
        if self.smoke.is_some()
            && self.frames == 3
            && std::env::args().any(|a| a == "--smoke-voice-summon")
        {
            let _ = self.engine.tx.send(Event::VoiceSummon(0));
        }
        if self.frames == 2
            && self.smoke.is_none()
            && (self.s.auto_check_updates || self.s.auto_update)
        {
            self.check_update();
        }
        if self.s.auto_update
            && self.update_installer.is_some()
            && !self.engine.control.running.load(Ordering::Relaxed)
            && !self.setup_busy
            && !self.exiting
        {
            self.install_update(ctx);
        }
        if self.tts_voice_due.is_some_and(|t| Instant::now() >= t) {
            self.tts_voice_due = None;
            self.tts_voice_job += 1;
            let job = self.tts_voice_job;
            if !self.s.tts_model.trim().is_empty() && !self.s.tts_url.trim().is_empty() {
                self.tts_voice_busy = true;
                let s = self.s.clone();
                let tx = self.engine.tx.clone();
                std::thread::spawn(move || {
                    let result = crate::tts::voices(&s).map_err(|e| e.to_string());
                    let _ = tx.send(Event::TtsVoices(job, s.tts_url, s.tts_model, result));
                });
            }
        }
        self.events(ctx);
        if !self.saved_settings.tool_available(&self.tool) {
            if self.tool == "AI" {
                self.stop_ai();
            }
            self.switch_tool("Search");
        }
        if !crate::platform::main_window_visible()
            || ctx.input(|i| i.viewport().minimized.unwrap_or(false))
        {
            self.texture = None;
            self.clipboard_texture = None;
            self.clipboard_thumbs.clear();
            self.clipboard_preview_id = -1;
            self.color_emoji.clear(ctx);
            self.preview_pending = None;
            self.search_visible = false;
            self.engine
                .models
                .session_override
                .store(false, Ordering::Release);
        } else {
            if !self.search_visible {
                self.model_warm_attempted = false;
            }
            self.search_visible = true;
        }
        if !self.search_visible
            && !self.ai_busy
            && !self.dictation_busy
            && !self.capture_open
            && !self.exiting
        {
            // Hotkeys and model/file services have their own threads and wake through events.
            // Do not build controls or animate the renderer while the root window is hidden.
            ctx.request_repaint_after(Duration::from_secs(5));
            return;
        }
        self.warm_search_model();

        let copy_event = ctx.input(|i| {
            i.events.iter().any(|e| matches!(e, egui::Event::Copy))
                || i.modifiers.ctrl && i.key_pressed(egui::Key::C)
        });
        let query_focus = ctx.memory(|m| m.has_focus(egui::Id::new("query")));
        let query_selection = egui::text_edit::TextEditState::load(ctx, egui::Id::new("query"))
            .and_then(|s| s.cursor.char_range())
            .is_some_and(|r| !r.is_empty());
        let copy_ai_request =
            copy_event && self.page == 0 && self.tool == "AI" && !self.ai_answer.is_empty();
        let copy_tool_request = copy_event
            && self.page == 0
            && ["Emoji", "Clipboard"].contains(&self.tool.as_str())
            && !self.tool_items.is_empty()
            && (!ctx.wants_keyboard_input() || query_focus && !query_selection);
        let copy_file_request = copy_event
            && self.page == 0
            && self.tool == "Search"
            && !self.query.trim().is_empty()
            && !self.hits.is_empty()
            && (!ctx.wants_keyboard_input() || query_focus && !query_selection);
        if copy_ai_request {
            ctx.copy_text(self.ai_answer.clone());
        }
        if copy_file_request || copy_tool_request || copy_ai_request {
            ctx.input_mut(|i|i.events.retain(|e|!matches!(e,egui::Event::Copy) && !matches!(e,egui::Event::Key{key:egui::Key::C,modifiers,..} if modifiers.ctrl)));
        }
        if self.dirty
            && self.changed.elapsed()
                > Duration::from_millis(if self.s.performance == "Fast search" {
                    35
                } else {
                    75
                })
        {
            self.dirty = false;
            self.dispatch();
        }
        self.indexing = self.engine.control.running.load(Ordering::Relaxed);
        if self.indexing
            && !self
                .engine
                .control
                .cancelled_by_user
                .load(Ordering::Acquire)
            && !self.engine.control.sources_pending.load(Ordering::Acquire)
            && crate::engine::source_signature(&self.s)
                != crate::engine::source_signature(&self.engine.control.schedule.lock().unwrap())
        {
            self.engine.control.interrupt_sources();
            self.status =
                "Sources changed; stopping the old scan. Save to restart with updated sources."
                    .into();
        }

        let metrics = crate::scheduling::metrics();
        if self.indexing {
            if let Some(reason) = crate::scheduling::resource_pause(&self.s, metrics) {
                self.progress.phase = format!("Indexing paused: {reason}");
            }
        }
        self.schedule_status =
            if self.engine.control.automatic_hold.load(Ordering::Acquire) && !self.indexing {
                if self.engine.control.sources_pending.load(Ordering::Acquire) {
                    "Sources changed; save to restart".into()
                } else if self
                    .engine
                    .control
                    .restart_requested
                    .load(Ordering::Acquire)
                {
                    "Source restart queued; waiting for idle conditions".into()
                } else {
                    "Automatic indexing paused after Cancel".into()
                }
            } else if self.indexing && self.engine.control.automatic.load(Ordering::Relaxed) {
                if self.engine.control.watching.load(Ordering::Relaxed)
                    || ["When idle", "Time + idle"].contains(&self.s.schedule.as_str())
                {
                    crate::scheduling::idle_allowed(&self.s, metrics)
                        .map(|_| "Automatic indexing in progress".into())
                        .unwrap_or_else(str::to_string)
                } else {
                    "Automatic indexing in progress".into()
                }
            } else {
                crate::scheduling::due(&self.s, chrono::Local::now(), metrics)
                    .map(|_| "Automatic indexing is ready".into())
                    .unwrap_or_else(|reason| reason)
            };
        ctx.request_repaint_after(Duration::from_millis(if self.dictation_busy {
            33
        } else {
            250
        }));
        if self.position_mode != Some(self.quick) {
            let _ = self.window_state.save(&self.home);
            if let Some(pos) = if self.quick {
                self.window_state.popup
            } else {
                self.window_state.main
            } {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                    pos[0], pos[1],
                )));
            }
            self.position_mode = Some(self.quick);
            self.position_settle = 3;
        } else if self.position_settle > 0 {
            self.position_settle -= 1;
        } else if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            let point = [rect.min.x, rect.min.y];
            if point.iter().all(|v| v.is_finite()) {
                if self.quick {
                    self.window_state.popup = Some(point);
                } else {
                    self.window_state.main = Some(point);
                }
            }
        }
        crate::platform::clear_drag_regions();
        if !self.quick {
            self.titlebar(ctx);
        }
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(20, 9)))
            .show(ctx, |ui| {
                if self.page == 0 {
                    if !self.tts_status.is_empty() {
                        ui.label(&self.tts_status);
                    }
                    if self.tts_busy {
                        ui.horizontal(|ui| {
                            ui.label("Reading aloud…");
                            if ui.button("Stop voice").clicked() {
                                self.stop_read_aloud();
                            }
                        });
                    }
                    if !self.dictation_status.is_empty()
                        && !self.dictation_status.starts_with("Recording")
                    {
                        ui.label(&self.dictation_status);
                    }
                    if self.tool == "AI" {
                        ui.weak(format!("AI model: {}", self.saved_settings.llm_model));
                        if let Some(error) = &self.ai_error {
                            ui.colored_label(Color32::LIGHT_RED, error);
                        }
                        if self.ai_context_trimmed {
                            ui.weak("Older turns removed to fit Max context.");
                        }
                    }
                }
                let footer = self.footer_text();
                ui.horizontal_wrapped(|ui| {
                    if self.quick && ui.button("Open full app").clicked() {
                        let _ = self.engine.tx.send(Event::Full);
                    }
                    ui.add_sized(
                        [(ui.available_width() - 550.0).max(120.0), 22.0],
                        egui::Label::new(&footer).truncate(),
                    )
                    .on_hover_text(&footer);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (key, action) in [
                            ("Esc", if self.ai_busy { "stop" } else { "dismiss" }),
                            (
                                if self.saved_settings.search_tools().len() > 1 {
                                    "Tab"
                                } else {
                                    ""
                                },
                                "cycle tools",
                            ),
                            ("Ctrl+C", "copy"),
                            (if self.tool == "Search" { "Space" } else { "" }, "preview"),
                            (
                                "Enter",
                                if self.tool == "AI" {
                                    "send"
                                } else if self.tool == "Search" {
                                    "open"
                                } else {
                                    "copy"
                                },
                            ),
                            ("↑ ↓", if self.tool == "AI" { "page" } else { "select" }),
                        ] {
                            if key.is_empty() {
                                continue;
                            }
                            ui.label(action);
                            ui.label(
                                RichText::new(key)
                                    .font(egui::FontId::new(
                                        12.0,
                                        egui::FontFamily::Name("bold".into()),
                                    ))
                                    .color(self.accent()),
                            );
                        }
                    });
                });
            });
        if !self.quick {
            egui::SidePanel::left("navigation")
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 18)))
                .exact_width(if self.collapsed { 68.0 } else { 204.0 })
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("nav-pages")
                        .max_height(
                            (ui.available_height() - if self.collapsed { 40.0 } else { 120.0 })
                                .max(100.0),
                        )
                        .show(ui, |ui| {
                            for (i, label) in PAGES.iter().enumerate() {
                                let icon = [
                                    crate::icons::Icon::Search,
                                    crate::icons::Icon::Sources,
                                    crate::icons::Icon::Models,
                                    crate::icons::Icon::Schedule,
                                    crate::icons::Icon::Settings,
                                    crate::icons::Icon::Robot,
                                    crate::icons::Icon::Logs,
                                    crate::icons::Icon::About,
                                ][i];
                                let color = if i == 0 {
                                    Color32::WHITE
                                } else {
                                    self.accent()
                                };
                                let button = if self.collapsed {
                                    egui::Button::image(crate::icons::image(icon, color, 22.0))
                                } else {
                                    egui::Button::image_and_text(
                                        crate::icons::image(icon, color, 22.0),
                                        *label,
                                    )
                                };
                                let response = ui
                                    .add(
                                        button
                                            .fill(if i == 0 {
                                                self.accent()
                                            } else if self.page == i {
                                                self.accent().gamma_multiply(0.25)
                                            } else {
                                                Color32::TRANSPARENT
                                            })
                                            .min_size(egui::vec2(ui.available_width(), 46.0)),
                                    )
                                    .on_hover_text(*label);
                                if response.clicked() {
                                    self.page = i;
                                }
                            }
                        });
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        let exit = if self.collapsed {
                            ui.add(egui::Button::image(crate::icons::image(
                                crate::icons::Icon::Close,
                                self.accent(),
                                22.0,
                            )))
                            .on_hover_text("Exit VectorDash")
                        } else {
                            ui.button("Exit VectorDash")
                        };
                        if exit.clicked() {
                            self.exiting = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if !self.collapsed {
                            ui.label("Your files and data stay on your computer");
                            ui.label("Everything runs locally");
                        }
                    });
                });
        }
        if self.quick {
            let focused = ctx.input(|i| i.viewport().focused.unwrap_or(false));
            if focused {
                self.popup_had_focus = true;
            }
            let outside_down = crate::platform::left_button_down();
            let outside_click = outside_down && !self.popup_outside_down && !focused;
            self.popup_outside_down = outside_down;
            if (self.ai_busy
                || self.dictation_busy
                || self.capture_open
                || self.save_search_open
                || self.filter_open)
                && !focused
            {
                self.popup_had_focus = false;
            }
            if (self.popup_had_focus && !focused || outside_click)
                && !self.large_preview
                && !self.ai_busy
                && !self.dictation_busy
                && !self.capture_open
                && !self.save_search_open
                && !self.filter_open
            {
                self.popup_had_focus = false;
                self.search_visible = false;
                self.engine
                    .models
                    .session_override
                    .store(false, Ordering::Release);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            let empty = self.query.is_empty() && self.tool == "Search";
            let expansion =
                ctx.animate_bool_with_time(egui::Id::new("popup-expansion"), !empty, 0.18);
            let panel = self.tool == "Search"
                && self.mode != "Names"
                && self.s.remote_url.is_empty()
                && (self.engine.models.loading.load(Ordering::Acquire)
                    || self.engine.models.search_hold(&self.s).is_some()
                    || self.model_warm_error.is_some()
                    || (self.s.show_model_notices
                        && !self.engine.models.loaded.load(Ordering::Acquire)));
            if self.popup_size_state != Some(empty)
                || self.popup_model_panel != panel
                || (expansion > 0.0 && expansion < 1.0)
            {
                self.popup_model_panel = panel;
                self.popup_size_state = Some(empty);
                ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                    780.0, 200.0,
                )));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    940.0,
                    270.0 + 370.0 * expansion + if panel { 75.0 } else { 0.0 },
                )));
            }
        } else {
            self.popup_size_state = None;
        }
        if !self.quick
            && !crate::config::same_preferences(&self.pending_settings(), &self.saved_settings)
        {
            egui::TopBottomPanel::bottom("pending-settings")
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(20, 10)))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Unsaved changes");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !self.index_clearing,
                                    egui::Button::new("Undo")
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(egui::Stroke::new(1.0, self.accent())),
                                )
                                .clicked()
                            {
                                self.undo_settings(ctx);
                            }
                            if ui
                                .add_enabled(
                                    !self.index_clearing,
                                    egui::Button::new(
                                        RichText::new("Save").strong().color(Color32::WHITE),
                                    )
                                    .fill(Color32::from_rgb(195, 45, 65)),
                                )
                                .clicked()
                            {
                                self.apply_settings(ctx);
                            }
                        });
                    });
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(20.0))
            .show(ctx, |ui| {
                if self.page == 0 {
                    self.search_ui(ui, ctx);
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| match self.page {
                        1 => self.sources(ui),
                        2 => self.models(ui),
                        3 => self.scheduling(ui),
                        4 => self.settings(ui, ctx),
                        5 => self.llm(ui, ctx),
                        6 => self.logs(ui),
                        _ => self.about(ui),
                    });
                }
            });
        if self.page == 0 && self.tool != "AI" && ctx.input(|i| i.key_pressed(egui::Key::ArrowDown))
        {
            ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("query")));
            self.scroll_to_selection = true;
            let step = if self.tool == "Emoji" {
                ((ctx.content_rect().width()
                    - if self.quick {
                        40.0
                    } else if self.collapsed {
                        108.0
                    } else {
                        244.0
                    })
                    / 62.0)
                    .floor()
                    .max(1.0) as usize
            } else {
                1
            };
            self.selected = (self.selected + step).min(
                if self.tool == "Search" {
                    self.hits.len()
                } else {
                    self.tool_items.len()
                }
                .saturating_sub(1),
            );
            self.texture = None;
            self.preview_error = None;
        }
        if self.page == 0 && self.tool != "AI" && ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("query")));
            self.scroll_to_selection = true;
            let step = if self.tool == "Emoji" {
                ((ctx.content_rect().width()
                    - if self.quick {
                        40.0
                    } else if self.collapsed {
                        108.0
                    } else {
                        244.0
                    })
                    / 62.0)
                    .floor()
                    .max(1.0) as usize
            } else {
                1
            };
            self.selected = self.selected.saturating_sub(step);
            self.texture = None;
            self.preview_error = None;
        }
        if self.page == 0
            && self.tool == "Search"
            && !self.query.trim().is_empty()
            && !self.hits.is_empty()
            && !ctx.memory(|m| m.has_focus(egui::Id::new("query")))
            && ctx.input(|i| i.key_pressed(egui::Key::Space))
        {
            self.large_preview = !self.large_preview;
        }
        if self.page == 0
            && (!self.query.trim().is_empty() || self.tool == "Emoji" || self.tool == "Clipboard")
            && !self.save_search_open
            && !self.filter_open
            && !self.capture_open
            && ctx.input(|i| i.key_pressed(egui::Key::Enter))
        {
            self.open_selected(ctx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.save_search_open {
                self.save_search_open = false;
            } else if self.filter_open {
                self.filter_open = false;
            } else if self.large_preview {
                self.large_preview = false;
            } else if self.dictation_busy {
                self.dictation_cancel.store(true, Ordering::Release);
            } else if self.capture_open {
                self.capture_open = false;
                self.capture_bytes = None;
                self.capture_texture = None;
            } else if self.ai_busy {
                self.stop_ai();
            } else {
                self.close_window(ctx);
            }
        }

        if copy_file_request {
            if let Some(hit) = self.hits.get(self.selected).cloned() {
                if hit.kind == "calculator" {
                    ctx.copy_text(hit.path);
                } else {
                    self.run(crate::platform::copy_file(&hit.path));
                }
            }
        }
        if copy_tool_request {
            if let Some(item) = self.tool_items.get(self.selected).cloned() {
                self.copy_tool_item(ctx, item.clone());
            }
        }
        if (crate::dictation::recording()
            || self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-recording"))
            && self.search_visible
        {
            paint_recording_outline(ctx);
        } else if (self.ai_busy
            || self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-ai-processing"))
            && self.search_visible
        {
            paint_ai_outline(ctx);
        }
        self.capture_ui(ctx);
        if let Some(image) = self.ai_image_view.clone() {
            let mut open = true;
            egui::Window::new("Sent screenshot")
                .open(&mut open)
                .default_width(700.0)
                .show(ctx, |ui| {
                    if let Ok(bytes) = image_bytes(&image) {
                        ui.add(
                            egui::Image::from_bytes(
                                format!("bytes://ai-screenshot-{}.jpg", image_key(&image)),
                                bytes,
                            )
                            .max_size(egui::vec2(ui.available_width(), 500.0)),
                        );
                    }
                    if ui.button("Copy screenshot").clicked() {
                        self.run(copy_ai_image(&image));
                    }
                });
            if !open {
                self.ai_image_view = None;
            }
        }
        if let Some(key) = self.remove_component.clone() {
            egui::Window::new("Uninstall downloaded item?").collapsible(false).resizable(false).show(ctx,|ui|{
                ui.label(format!("Remove the downloaded {key} files? Index data is kept. Active indexing and model work will stop."));
                ui.horizontal(|ui|{
                    if ui.button("Uninstall").clicked(){
                        self.remove_component=None;self.setup_busy=true;self.dictation_cancel.store(true,Ordering::Release);self.engine.control.cancel_by_user();
                        let home=self.home.clone();let settings=self.s.clone();let models=self.engine.models.clone();let tx=self.engine.tx.clone();
                        std::thread::spawn(move||{let result=models.stop().and_then(|_|{
                            crate::dictation::stop();
                            let deadline=Instant::now()+Duration::from_secs(10);
                            while crate::dictation::in_use() && Instant::now()<deadline {std::thread::sleep(Duration::from_millis(50));}
                            anyhow::ensure!(!crate::dictation::in_use(),"Dictation is still stopping; retry uninstall shortly");
                            crate::resources::uninstall(&home,&settings,&key)
                        }).map_err(|e|e.to_string());let _=tx.send(Event::ComponentRemoved(key,result));});
                    }
                    if ui.button("Cancel").clicked(){self.remove_component=None;}
                });
            });
        }
        if self.save_search_open {
            let mut open = true;
            egui::Window::new("Save search")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(340.0)
                .show(ctx, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.save_search_name);
                    ui.weak(&self.query);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.save_search_name.trim().is_empty()
                                    && !self.query.trim().is_empty(),
                                egui::Button::new("Save"),
                            )
                            .clicked()
                        {
                            let saved = crate::config::SavedSearch {
                                name: self.save_search_name.trim().into(),
                                query: self.query.clone(),
                                mode: self.mode.clone(),
                                category: self.category.clone(),
                                scope: self.scope.clone(),
                                excluded: self.exclude_types.clone(),
                            };
                            if let Some(existing) = self
                                .s
                                .saved_searches
                                .iter_mut()
                                .find(|v| v.name == saved.name)
                            {
                                *existing = saved;
                            } else {
                                self.s.saved_searches.push(saved);
                            }
                            self.save();
                            self.save_search_open = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.save_search_open = false;
                        }
                    });
                });
            self.save_search_open &= open;
        }
        if self.clipboard_clear_confirm {
            egui::Window::new("Clear clipboard history?").collapsible(false).resizable(false).show(ctx,|ui|{
                ui.label("Remove all saved clipboard entries? Your current Windows clipboard is kept.");
                ui.horizontal(|ui|{
                    if ui.button("Clear history").clicked(){self.clipboard_clear_confirm=false;let store=crate::launcher::Clipboard::new(&self.home);let tx=self.engine.tx.clone();std::thread::spawn(move||{let _=tx.send(Event::ClipboardCleared(store.clear().map_err(|e|e.to_string())));});}
                    if ui.button("Cancel").clicked(){self.clipboard_clear_confirm=false;}
                });
            });
        }
        if self.show_changelog {
            let mut open = true;
            egui::Window::new("Changes since your installed version")
                .id(egui::Id::new("update-changelog"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size([720.0, 520.0])
                .vscroll(true)
                .show(ctx, |ui| {
                    ui.label(format!("Installed: {}", env!("CARGO_PKG_VERSION")));
                    if let Some(release) = &self.update_release {
                        for note in &release.changes {
                            ui.separator();
                            ui.heading(&note.title);
                            ui.label(format!("Version {} · {}", note.version, note.date));
                            ui.label(&note.body);
                            ui.hyperlink_to("Open this release on GitHub", &note.page);
                        }
                    }
                });
            self.show_changelog = open;
        }
        if !self.large_preview {
            self.child_theme_key.clear();
            self.preview_ready_frame = false;
        }
        if self.large_preview {
            let monitor = ctx
                .input(|i| i.viewport().monitor_size)
                .unwrap_or(egui::vec2(1920.0, 1080.0));
            let selected = self.hits.get(self.selected);
            let image = self
                .texture
                .as_ref()
                .filter(|(path, _)| selected.is_some_and(|h| h.path == *path))
                .map(|(_, t)| t.size_vec2());
            let awaiting_image = selected.is_some_and(|h| {
                crate::content::is_image(&crate::content::extension(Path::new(&h.path)))
                    || h.path.to_lowercase().ends_with(".pdf")
            }) && image.is_none()
                && self.preview_error.is_none();
            let size = quick_look_size(image, monitor);
            let position = ctx.input(|i| {
                i.viewport()
                    .outer_rect
                    .map(|r| r.center() - size * 0.5)
                    .unwrap_or(egui::pos2(100.0, 100.0))
            });
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("vectordash-quick-look"),
                egui::ViewportBuilder::default()
                    .with_title("VectorDash — Quick Look")
                    .with_position(position)
                    .with_visible(self.preview_ready_frame && !awaiting_image)
                    .with_inner_size(size)
                    .with_min_inner_size([240.0, 180.0])
                    .with_window_level(egui::WindowLevel::AlwaysOnTop),
                |child, class| {
                    if child.input(|i| {
                        i.viewport().close_requested()
                            || i.key_pressed(egui::Key::Space)
                            || i.key_pressed(egui::Key::Escape)
                    }) {
                        self.large_preview = false;
                        self.popup_had_focus = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        child.send_viewport_cmd(egui::ViewportCommand::Close);
                        return;
                    }
                    if self.smoke.is_some() {
                        for event in child.input(|i| i.events.clone()) {
                            if let egui::Event::Screenshot { image, .. } = event {
                                self.qa_child_size = Some(image.size);
                                if let Some(path) = &self.qa_screenshot {
                                    let pixels: Vec<u8> =
                                        image.pixels.iter().flat_map(|p| p.to_array()).collect();
                                    if let Some(rgba) = image::RgbaImage::from_raw(
                                        image.width() as u32,
                                        image.height() as u32,
                                        pixels,
                                    ) {
                                        let _ = rgba.save(path.with_file_name("quick-look.png"));
                                    }
                                }
                            }
                        }
                        if !self.qa_child_capture_sent
                            && self.startup.elapsed() > Duration::from_secs(2)
                        {
                            self.qa_child_capture_sent = true;
                            child.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                                egui::UserData::default(),
                            ));
                        }
                    }
                    if class != egui::ViewportClass::Embedded {
                        self.qa_child_size = Some([
                            child.content_rect().width() as usize,
                            child.content_rect().height() as usize,
                        ]);
                    }
                    let key = format!(
                        "{}{:?}{}",
                        self.s.theme,
                        self.s.accent,
                        crate::appearance::dark(ctx, &self.s)
                    );
                    if self.child_theme_key != key {
                        crate::appearance::apply(child, &self.s);
                        self.child_theme_key = key;
                    }
                    if class == egui::ViewportClass::Embedded {
                        let mut open = true;
                        egui::Window::new("Quick Look")
                            .open(&mut open)
                            .fixed_size([900.0, 650.0])
                            .show(child, |ui| self.preview(ui, child));
                        if !open {
                            self.large_preview = false;
                        }
                    } else {
                        egui::CentralPanel::default()
                            .frame(
                                egui::Frame::new()
                                    .fill(child.style().visuals.panel_fill)
                                    .inner_margin(24.0),
                            )
                            .show(child, |ui| self.preview(ui, child));
                    }
                    if awaiting_image {
                        self.preview_ready_frame = false;
                        ctx.request_repaint_after(Duration::from_millis(30));
                    } else if !self.preview_ready_frame {
                        self.preview_ready_frame = true;
                        child.request_repaint();
                        ctx.request_repaint();
                    }
                },
            );
        }
        if let Some(hit) = self.delete_confirm.clone() {
            egui::Window::new("Delete selected item?")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(&hit.path);
                    ui.horizontal(|ui| {
                        if ui.button("Move to Recycle Bin").clicked() {
                            self.run(crate::platform::recycle(&hit.path));
                            self.delete_confirm = None;
                            self.changed();
                        }
                        if ui.button("Delete permanently").clicked() {
                            self.run(crate::platform::remove(&hit.path));
                            self.delete_confirm = None;
                            self.changed();
                        }
                        if ui.button("Cancel").clicked() {
                            self.delete_confirm = None;
                        }
                    });
                });
        }
        if self.uninstall_confirm {
            egui::Window::new("Uninstall downloaded models and runtimes?").collapsible(false).show(ctx,|ui|{
                ui.label("Remove managed downloads from the current and legacy VectorDash folders? Source files, external tools and the index are kept. Automatic indexing will pause.");
                if ui.button("Uninstall downloads").clicked(){self.uninstall_downloads();}
                if ui.button("Cancel").clicked(){self.uninstall_confirm=false;}
            });
        }
        if self.clear_index_confirm {
            egui::Window::new("Clear entire index?").collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label("Remove all indexed names, document/OCR text, embeddings and indexing history from the active database?");
                ui.label("Your original files, sources, settings and downloaded models are kept. Retained conversion/import backups are not deleted.");
                ui.label("Automatic indexing will pause. Remove unwanted sources or add exclusions before rebuilding.");
                ui.horizontal(|ui| {
                    if ui.button("Clear entire index").clicked() {
                        self.clear_index_confirm = false; self.index_clearing = true; self.setup_busy = true;
                        self.sequence += 1; self.hits.clear(); self.texture = None; self.large_preview = false;
                        self.status = "Stopping indexing and clearing the active index…".into();
                        self.engine.clear_index();
                    }
                    if ui.button("Cancel").clicked() { self.clear_index_confirm = false; }
                });
            });
        }
        if let Some(report) = &self.smoke {
            if self.startup.elapsed() > Duration::from_secs(self.smoke_seconds)
                && (self.qa_screenshot.is_none() || self.qa_captured)
            {
                let data = serde_json::json!({"ok":true,"native_rust":true,"tool":self.tool,"tools":self.saved_settings.search_tools(),"footer":self.footer_text(),"dictation_status":self.dictation_status,"query":self.query,"ai_busy":self.ai_busy,"ai_answer":self.ai_answer,"model_loaded":self.engine.models.loaded.load(Ordering::Relaxed),"frames":self.frames,"max_frame_interval_ms":self.max_frame_ms,"dark_mode":ctx.style().visuals.dark_mode,"hidden_reopened":self.qa_hidden_reopened,"ocr_enabled":self.s.ocr_enabled,"ocr_setup_selected":self.ocr_setup,"results":self.hits.len(),"selected":self.selected,"result_scroll_offset":self.result_scroll_offset,"scope":self.scope,"quick_look":self.large_preview,"quick_look_size":self.qa_child_size,"cpu":crate::scheduling::metrics().cpu,"gpu":crate::scheduling::metrics().gpu,"query_focus":ctx.memory(|m|m.has_focus(egui::Id::new("query"))),"ai_metrics":self.ai_metrics.label(),"tts_busy":self.tts_busy,"ai_scroll_offset":self.ai_scroll_offset,"qa_ai_scroll_before":self.qa_ai_scroll_before,"qa_ai_followup_sent":self.qa_ai_followup_sent,"tts_status":self.tts_status,"tts_notice":self.tts_notice,"tts_voices":self.tts_voices,"response_length":self.response_length,"toolbar_top":self.qa_toolbar_top,"filters_top":self.qa_filter_top,"query_selected":egui::text_edit::TextEditState::load(ctx,egui::Id::new("query")).and_then(|s|s.cursor.char_range()).map(|r|r.as_sorted_char_range().len()).unwrap_or(0)});
                let _ = std::fs::write(report, data.to_string());
                self.exiting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        let _ = self.window_state.save(&self.home);
        self.dictation_cancel.store(true, Ordering::Release);
        self.stop_ai();
        self.stop_read_aloud();
        self.shutdown.store(true, Ordering::Relaxed);
        self.scheduler_shutdown.store(true, Ordering::Relaxed);
        self.engine.control.cancel.store(true, Ordering::Relaxed);
        self.setup.cancel.store(true, Ordering::Relaxed);
        crate::platform::stop_playback();
        let _ = self.engine.models.stop();
        crate::dictation::stop();
        self.mcp = None;
    }
}
fn combo(ui: &mut egui::Ui, id: &str, value: &mut String, items: &[&str]) -> egui::Response {
    egui::ComboBox::from_id_salt(id)
        .selected_text(if *value == "Both" {
            "Hybrid"
        } else if *value == "Names" {
            "File/Folder Names"
        } else {
            value.as_str()
        })
        .show_ui(ui, |ui| {
            for item in items {
                ui.selectable_value(
                    value,
                    (*item).into(),
                    if *item == "Both" {
                        "Hybrid"
                    } else if *item == "Names" {
                        "File/Folder Names"
                    } else {
                        *item
                    },
                );
            }
        })
        .response
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(360.0));
    });
}
fn secret(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::TextEdit::singleline(value)
                .password(true)
                .desired_width(360.0),
        );
    });
}
fn path_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(360.0));
        if ui.small_button("Browse…").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                *value = path.to_string_lossy().into_owned();
            }
        }
    });
}
fn content_image(path: &str) -> bool {
    crate::content::is_image(&crate::content::extension(Path::new(path)))
}

fn tail_log(path: &Path) -> std::io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len().min(64000);
    file.seek(SeekFrom::End(-(length as i64)))?;
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(64000).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn selection_offset(index: usize, row: f32, offset: f32, height: f32) -> f32 {
    let top = index as f32 * row;
    if top < offset {
        top
    } else if top + row > offset + height {
        (top + row - height).max(0.0)
    } else {
        offset
    }
}
#[cfg(test)]
mod result_navigation_tests {
    #[test]
    fn arrows_reveal_offscreen_rows_without_resetting_visible_ones() {
        assert_eq!(super::selection_offset(20, 70.0, 0.0, 350.0), 1120.0);
        assert_eq!(super::selection_offset(2, 70.0, 1120.0, 350.0), 140.0);
        assert_eq!(super::selection_offset(3, 70.0, 140.0, 350.0), 140.0);
    }
}

#[cfg(test)]
mod tab_focus_tests {
    #[test]
    fn consuming_tab_does_not_move_focus_to_the_next_control() {
        use eframe::egui::{self, Context, Event, Id, Key, Modifiers, RawInput};
        let ctx = Context::default();
        let id = Id::new("query");
        let mut query = "dog".to_owned();
        let _ = ctx.run(RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .id(id)
                        .lock_focus(true),
                );
                r.request_focus();
                egui::ComboBox::from_id_salt("saved-searches-test")
                    .selected_text("Saved searches")
                    .show_ui(ui, |ui| {
                        let _ = ui.button("Saved");
                    });
                let _ = ui.button("Save search");
            });
        });
        let mut input = RawInput::default();
        input.events = vec![Event::Key {
            key: Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }];
        let _ = ctx.run(input, |ctx| {
            assert!(ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)));
            ctx.memory_mut(|m| {
                m.move_focus(egui::FocusDirection::None);
                m.request_focus(id);
            });
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .id(id)
                        .lock_focus(true),
                );
                egui::ComboBox::from_id_salt("saved-searches-test")
                    .selected_text("Saved searches")
                    .show_ui(ui, |ui| {
                        let _ = ui.button("Saved");
                    });
                let _ = ui.button("Save search");
            });
        });
        let mut input = RawInput::default();
        input.events = vec![Event::Text("x".into())];
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .id(id)
                        .lock_focus(true),
                );
                egui::ComboBox::from_id_salt("saved-searches-test")
                    .selected_text("Saved searches")
                    .show_ui(ui, |ui| {
                        let _ = ui.button("Saved");
                    });
                let _ = ui.button("Save search");
            });
        });
        assert!(query.contains('x'));
        assert!(ctx.memory(|m| m.has_focus(id)));
    }
}

const ACTIVITY_TRAIL_LENGTH: f32 = 0.34 * 1.5;
#[derive(Clone, Copy)]
enum ActivityTrail {
    Microphone,
    Ai,
}
fn trail_style(kind: ActivityTrail, behind: f32) -> Option<(Color32, f32)> {
    if !(0.0..ACTIVITY_TRAIL_LENGTH).contains(&behind) {
        return None;
    }
    let position = behind / ACTIVITY_TRAIL_LENGTH;
    let (head, tail, mix) = match kind {
        ActivityTrail::Microphone => (
            [255.0, 55.0, 75.0],
            [255.0, 180.0, 35.0],
            ((position - 0.5) * 2.0).clamp(0.0, 1.0),
        ),
        ActivityTrail::Ai => ([155.0, 85.0, 255.0], [55.0, 135.0, 255.0], position),
    };
    let color = Color32::from_rgb(
        (head[0] + (tail[0] - head[0]) * mix) as u8,
        (head[1] + (tail[1] - head[1]) * mix) as u8,
        (head[2] + (tail[2] - head[2]) * mix) as u8,
    );
    Some((color, (1.0 - position).powf(1.3)))
}
fn paint_ai_outline(ctx: &egui::Context) {
    paint_activity_outline(ctx, ActivityTrail::Ai);
}
fn paint_recording_outline(ctx: &egui::Context) {
    paint_activity_outline(ctx, ActivityTrail::Microphone);
}
fn paint_activity_outline(ctx: &egui::Context, kind: ActivityTrail) {
    let rect = ctx.content_rect().shrink(3.0);
    let radius = 17.0;
    let mut points = Vec::new();
    let corners = [
        (
            egui::pos2(rect.right() - radius, rect.top() + radius),
            -std::f32::consts::FRAC_PI_2,
        ),
        (
            egui::pos2(rect.right() - radius, rect.bottom() - radius),
            0.0,
        ),
        (
            egui::pos2(rect.left() + radius, rect.bottom() - radius),
            std::f32::consts::FRAC_PI_2,
        ),
        (
            egui::pos2(rect.left() + radius, rect.top() + radius),
            std::f32::consts::PI,
        ),
    ];
    for (i, (center, start)) in corners.iter().enumerate() {
        for n in 0..=16 {
            let a = start + n as f32 / 16.0 * std::f32::consts::FRAC_PI_2;
            points.push(*center + egui::vec2(a.cos(), a.sin()) * radius);
        }
        let a = start + std::f32::consts::FRAC_PI_2;
        let from = *center + egui::vec2(a.cos(), a.sin()) * radius;
        let (next, angle) = corners[(i + 1) % 4];
        let to = next + egui::vec2(angle.cos(), angle.sin()) * radius;
        for n in 1..=64 {
            points.push(from + (to - from) * (n as f32 / 64.0));
        }
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("activity-outline"),
    ));
    let head = (ctx.input(|i| i.time) * 0.20).fract() as f32;
    let lengths = (0..points.len())
        .map(|i| points[i].distance(points[(i + 1) % points.len()]))
        .collect::<Vec<_>>();
    let perimeter = lengths.iter().sum::<f32>().max(1.0);
    let mut distance = 0.0;
    for i in 0..points.len() {
        let phase = distance / perimeter;
        distance += lengths[i];
        if let Some((color, intensity)) = trail_style(kind, (head - phase).rem_euclid(1.0)) {
            let segment = [points[i], points[(i + 1) % points.len()]];
            for (width, alpha) in [(12.0_f32, 0.045_f32), (7.0, 0.10), (4.0, 0.20), (2.2, 1.0)] {
                painter.line_segment(
                    segment,
                    egui::Stroke::new(
                        width,
                        Color32::from_rgba_unmultiplied(
                            color.r(),
                            color.g(),
                            color.b(),
                            (alpha * intensity * 255.0) as u8,
                        ),
                    ),
                );
            }
        }
    }
    ctx.request_repaint_after(Duration::from_millis(16));
}
#[cfg(test)]
mod activity_trail_tests {
    use super::*;
    #[test]
    fn both_trails_have_the_same_fifty_percent_longer_span_and_distinct_gradients() {
        assert!((ACTIVITY_TRAIL_LENGTH / 0.34 - 1.5).abs() < 0.0001);
        for kind in [ActivityTrail::Microphone, ActivityTrail::Ai] {
            assert!(trail_style(kind, 0.50).is_some());
            assert!(trail_style(kind, 0.52).is_none());
            assert!(trail_style(kind, 0.0).unwrap().1 > trail_style(kind, 0.4).unwrap().1);
        }
        let red = trail_style(ActivityTrail::Microphone, 0.0).unwrap().0;
        assert_eq!(red, trail_style(ActivityTrail::Microphone, 0.2).unwrap().0);
        let orange = trail_style(ActivityTrail::Microphone, 0.48).unwrap().0;
        assert!(orange.g() > red.g() + 90);
        let purple = trail_style(ActivityTrail::Ai, 0.0).unwrap().0;
        let blue = trail_style(ActivityTrail::Ai, 0.48).unwrap().0;
        assert!(purple.r() > blue.r() && purple.g() < blue.g());
    }
}

fn image_key(value: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hash);
    hash.finish()
}
fn image_bytes(value: &str) -> anyhow::Result<Vec<u8>> {
    use base64::Engine;
    anyhow::ensure!(value.len() <= 3 * 1024 * 1024, "Screenshot exceeds limit");
    Ok(base64::engine::general_purpose::STANDARD.decode(
        value
            .strip_prefix("data:image/jpeg;base64,")
            .ok_or_else(|| anyhow::anyhow!("Invalid screenshot"))?,
    )?)
}
fn copy_ai_image(value: &str) -> anyhow::Result<()> {
    let image = crate::capture::rgba(&image_bytes(value)?)?;
    arboard::Clipboard::new()?.set_image(arboard::ImageData {
        width: image.width() as usize,
        height: image.height() as usize,
        bytes: std::borrow::Cow::Owned(image.into_raw()),
    })?;
    Ok(())
}

fn quick_look_size(image: Option<egui::Vec2>, monitor: egui::Vec2) -> egui::Vec2 {
    let bounds = egui::vec2(
        (monitor.x - 100.0).max(240.0),
        (monitor.y - 130.0).max(180.0),
    );
    let picture = image.unwrap_or(egui::vec2(640.0, 400.0));
    let scale = ((bounds.x - 48.0) / picture.x.max(1.0))
        .min((bounds.y - 153.0) / picture.y.max(1.0))
        .min(1.0);
    egui::vec2(
        (picture.x * scale + 48.0).max(300.0),
        (picture.y * scale + 153.0).max(240.0),
    )
}
#[cfg(test)]
mod quick_look_tests {
    #[test]
    fn fits_image_plus_metadata() {
        let size = super::quick_look_size(
            Some(eframe::egui::vec2(800.0, 600.0)),
            eframe::egui::vec2(1920.0, 1080.0),
        );
        assert_eq!(size, eframe::egui::vec2(848.0, 753.0));
        let portrait = super::quick_look_size(
            Some(eframe::egui::vec2(1000.0, 2000.0)),
            eframe::egui::vec2(1920.0, 1080.0),
        );
        assert!(portrait.x < 500.0 && portrait.y <= 950.0);
    }
}
