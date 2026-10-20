mod image_search;
mod manager;
mod web;
use crate::i18n::{t, tr};
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
const PAGES: [&str; 10] = [
    "Search",
    "Sources",
    "Hardware",
    "Scheduling",
    "Settings",
    "Models and AI",
    "Logs",
    "About",
    "Web",
    "Clipboard",
];
const CATEGORIES: [&str; 8] = [
    "All files",
    "Images",
    "Videos",
    "Documents",
    "Audio / video",
    "Folders",
    "Code",
    "Programs",
];
#[derive(Default)]
struct RemoteState {
    signature: String,
    models: Vec<String>,
    note: String,
    error: bool,
    busy: bool,
    vision: Option<bool>,
    dimensions: Option<usize>,
    context: Option<u32>,
}
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
    image_query: Option<(Arc<Vec<u8>>, egui::TextureHandle)>,
    image_previous: Option<(String, String, String)>,
    image_generation: Arc<std::sync::atomic::AtomicU64>,
    image_preparing: bool,
    image_search_requested: bool,
    image_search_submitted: bool,
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
    search_inflight: Option<u64>,
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
    update_generation: u64,
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
    grid_columns: usize,
    child_theme_key: String,
    search_visible: bool,
    model_warm_attempted: bool,
    model_warm_busy: bool,
    ai_session_model: String,
    model_warm_error: Option<String>,
    last_search_mode: String,
    popup_panel_height: u32,
    clear_index_confirm: bool,
    index_clearing: bool,
    tool: String,
    tool_items: Vec<crate::launcher::Item>,
    web: crate::web_search::Session,
    web_thumbs: HashMap<String, Option<egui::TextureHandle>>,
    web_thumb_pending: std::collections::HashSet<String>,
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
    ai_message_thumbs: HashMap<u64, egui::TextureHandle>,
    capture_return_visible: bool,
    ai_web_enabled: bool,
    ai_local_enabled: bool,
    ai_capture_restore: Option<(bool, String)>,
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
    ai_started: Option<Instant>,
    ai_elapsed: Option<f64>,
    ai_image_view: Option<String>,
    tts_models: Vec<String>,
    tts_voices: Vec<String>,
    tts_catalog_busy: bool,
    tts_notice: String,
    tts_voice_job: u64,
    tts_voice_busy: bool,
    tts_voice_due: Option<Instant>,
    response_length: String,
    shortcut_capture: Option<String>,
    shortcut_conflict: Option<(String, String, String)>,
    clip_edit: Option<(i64, String, String, String)>,
    clip_usage: Option<(u64, u64, u64)>,
    clip_filter: String,
    clip_selected: std::collections::BTreeSet<i64>,
    qa_web_events: Vec<egui::Event>,
    qa_toolbar_top: f32,
    qa_filter_top: Option<f32>,
    tts_status: String,
    tts_busy: bool,
    tts_job: u64,
    tts_cancel: Arc<AtomicBool>,
    tts_paused: Arc<AtomicBool>,
    status_clocks: [Option<(String, Instant)>; 6],
    connections: [ConnectionState; 2],
    remote_servers: [RemoteState; 4],
    hardware_open: bool,
    models_ai_open: bool,
    remote_due: [Option<Instant>; 4],
    type_hotkey_capture: bool,
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
        crate::platform::set_capture_allowed(s.ai_chat_available());
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
        crate::i18n::set_language(&s.language);
        crate::appearance::apply(&cc.egui_ctx, &s);
        crate::i18n::apply(&cc.egui_ctx, &s.language);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let saved_start_hidden = s.start_hidden;
        let window_state = crate::config::WindowState::load(&home);
        let read_aloud = s.tts_enabled && s.tts_auto;
        let response_length = s.llm_response.clone();
        let ai_session_model = s.llm_model.clone();
        let llm_models = load_chat_models(&home, &s.llm_url);
        let mut app = Self {
            home,
            collapsed: s.sidebar_collapsed,
            mode: s.default_search_mode.clone(),
            exclude_types: s.search_excluded_types.clone(),
            runtime_setup: s.setup_runtime,
            model_setup: s.setup_model,
            encoder_setup: s.setup_encoder,
            ocr_setup: s.setup_ocr,
            ai_local_enabled: false,
            ai_web_enabled: s.ai_web_default && s.web.available(),
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
            image_query: None,
            image_previous: None,
            image_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            image_preparing: false,
            image_search_requested: false,
            image_search_submitted: false,
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
            search_inflight: None,
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
            update_generation: 0,
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
            grid_columns: 1,
            child_theme_key: String::new(),
            search_visible: !(std::env::args().any(|a| a == "--startup") && saved_start_hidden),
            model_warm_attempted: false,
            model_warm_busy: false,
            ai_session_model,
            model_warm_error: None,
            last_search_mode: String::new(),
            popup_panel_height: 0,
            clear_index_confirm: false,
            index_clearing: false,
            tool: "Search".into(),
            tool_items: vec![],
            web: crate::web_search::Session::default(),
            web_thumbs: HashMap::new(),
            web_thumb_pending: Default::default(),
            ai_answer: String::new(),
            ai_question: String::new(),
            ai_error: None,
            ai_busy: false,
            ai_job: 0,
            ai_cancel: Arc::new(AtomicBool::new(false)),
            llm_models,
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
            ai_message_thumbs: HashMap::new(),
            capture_return_visible: false,
            ai_capture_restore: None,
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
            ai_started: None,
            ai_elapsed: None,
            ai_image_view: None,
            tts_models: vec![],
            tts_voices: vec![],
            tts_catalog_busy: false,
            tts_notice: String::new(),
            tts_voice_job: 0,
            tts_voice_busy: false,
            tts_voice_due: None,
            response_length,
            shortcut_capture: None,
            shortcut_conflict: None,
            clip_edit: None,
            clip_usage: None,
            clip_filter: "All".into(),
            clip_selected: Default::default(),
            qa_web_events: vec![],
            qa_toolbar_top: 0.0,
            qa_filter_top: None,
            tts_status: String::new(),
            tts_busy: false,
            tts_job: 0,
            tts_cancel: Arc::new(AtomicBool::new(false)),
            tts_paused: Arc::new(AtomicBool::new(false)),
            status_clocks: [None, None, None, None, None, None],
            connections: std::array::from_fn(|_| ConnectionState::default()),
            remote_servers: std::array::from_fn(|_| RemoteState::default()),
            hardware_open: false,
            models_ai_open: false,
            remote_due: [None; 4],
            type_hotkey_capture: false,
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
            app.qa_hidden = args.iter().any(|a| {
                a == "--smoke-capture-hidden"
                    || a == "--smoke-hidden"
                    || a == "--smoke-tray"
                    || a == "--smoke-tray-saturated"
            });
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
            if let Some(mode) = get("--smoke-mode") {
                app.mode = mode;
            }
            if args.iter().any(|a| a == "--smoke-indexing") {
                app.indexing = true;
            }
            if args.iter().any(|a| a == "--smoke-transcribing") {
                app.dictation_busy = true;
                app.dictation_status = "Transcribing…".into();
                app.query.clear();
            }
            if let Some(status) = get("--smoke-status") {
                app.status = status;
            }
            if let Some(style) = get("--smoke-response") {
                app.response_length = style;
            }
            if std::env::args().any(|a| a == "--smoke-voice-closed") {
                app.search_visible = false;
            }
            if std::env::args().any(|a| a == "--smoke-remote-catalog") {
                app.refresh_remote(0, false);
                app.refresh_remote(1, false);
            }
            if std::env::args().any(|a| a == "--smoke-test-connections") {
                app.start_connection_test(0);
                app.start_connection_test(1);
            }
            if std::env::args().any(|a| a == "--smoke-tts-voices") {
                app.tts_voice_due = Some(Instant::now());
            }
            if args.iter().any(|a| a == "--smoke-filter") {
                app.filter_open = true;
                app.filter_anchor = egui::pos2(600.0, 110.0);
            }
            if let Some(category) = get("--smoke-category") {
                app.category = category;
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
            if args.iter().any(|a| a == "--smoke-tts-busy") {
                app.tts_busy = true;
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
            let args = std::env::args().collect::<Vec<_>>();
            if let Some(kind) = args
                .iter()
                .position(|v| v == "--smoke-dialog")
                .and_then(|i| args.get(i + 1))
            {
                match kind.as_str() {
                    "capture" => {
                        app.tool = "AI".into();
                        app.capture_open = true;
                        app.capture_targets = (0..30)
                            .map(|id| crate::capture::Target {
                                id,
                                window: true,
                                label: format!(
                                    "Window {id}: {}",
                                    "Long application title ".repeat(12)
                                ),
                            })
                            .collect();
                    }
                    "save" => {
                        app.save_search_open = true;
                        app.query = "Search query ".repeat(50);
                    }
                    "clipboard-clear" => app.clipboard_clear_confirm = true,
                    "delete" => {
                        app.delete_confirm = Some(Hit {
                            path: "C:\\fixture\\selected-file.txt".into(),
                            kind: "file".into(),
                            text: String::new(),
                            score: 0.0,
                            reason: String::new(),
                            page: 0,
                            seconds: 0.0,
                            line: 0,
                        })
                    }
                    "component" => app.remove_component = Some("dictation".into()),
                    "uninstall" => app.uninstall_confirm = true,
                    "clear-index" => app.clear_index_confirm = true,
                    "changelog" => {
                        app.show_changelog = true;
                        app.update_release = Some(crate::updates::Release {
                            version: "fixture".into(),
                            page: String::new(),
                            url: String::new(),
                            digest: String::new(),
                            size: 0,
                            name: String::new(),
                            changes: vec![crate::updates::ReleaseNote {
                                version: "fixture".into(),
                                date: String::new(),
                                title: "Fixture changes".into(),
                                body: "A release note with enough text to require scrolling.\n"
                                    .repeat(80),
                                page: String::new(),
                            }],
                        });
                    }
                    "sent-image" => {
                        app.tool = "AI".into();
                    }
                    _ => {}
                }
                if let Some(path) = args
                    .iter()
                    .position(|v| v == "--smoke-dialog-image")
                    .and_then(|i| args.get(i + 1))
                {
                    let bytes = std::fs::read(path)?;
                    if kind == "sent-image" {
                        app.ai_image_view = Some(crate::capture::vision(&bytes, None)?);
                    } else if kind == "capture" {
                        let image = image::load_from_memory(&bytes)?.to_rgba8();
                        app.capture_texture = Some(cc.egui_ctx.load_texture(
                            "fixture-capture",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                image.as_raw(),
                            ),
                            Default::default(),
                        ));
                        app.capture_bytes = Some(bytes);
                    }
                }
            }
        }
        app.web.provider = app
            .saved_settings
            .web
            .selected(&app.saved_settings.web.provider);
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
        if self.saved_settings.cache_precision != self.s.cache_precision {
            self.engine.vectors.clear();
        }
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
                if self.saved_settings.update_channel != self.s.update_channel {
                    self.update_generation += 1;
                    self.update_busy = false;
                    self.update_release = None;
                    self.update_installer = None;
                    self.update_status =
                        "Update channel saved. Check for updates to refresh.".into();
                }
                if self.s.web != self.saved_settings.web {
                    self.web.clear();
                    self.web.provider = self.s.web.selected(&self.s.web.provider);
                }
                self.saved_settings = self.s.clone();
                crate::platform::set_capture_allowed(self.saved_settings.ai_chat_available());
                self.read_aloud = self.s.tts_enabled && self.s.tts_auto;
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
        crate::resources::invalidate_storage();
        let previous = self.saved_settings.clone();
        if previous.remote_embeddings() != self.s.remote_embeddings() {
            let models = self.engine.models.clone();
            std::thread::spawn(move || {
                let _ = models.stop();
            });
        }
        if previous.remote_dictation() != self.s.remote_dictation() {
            std::thread::spawn(crate::dictation::stop);
        }
        if !self.s.tts_enabled {
            self.stop_read_aloud();
            self.read_aloud = false;
        }
        let startup_changed = previous.start_with_windows != self.s.start_with_windows;
        let hotkeys_changed = previous.forced_shortcuts != self.s.forced_shortcuts
            || previous.extra_shortcuts != self.s.extra_shortcuts
            || (
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
        if !self.s.image_search_enabled {
            self.clear_image_query();
        }
        if previous.ai_web_default != self.s.ai_web_default || previous.web != self.s.web {
            self.restore_capture_defaults();
            self.ai_web_enabled = self.s.ai_web_default && self.s.web.available();
        }
        if self.ai_session_model == previous.llm_model {
            self.ai_session_model = self.s.llm_model.clone();
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
        let mut events = self
            .engine
            .tx
            .take_full_request()
            .into_iter()
            .collect::<Vec<_>>();
        events.extend(self.rx.try_iter().take(100));
        for event in events {
            let event = if let Event::Shortcut(target, action) = event {
                if self.shortcut_capture.is_some() {
                    let value = self.saved_settings.binding(&action).to_string();
                    self.capture_binding(value);
                    continue;
                }
                match action.as_str() {
                    "dictate" => Event::VoiceSummon(target),
                    "search" => Event::Summon(target, String::new()),
                    "clipboard" | "ai" | "web" | "emoji" | "capture_ai" => {
                        let mode = match action.as_str() {
                            "clipboard" => "Clipboard",
                            "web" => "Web",
                            "emoji" => "Emoji",
                            _ => "AI",
                        };
                        if !self.saved_settings.tool_available(mode) {
                            self.status = "This search mode is disabled in settings".into();
                            continue;
                        }
                        Event::ModeSummon(target, mode.into(), action == "capture_ai")
                    }
                    "pause_clipboard" => {
                        crate::launcher::pause(if crate::launcher::paused() {
                            0
                        } else {
                            self.saved_settings.clipboard_options.pause_minutes
                        });
                        self.status = "Clipboard capture pause changed".into();
                        continue;
                    }
                    "ignore_copy" => {
                        crate::launcher::ignore_next();
                        self.status = "The next copy will not be saved".into();
                        continue;
                    }
                    "stop_voice" => {
                        self.stop_read_aloud();
                        continue;
                    }
                    _ => continue,
                }
            } else {
                event
            };
            let requested_mode = if let Event::ModeSummon(_, mode, area) = &event {
                Some((mode.clone(), *area))
            } else {
                None
            };
            if let Event::ModeSummon(target, _, true) = &event {
                self.target = *target;
                self.start_area_capture(ctx);
                continue;
            }
            let event = if let Event::ModeSummon(target, _, _) = event {
                Event::Summon(target, String::new())
            } else {
                event
            };
            let voice_summon = matches!(&event, Event::VoiceSummon(..));
            if voice_summon
                && self.page == 0
                && self.search_visible
                && crate::platform::main_window_visible()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                if self.tool == "AI" {
                    self.read_aloud =
                        self.saved_settings.tts_enabled && self.saved_settings.voice_read_aloud;
                    self.response_length = self.saved_settings.voice_response.clone();
                }
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
                Event::VoiceSummon(_) | Event::Shortcut(..) | Event::ModeSummon(..) => {}
                Event::RemoteCatalog(index, signature, result) => {
                    if signature == self.remote_signature(index) {
                        let state = &mut self.remote_servers[index];
                        state.signature = signature;
                        state.busy = false;
                        match result {
                            Ok(catalog) => {
                                state.vision = catalog.vision;
                                state.dimensions = catalog.dimensions;
                                state.context = catalog.context;
                                if index == 0 && self.s.remote_context_auto {
                                    if let Some(context) = catalog.context {
                                        self.s.remote_context = context;
                                    }
                                }
                                if index == 2 {
                                    cache_chat_models(&self.home, &self.s.llm_url, &catalog.models);
                                    self.llm_models = catalog.models.clone();
                                    self.llm_models_busy = false;
                                    if self.s.llm_context_auto {
                                        if let Some(context) = catalog.context {
                                            self.s.llm_context = context;
                                        }
                                    }
                                }
                                state.models = catalog.models;
                                state.note = catalog.note;
                                state.error = false;
                            }
                            Err(error) => {
                                state.note = error;
                                state.vision = None;
                                state.dimensions = None;
                                state.context = None;
                                state.error = true;
                                if index == 2 {
                                    self.llm_models_busy = false;
                                }
                            }
                        }
                    }
                }
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
                            if [
                                "all",
                                "models",
                                "runtimes",
                                "embedding-models",
                                "dictation-model",
                            ]
                            .contains(&key.as_str())
                            {
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
                                self.dictation_status = if self.saved_settings.remote_dictation() { "Dictation complete · Remote".into() } else if self.saved_settings.dictation_keep_loaded
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
                        self.resume_read_aloud();
                    }
                }
                Event::DictationInstalled(result) => {
                    crate::resources::invalidate_storage();
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
                                    let image = image::DynamicImage::ImageRgba8(image);
                                    let thumb = image.thumbnail(112, 112).to_rgba8();
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
                                        let image = image.thumbnail(1280, 1280).to_rgba8();
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
                Event::GlobalAreaCapture(target, restore, result) => {
                    self.target = target;
                    self.capture_return_visible = restore;
                    self.finish_area_capture(ctx, result);
                }
                Event::AreaCapture(result) => {
                    self.finish_area_capture(ctx, result);
                }
                Event::AiAttachmentReady(id, result) => {
                    if id == self.image_generation.load(Ordering::Acquire)
                        && self.page == 0
                        && self.tool == "AI"
                    {
                        self.status.clear();
                        match result.and_then(|bytes| {
                            self.attach_ai_bytes(ctx, &bytes).map_err(|e| e.to_string())
                        }) {
                            Ok(()) => ctx.memory_mut(|m| m.request_focus(egui::Id::new("query"))),
                            Err(error) => self.ai_error = Some(error),
                        }
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
                Event::ConnectionTest(index, job, result) => {
                    if let Some(state) = self.connections.get_mut(index) {
                        if state.job == job {
                            state.busy = false;
                            state.result = Some(result);
                            state.at = Some(Instant::now());
                            ctx.request_repaint_after(Duration::from_secs(3));
                        }
                    }
                }
                Event::WebThumbnail(url, bytes) => {
                    if self.web_thumb_pending.remove(&url)
                        && self
                            .web
                            .hits
                            .iter()
                            .any(|h| h.thumbnail.as_ref() == Some(&url))
                        && self.search_visible
                    {
                        let texture = bytes.and_then(|b| crate::capture::rgba(&b).ok()).map(|p| {
                            ctx.load_texture(
                                format!("web-thumb-{url}"),
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [p.width() as usize, p.height() as usize],
                                    p.as_raw(),
                                ),
                                egui::TextureOptions::LINEAR,
                            )
                        });
                        self.web_thumbs.insert(url, texture);
                    }
                }
                Event::WebDone(id, result, ms) => {
                    if self.web.finish(id, result, ms) {
                        self.popup_size_state = None;
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
                        self.ai_elapsed = self
                            .ai_started
                            .take()
                            .map(|start| start.elapsed().as_secs_f64());
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
                                if self.s.tts_enabled
                                    && self.saved_settings.tts_enabled
                                    && self.read_aloud
                                    && !self.ai_answer.is_empty()
                                    && self.search_visible
                                    && crate::platform::main_window_visible()
                                    && !ctx.input(|i| i.viewport().minimized.unwrap_or(false))
                                {
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
                                cache_chat_models(&self.home, &url, &models);
                                self.llm_models = models;
                            }
                            self.status = format!("{} LLM models available", self.llm_models.len());
                        }
                        Err(e) => self.status = e,
                    }
                }
                Event::Launcher(id, result) => {
                    if self.search_inflight == Some(id) {
                        self.searching = false;
                        self.search_inflight = None;
                    }
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
                Event::ClipboardUsage(stats) => {
                    self.clip_usage = Some(stats);
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
                    self.model_warm_busy = false;
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
                Event::Update(generation, result) => {
                    if generation != self.update_generation {
                        continue;
                    }
                    self.update_busy = false;
                    match result {
                        crate::updates::Update::Checked(Ok(Some(release))) => {
                            self.update_status = format!("Version {} available", release.version);
                            self.update_release = Some(release);
                            if self.saved_settings.auto_update && !crate::updates::portable() {
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
                            if self.thumbs.len() >= 96 {
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
                    if self.qa_hidden {
                        self.qa_hidden_reopened = true;
                    }
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
                Event::OpenDone(result) => {
                    if let Err(error) = result {
                        self.status = error.clone();
                        self.engine
                            .log("Error", format!("Open selected item failed: {error}"));
                        if self.quick {
                            self.search_visible = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        }
                    }
                }
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
                Event::SearchPartial(seq, hits, ms) => {
                    if seq == self.sequence {
                        self.hits = hits;
                        self.texture = None;
                        self.preview_pending = None;
                        self.preview_error = None;
                        self.search_ms = ms;
                        self.selected = 0;
                        self.result_scroll_offset = 0.0;
                        self.scroll_to_selection = true;
                    }
                }
                Event::SearchWarning(seq, message) => {
                    if seq == self.sequence {
                        self.search_error = Some(message);
                    }
                }
                Event::Search(seq, result, ms) => {
                    if self.search_inflight == Some(seq) {
                        self.searching = false;
                        self.search_inflight = None;
                    }
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

                                self.search_ms = ms;
                                self.status = format!("{} results · {ms:.1} ms", self.hits.len());
                            }
                            Err(e) => {
                                self.search_error = Some(e.clone());
                                self.status = e;
                            }
                        }
                    }
                    if self.search_inflight.is_none() && self.search_pending {
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
                    crate::resources::invalidate_storage();
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
                    self.clear_image_query();
                    // Wake/display changes can relocate a hidden HWND; restore once on summon.
                    self.position_mode = None;
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
                    self.changed();
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
                    if let Some((mode, area)) = requested_mode {
                        self.switch_tool(&mode);
                        if area {
                            self.start_area_capture(ctx);
                        }
                    }
                    if voice_summon {
                        self.tool = crate::ai::voice_tool(&self.saved_settings).into();
                        self.read_aloud =
                            self.saved_settings.tts_enabled && self.saved_settings.voice_read_aloud;
                        self.response_length = self.saved_settings.voice_response.clone();
                        self.toggle_dictation();
                    }
                }
                Event::ImageQueryReady(id, result) => {
                    if id == self.image_generation.load(Ordering::Acquire) {
                        self.finish_image_input(ctx, result);
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
        if !self.search_visible
            || self.page != 0
            || self.mode == "Names"
            || self.tool != "Search"
            || (!self.query.trim().is_empty() && !self.query_uses_embeddings())
            || (!self.scope.is_empty() && self.query.trim().is_empty())
        {
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
            || self.model_warm_busy
            || !self.s.warm_on_search_open(&self.mode)
            || self.engine.models.search_hold(&self.s).is_some()
            || self.engine.models.loading.load(Ordering::Acquire)
            || self.index_clearing
        {
            return;
        }
        if !self.s.remote_embeddings()
            && (!Path::new(&self.s.server).is_file() || !Path::new(&self.s.model).is_file())
        {
            return;
        }
        self.model_warm_attempted = true;
        self.model_warm_busy = true;
        let models = self.engine.models.clone();
        let s = self.s.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let result = models.warm_search(&s).map_err(|e| e.to_string());
            let _ = tx.send(Event::ModelReady(result));
        });
    }
    fn dispatch(&mut self) {
        if self.index_clearing || !self.search_visible || self.page != 0 {
            return;
        }
        if self.image_preparing || self.image_query.is_some() {
            if self.image_search_requested && !self.image_preparing {
                self.dispatch_image_search();
            }
            return;
        }
        if self.tool == "AI" || self.tool == "Web" {
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
            let q = if self.clip_filter == "All" {
                self.query.clone()
            } else {
                format!(
                    "{}:{}",
                    if self.clip_filter == "Pinned items" {
                        "pinned".to_string()
                    } else {
                        self.clip_filter.to_lowercase()
                    },
                    self.query
                )
            };
            let settings = self.saved_settings.clone();
            let models = self.engine.models.clone();
            let id = self.sequence;
            self.search_inflight = Some(id);
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let result = store
                    .search_with_images(&q, &settings, &models)
                    .map_err(|e| e.to_string());
                let _ = tx.send(Event::Launcher(id, result));
            });
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
        if self.searching && self.search_inflight == Some(self.sequence) {
            return;
        }
        self.search_pending = false;
        self.searching = true;
        self.search_inflight = Some(self.sequence);
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
    fn reset_result_selection(&mut self) {
        self.selected = 0;
        self.result_scroll_offset = 0.0;
        self.scroll_to_selection = true;
        self.texture = None;
        self.preview_pending = None;
        self.preview_error = None;
    }
    fn changed(&mut self) {
        if self.image_query.is_some() && self.image_search_submitted {
            self.image_search_requested = true;
        }

        if self.tool == "Web" {
            self.web.edited(&self.query);
        }
        self.reset_result_selection();
        self.engine.models.cancel_query();
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
        if self.s.update_channel != self.saved_settings.update_channel {
            return;
        }
        self.update_busy = true;
        self.update_generation += 1;
        let generation = self.update_generation;
        let channel = self.saved_settings.update_channel.clone();
        self.update_status = "Checking GitHub releases…".into();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Update(
                generation,
                crate::updates::Update::Checked(
                    crate::updates::check(&channel).map_err(|e| e.to_string()),
                ),
            ));
        });
    }
    fn download_update(&mut self) {
        if self.update_busy {
            return;
        }
        if let Some(release) = self.update_release.clone() {
            self.update_busy = true;
            self.update_generation += 1;
            let generation = self.update_generation;
            self.update_status = "Downloading and verifying update…".into();
            let home = self.home.clone();
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Event::Update(
                    generation,
                    crate::updates::Update::Downloaded(
                        crate::updates::download(&release, &home).map_err(|e| e.to_string()),
                    ),
                ));
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
        self.clear_image_query();
        self.web.stop();
        self.stop_read_aloud();
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
        self.engine.models.cancel_query();
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
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(20, 7)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.logo).fit_to_exact_size(egui::vec2(28.0, 28.0)));
                    let response = ui.add(
                        egui::Label::new(
                            RichText::new(t("VectorDash"))
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
                                t("Expand sidebar")
                            } else {
                                t("Collapse sidebar")
                            })
                            .clicked()
                    {
                        self.collapsed = !self.collapsed;
                    }
                    if !self.quick
                        && self.page != 0
                        && ui
                            .button(tr(&format!("{} · Ctrl+F", t("Back to search"))))
                            .clicked()
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
                            .on_hover_text(t("Close"))
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
                                .on_hover_text(t("Maximize / restore"))
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
                            .on_hover_text(t("Minimize"))
                            .clicked()
                        {
                            self.stop_read_aloud();
                            ctx.send_viewport_cmd(if self.s.minimize_behavior == "Tray" {
                                egui::ViewportCommand::Visible(false)
                            } else {
                                egui::ViewportCommand::Minimized(true)
                            });
                        }
                        if self.quick && ui.button(t("Open full app")).clicked() {
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
            ui.heading(t("What are you looking for?"));
            ui.label(t("Find files, meanings, programs and Windows settings."));
            ui.add_space(8.0);
        }
        let mut query = self.query.clone();
        let recording = crate::dictation::recording()
            || self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-recording");
        if recording {
            query.clear();
        }
        ui.horizontal(|ui| {
            let (search_icon_rect, drag) =
                ui.allocate_exact_size(egui::vec2(26.0, 42.0), egui::Sense::drag());
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
                    .frame(false)
                    .background_color(Color32::TRANSPARENT)
                    .margin(egui::Margin {
                        right: if self.tool == "Search" {
                            8
                        } else if self.tool == "AI" || self.tool == "Web" {
                            127
                        } else {
                            8
                        },
                        ..egui::Margin::symmetric(8, 8)
                    })
                    .lock_focus(true)
                    .interactive(
                        !self.dictation_busy
                            && !recording
                            && !self.image_preparing
                            && self.image_query.is_none(),
                    )
                    .text_color(if recording {
                        Color32::TRANSPARENT
                    } else {
                        ui.visuals().text_color()
                    })
                    .hint_text(t(if recording {
                        ""
                    } else if self.image_query.is_some() || self.image_preparing {
                        "Search by image"
                    } else if self.dictation_busy {
                        "Transcribing…"
                    } else {
                        match self.tool.as_str() {
                            "AI" => "Ask AI",
                            "Web" => "Search the web",
                            "Emoji" => "Search emoji",
                            "Clipboard" => "Search clipboard history",
                            _ => "Search Anything",
                        }
                    }))
                    .font(egui::TextStyle::Heading)
                    .id(egui::Id::new("query")),
            );
            crate::icons::image(crate::icons::Icon::Search, self.accent(), 26.0).paint_at(
                ui,
                egui::Rect::from_center_size(
                    egui::pos2(search_icon_rect.center().x, response.rect.center().y),
                    egui::vec2(26.0, 26.0),
                ),
            );
            if recording {
                self.paint_waveform(ui, response.rect);
            }
            if self.tool == "AI" && !recording {
                let label_rect = egui::Rect::from_min_max(
                    egui::pos2(
                        (response.rect.right() - 125.0).max(response.rect.left()),
                        response.rect.top(),
                    ),
                    response.rect.right_bottom() - egui::vec2(8.0, 0.0),
                );
                let models = chat_model_choices(&self.ai_session_model, &self.llm_models);
                ui.scope_builder(egui::UiBuilder::new().max_rect(label_rect), |ui| {
                    egui::ComboBox::from_id_salt("chat-model-picker")
                        .truncate()
                        .width(label_rect.width())
                        .selected_text(RichText::new(&self.ai_session_model).size(11.0))
                        .show_ui(ui, |ui| {
                            for model in models {
                                ui.selectable_value(
                                    &mut self.ai_session_model,
                                    model.clone(),
                                    RichText::new(model).size(11.0),
                                );
                            }
                        });
                });
            }
            if self.tool == "Web" && !recording {
                self.web_provider_picker(
                    ui,
                    egui::Rect::from_min_max(
                        egui::pos2(
                            (response.rect.right() - 125.0).max(response.rect.left()),
                            response.rect.top(),
                        ),
                        response.rect.right_bottom() - egui::vec2(8.0, 0.0),
                    ),
                );
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
                    ui.button(t("Screenshot ×"))
                };
                chip.clone().on_hover_ui(|ui| {
                    if let Some(texture) = &self.ai_attachment {
                        ui.add(egui::Image::new(texture).max_size(egui::vec2(240.0, 160.0)));
                    }
                    ui.label(t("Screenshot attached. Click to remove."));
                });
                if chip.clicked() {
                    self.restore_capture_defaults();
                    self.ai_images.clear();
                    self.ai_attachment = None;
                }
            }
            if self.tool == "Search"
                && !self.query.trim().is_empty()
                && ui.button(t("Save Search")).clicked()
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
                .on_hover_text(t("Emoji"))
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
                    t("Finish dictation")
                } else {
                    t("Dictate · shortcut configurable in Settings")
                })
                .clicked()
            {
                self.toggle_dictation();
            }
        });
        // Span the entire search pane, including its padding, like the reference.
        let edge = if self.quick {
            ctx.content_rect().left()
        } else {
            ui.max_rect().left() - 20.0
        };
        let y = ui.cursor().min.y - 4.0;
        ui.painter().with_clip_rect(ctx.content_rect()).hline(
            edge..=ctx.content_rect().right(),
            y,
            egui::Stroke::new(
                1.0,
                if ui.visuals().dark_mode {
                    Color32::from_white_alpha(38)
                } else {
                    Color32::from_black_alpha(35)
                },
            ),
        );
        ui.add_space(4.0);
        let old = (
            self.mode.clone(),
            self.category.clone(),
            self.scope.clone(),
            self.exclude_types.clone(),
        );
        let split_toolbar = self.tool == "AI" && ui.available_width() < 770.0;
        if split_toolbar {
            ui.horizontal_top(|ui| self.search_tool_buttons(ui));
        }
        self.image_query_bar(ui);
        self.qa_toolbar_top = ui.cursor().min.y;
        self.qa_filter_top = None;
        ui.horizontal_top(|ui| {
            if !split_toolbar { self.search_tool_buttons(ui); }
            if self.tool == "AI" {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(72.0, 54.0),
                        egui::Layout::top_down(egui::Align::Max),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.spacing_mut().interact_size.y = 16.0;
                            ui.spacing_mut().icon_width = 12.0;
                            ui.spacing_mut().icon_width_inner = 8.0;
                            if self.ai_busy {
                                if ui
                                    .add_sized([72.0, 34.0], egui::Button::new(t("stop")))
                                    .clicked()
                                {
                                    self.stop_ai();
                                    self.ai_error = Some("Request stopped".into());
                                }
                            } else {
                                let enabled = !self.query.trim().is_empty() && !self.dictation_busy;
                                let accent = self.accent();
                                let send = ui.add_enabled(enabled,
                                    egui::Button::new(t("Send"))
                                        .min_size(egui::vec2(72.0, 34.0))
                                        .fill(accent.gamma_multiply(if enabled {0.35} else {0.12}))
                                        .stroke(egui::Stroke::new(1.0, accent)),
                                );
                                // Static soft outline: no idle animation or extra repaint loop.
                                for (width, alpha) in [(5.0, 0.07), (3.0, 0.15)] {
                                    ui.painter().rect_stroke(send.rect.expand(0.5), 14,
                                        egui::Stroke::new(width, accent.gamma_multiply(alpha)),
                                        egui::StrokeKind::Outside);
                                }
                                if send.clicked() { self.ask_ai(); }
                            }
                            let options=egui::Rect::from_min_size(ui.cursor().min-egui::vec2(228.0,0.0),egui::vec2(300.0,18.0));
                            let mut options_ui=ui.new_child(egui::UiBuilder::new().max_rect(options).layout(egui::Layout::left_to_right(egui::Align::Center)));
                            {let ui=&mut options_ui;
                            ui.add(egui::Checkbox::new(&mut self.ai_local_enabled,RichText::new(t("Local Search")).size(11.0)))
                                .on_hover_text(t("Allow this model to search indexed file names and snippets. Results are sent to your configured AI server. Requires tool support."));
                            if self.saved_settings.web.enabled && !self.saved_settings.web.providers().is_empty() {
                                ui.add(egui::Checkbox::new(&mut self.ai_web_enabled,RichText::new(t("Web search")).size(11.0)))
                                    .on_hover_text(t("Allow this model to search using your selected Web provider. Requires tool support."));
                            }
                            if self.saved_settings.tts_enabled {ui.add(egui::Checkbox::new(
                                &mut self.read_aloud,
                                RichText::new(t("Read Aloud")).size(11.0),
                            ))
                            .on_hover_text(
                                t("Automatically read completed AI replies using your TTS server"),
                            );}
                            }
                            ui.add_space(18.0);
                        },
                    );
                    if ui
                        .add_enabled(!self.ai_busy, egui::Button::new(RichText::new(t("New Conversation")).size(12.0)))
                        .clicked()
                    {
                        self.new_conversation();
                    }
                egui::ComboBox::from_id_salt("ai-response-length").width(78.0).selected_text(t(&self.response_length).to_owned()).show_ui(ui,|ui|{for style in ["Brief","Medium","Full"]{ui.selectable_value(&mut self.response_length,style.into(),t(style));}}).response.on_hover_text(t("Response length: Brief (concise voice assistant), Medium (8–12 sentences or two paragraphs), Full (model default)"));
                    if ui
                        .add_enabled(
                            !self.ai_busy && self.saved_settings.llm_vision,
                            egui::Button::image_and_text(
                                crate::icons::image(
                                    crate::icons::Icon::Camera,
                                    self.accent(),
                                    18.0,
                                ),
                                t("Screenshot"),
                            ),
                        )
                        .on_hover_text(t("Requires a vision model enabled in Models and AI"))
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
            if self.tool == "Web" {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if self.web.busy {
                        if ui.button(t("stop")).clicked() { self.web.stop(); self.web.error=Some(t("Web search stopped").into()); }
                    } else if ui.add_enabled(!self.query.trim().is_empty() && !self.dictation_busy,egui::Button::new(t("Search"))).clicked() { self.ask_web(); }
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
                let filter = ui.button(tr(&format!("{}{}", t("Filter"), if filtered { " •" } else { "" })));
                if filter.clicked() {
                    self.filter_open = !self.filter_open;
                    self.filter_anchor = filter.rect.left_bottom();
                }
                let mut filter_open = self.filter_open;
                let filter_window=egui::Window::new(t("Filter"))
                    .id(egui::Id::new("search-filter-panel"))
                    .open(&mut filter_open)
                    .collapsible(false)
                    .resizable(false)
                    .movable(false)
                    .fixed_pos(egui::pos2(self.filter_anchor.x.min(ctx.content_rect().right()-330.0).max(8.0),self.filter_anchor.y))
                    .constrain_to(ctx.content_rect().shrink(8.0))
                    .max_width(320.0).max_height(330.0).vscroll(true)
                    .show(ctx, |ui| {
                        ui.set_min_width(240.0);
                        ui.label(t("File type"));
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
                        ui.label(t("Sources"));
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
                                t("All sources").into()
                            } else if scopes.len() == 1 {
                                std::path::Path::new(scopes[0].trim_end_matches(['\\', '/']))
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| scopes[0].clone())
                            } else {
                                tr(&format!("{} sources", scopes.len()))
                            })
                            .width(180.0)
                            .truncate()
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(scopes.is_empty(), t("All sources"))
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
                        ui.label(t("Exclude file types"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.exclude_types)
                                .hint_text(t("Exclude types: *.zip, *.log"))
                                .desired_width(150.0),
                        );
                    });
                if let Some(window)=filter_window {if self.smoke.is_some(){ctx.data_mut(|d|d.insert_temp(egui::Id::new("qa-filter-bounds"),window.response.rect));}}
                self.filter_open = filter_open;
                let mut selected = None;
                egui::ComboBox::from_id_salt("saved-searches")
                    .selected_text(t("Saved searches"))
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

        if self.tool == "Search" {
            ui.horizontal_wrapped(|ui| {
                for kind in CATEGORIES {
                    if ui
                        .selectable_value(
                            &mut self.category,
                            kind.into(),
                            t(if kind == "All files" { "All" } else { kind }),
                        )
                        .changed()
                    {
                        self.changed();
                    }
                }
            });
        }
        if self.quick
            && self.hits.is_empty()
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
                RichText::new(t(
                    "This folder or drive has not been indexed. Add it in Sources to search it.",
                ))
                .color(self.accent()),
            );
        }
        let available = ui.available_size();
        let grid = self.s.picture_grid && matches!(self.category.as_str(), "Images" | "Videos");
        let split = if grid {
            &mut self.window_state.grid_split
        } else {
            &mut self.window_state.search_split
        };
        let mut columns = preview_columns(
            ui,
            if grid {
                "grid-preview-divider"
            } else {
                "search-preview-divider"
            },
            split,
            if grid { 0.55 } else { 0.60 },
            available.y.max(1.0),
        );
        if self.smoke.is_some() {
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new("qa-search-body"), ui.min_rect());
                d.insert_temp(egui::Id::new("qa-preview-pane"), columns[1].max_rect());
            });
        }
        {
            let ui = &mut columns[0];
            if self.s.picture_grid && matches!(self.category.as_str(), "Images" | "Videos") {
                self.picture_grid(ui, ctx);
            } else {
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
                        available.y.max(62.0),
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
                        let (rect, response) = ui
                            .allocate_exact_size(egui::vec2(row_width, 62.0), egui::Sense::click());
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
                        ui.painter().rect_filled(
                            rect,
                            14.0,
                            if selected || response.hovered() {
                                fill
                            } else if dark {
                                Color32::from_white_alpha(8)
                            } else {
                                Color32::from_white_alpha(75)
                            },
                        );
                        if selected {
                            ui.painter().rect_stroke(
                                rect,
                                14.0,
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
                            if ui.button(t("Open")).clicked() {
                                self.selected = i;
                                self.open_selected(ctx);
                                ui.close();
                            }
                            if ui.button(t("Open file location")).clicked() {
                                self.run(crate::platform::reveal(&hit.path));
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    !["calculator", "settings"].contains(&hit.kind.as_str()),
                                    egui::Button::new(t("Copy file")),
                                )
                                .clicked()
                            {
                                self.run(crate::platform::copy_file(&hit.path));
                                ui.close();
                            }
                            if ui.button(t("Copy path")).clicked() {
                                ctx.copy_text(hit.path.clone());
                                ui.close();
                            }
                            if ui.button(t("Exclude from future searches")).clicked() {
                                self.s.exclusions.push(hit.path.clone());
                                self.save();
                                self.changed();
                                ui.close();
                            }
                            if ui.button(t("Properties")).clicked() {
                                self.run(crate::platform::properties(&hit.path));
                                ui.close();
                            }
                            if ui.button(t("Delete…")).clicked() {
                                self.delete_confirm = Some(hit.clone());
                                ui.close();
                            }
                        });
                    }
                });
                self.result_scroll_offset = output.state.offset.y;
            }
        }
        self.preview(&mut columns[1], ctx);
    }

    fn picture_grid(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let columns = (ui.available_width() / 150.0).floor().max(1.0) as usize;
        if self.grid_columns != columns {
            self.scroll_to_selection = true;
        }
        self.grid_columns = columns;
        let row_height = 150.0;
        let width = (ui.available_width() - ui.spacing().item_spacing.x * (columns - 1) as f32)
            / columns as f32;
        let mut scroll = egui::ScrollArea::vertical().id_salt("picture-grid");
        if self.scroll_to_selection {
            let current =
                egui::scroll_area::State::load(ctx, ui.make_persistent_id("picture-grid"))
                    .map(|s| s.offset.y)
                    .unwrap_or(0.0);
            scroll = scroll.vertical_scroll_offset(selection_offset(
                self.selected / columns,
                row_height + ui.spacing().item_spacing.y,
                current,
                ui.available_height(),
            ));
            self.scroll_to_selection = false;
        }
        let output = scroll.show_rows(
            ui,
            row_height,
            self.hits.len().div_ceil(columns),
            |ui, rows| {
                for row in rows {
                    ui.horizontal(|ui| {
                        for index in row * columns..((row + 1) * columns).min(self.hits.len()) {
                            let hit = self.hits[index].clone();
                            if !self.thumbs.contains_key(&hit.path)
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
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(width, row_height),
                                egui::Sense::click(),
                            );
                            let fill = if self.selected == index {
                                ui.visuals().selection.bg_fill
                            } else if response.hovered() {
                                ui.visuals().widgets.hovered.bg_fill
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter().rect(
                                rect,
                                10.0,
                                fill,
                                ui.visuals().widgets.inactive.bg_stroke,
                                egui::StrokeKind::Inside,
                            );
                            let photo = egui::Rect::from_min_size(
                                rect.min + egui::vec2(6.0, 6.0),
                                egui::vec2(width - 12.0, 108.0),
                            );
                            if let Some(texture) = self.thumbs.get(&hit.path) {
                                let size = preview_image_size(texture.size_vec2(), photo.size());
                                egui::Image::new(texture).corner_radius(8).paint_at(
                                    ui,
                                    egui::Rect::from_center_size(photo.center(), size),
                                );
                            } else {
                                crate::icons::image(crate::icons::Icon::Image, self.accent(), 28.0)
                                    .paint_at(
                                        ui,
                                        egui::Rect::from_center_size(
                                            photo.center(),
                                            egui::vec2(28.0, 28.0),
                                        ),
                                    );
                            }
                            let title = Path::new(&hit.path)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            let label = egui::Rect::from_min_max(
                                egui::pos2(rect.left() + 6.0, rect.top() + 119.0),
                                rect.max - egui::vec2(6.0, 4.0),
                            );
                            let mut text = ui.new_child(egui::UiBuilder::new().max_rect(label));
                            text.add(
                                egui::Label::new(title.as_ref())
                                    .truncate()
                                    .selectable(false),
                            );
                            if response.clicked() {
                                self.selected = index;
                                self.texture = None;
                                self.preview_pending = None;
                                self.preview_error = None;
                            }
                            if response.double_clicked() {
                                self.selected = index;
                                self.open_selected(ctx);
                            }
                            response.context_menu(|ui| {
                                if ui.button(t("Open")).clicked() {
                                    self.selected = index;
                                    self.open_selected(ctx);
                                    ui.close();
                                }
                                if ui.button(t("Open file location")).clicked() {
                                    self.run(crate::platform::reveal(&hit.path));
                                    ui.close();
                                }
                                if ui.button(t("Copy file")).clicked() {
                                    self.run(crate::platform::copy_file(&hit.path));
                                    ui.close();
                                }
                            });
                            response.on_hover_text(&hit.path);
                        }
                    });
                }
            },
        );
        self.result_scroll_offset = output.state.offset.y;
    }
    fn preview(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let Some(hit) = self.hits.get(self.selected).cloned() else {
            ui.allocate_ui_with_layout(
                ui.available_size(),
                egui::Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    ui.label(t("Select a result to preview it."));
                },
            );
            return;
        };
        let pane = ui.available_size();
        let visual = content_image(&hit.path)
            || hit.path.to_lowercase().ends_with(".pdf")
            || hit.kind == "pdf"
            || hit.kind == "application"
            || crate::content::is_video(&crate::content::extension(Path::new(&hit.path)));
        let mut measure = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("preview-details-measure")
                .max_rect(egui::Rect::from_min_size(ui.next_widget_position(), pane))
                .sizing_pass()
                .invisible(),
        );
        if visual {
            self.preview_metadata(&mut measure, &hit);
        }
        let details_height = measure.min_rect().height();
        let excerpt = if !hit.text.is_empty()
            && !crate::content::is_image(&crate::content::extension(Path::new(&hit.path)))
        {
            (pane.y * 0.15).min(72.0)
        } else {
            0.0
        };
        let image_bounds = egui::vec2(
            pane.x,
            (pane.y - details_height - excerpt - 8.0 - ui.spacing().item_spacing.y * 2.0).max(1.0),
        );
        if self.smoke.is_some() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("qa-preview-fit-bounds"), image_bounds));
        }

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
                    let resolved = crate::platform::preview_path(&path);
                    let result = if application {
                        crate::platform::icon_png(&path)
                    } else if crate::content::is_video(&crate::content::extension(&resolved)) {
                        crate::content::video_frame(&resolved, hit.seconds, &s)
                    } else if path.to_lowercase().ends_with(".pdf") {
                        crate::pdf::preview(&resolved, hit.page.max(1), &query)
                    } else {
                        crate::content::decode(&resolved, &s).and_then(|i| {
                            crate::content::jpeg(&i, 1024.min(i.width().max(i.height())))
                        })
                    }
                    .map_err(|e| e.to_string());
                    let _ = tx.send(Event::Preview(path, result));
                });
            }
            if let Some((path, texture)) = &self.texture {
                if path == &hit.path {
                    let bounds = if hit.kind == "application" {
                        egui::vec2(
                            if self.large_preview { 192.0 } else { 72.0 },
                            if self.large_preview { 192.0 } else { 72.0 },
                        )
                    } else {
                        image_bounds
                    };
                    let fit = preview_image_size(texture.size_vec2(), bounds);
                    let response = ui.add(
                        egui::Image::new(texture)
                            .corner_radius(14)
                            .fit_to_exact_size(fit),
                    );
                    if self.smoke.is_some() {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("qa-preview-image"), response.rect)
                        });
                    }
                }
            } else if let Some(error) = &self.preview_error {
                ui.label(tr(&format!("Preview unavailable: {error}")));
            } else {
                ui.label(t("Loading preview…"));
            }
        } else if hit.kind == "calculator" {
            ui.heading(&hit.path);
        }
        if visual {
            ui.add_space(8.0);
        }
        self.preview_metadata(ui, &hit);
        if !crate::content::is_image(&crate::content::extension(Path::new(&hit.path))) {
            egui::ScrollArea::vertical()
                .id_salt("preview_text")
                .show(ui, |ui| {
                    ui.label(&hit.text);
                });
        }
    }
    fn preview_metadata(&mut self, ui: &mut egui::Ui, hit: &Hit) {
        ui.horizontal_wrapped(|ui| {
            if ["application", "settings"].contains(&hit.kind.as_str()) {
                ui.label(
                    RichText::new(if hit.kind == "application" {
                        t("App")
                    } else {
                        t("Settings")
                    })
                    .strong()
                    .color(self.accent()),
                );
            }
            ui.label(RichText::new(&hit.path).weak());
        });
        ui.label(tr(&format!(
            "Match score: {:.3} (ranking score)",
            hit.score
        )));
        ui.label(&hit.reason);
        if hit.page > 0 {
            ui.label(tr(&format!("Page {}", hit.page)));
        }
        if hit.seconds >= 0.0 {
            if ui.button(t("Play from matched moment")).clicked() {
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
            if ui.button(t("Stop playback")).clicked() {
                crate::platform::stop_playback();
                self.status = "Playback stopped".into();
            }
            ui.label(tr(&format!(
                "Moment {}:{:02}",
                hit.seconds as u32 / 60,
                hit.seconds as u32 % 60
            )));
        }
    }
    fn open_selected(&mut self, ctx: &egui::Context) {
        if self.image_preparing {
            self.image_search_requested = true;
            return;
        }
        if self.image_query.is_some()
            && (!self.image_search_submitted
                || self.tool == "Search" && self.hits.is_empty()
                || self.tool == "Clipboard" && self.tool_items.is_empty())
        {
            if !self.searching {
                self.image_search_requested = true;
                self.dirty = true;
            }
            return;
        }
        if self.tool == "Web" {
            if ctx.memory(|m| m.has_focus(egui::Id::new("query"))) || self.web.hits.is_empty() {
                self.ask_web();
            } else {
                self.open_web_result(ctx, self.selected);
            }
            return;
        }
        if self.tool == "AI" {
            self.ask_ai();
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("query")));
            return;
        }
        if self.tool != "Search" {
            if let Some(item) = self.tool_items.get(self.selected).cloned() {
                if self.tool == "Clipboard" && self.saved_settings.clipboard_options.paste_on_enter
                {
                    self.paste_clip(ctx, item.clone());
                } else {
                    self.copy_tool_item(ctx, item.clone());
                }
            }
            return;
        }
        if let Some(hit) = self.hits.get(self.selected).cloned() {
            // Activation is independent of the optional semantic request.
            self.engine.models.cancel_query();
            self.sequence += 1;
            self.search_pending = false;
            self.dirty = false;
            if hit.kind == "calculator" {
                ctx.copy_text(hit.path);
                return;
            }
            if self.target != 0 && self.s.dialog_integration {
                match crate::platform::send_to_dialog(self.target, &hit.path) {
                    Ok(true) => {
                        self.stop_read_aloud();
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
            let tx = self.engine.tx.clone();
            crate::engine::dispatch_action(tx, move || crate::platform::open(&hit.path));
            if self.quick {
                self.close_window(ctx);
            }
        }
    }
    fn sources(&mut self, ui: &mut egui::Ui) {
        ui.heading(t("Your sources"));
        ui.label(t(
            "Quick names for entire drives; Full index for content and media.",
        ));
        ui.group(|ui| {
            ui.heading(t("Index status"));
            ui.label(tr(&format!(
                "{} files · {} Full · {} Quick · {} vectors",
                self.stats.files, self.stats.full, self.stats.quick, self.stats.vectors
            )));
            if self.progress.phase == "Removing deleted index records" {
                ui.label(tr(&format!("{} · {}", self.progress.phase, self.progress.current)));
            } else {
                ui.label(tr(&format!(
                    "{} · {} found · {} done · {} unchanged · {} skipped",
                    self.progress.phase,
                    self.progress.found,
                    self.progress.completed,
                    self.progress.reused,
                    self.progress.skipped
                )));
            }
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
                if self.progress.phase.starts_with("EmbeddingGemma") {
                    ui.label(tr(&format!(
                        "{} chunks embedded in this file",
                        self.progress.content_chunks
                    )));
                }
                if self.engine.control.automatic.load(Ordering::Relaxed) {
                    ui.label(tr(&self.schedule_status));
                }
                if self.progress.session_completed+self.progress.session_pending>0 {
                    ui.weak(tr(&format!("Session: {} content files complete · {} pending (checkpoints saved)",self.progress.session_completed,self.progress.session_pending)));
                }
                if let Some(seconds)=self.progress.estimated_seconds {ui.weak(tr(&format!("Known queued work: about {}m {}s (learned from completed files)",seconds as u64/60,seconds as u64%60)));}
                else {ui.weak(t("Learning indexing times; estimates appear after enough similar files complete."));}
                ui.label(&self.progress.current);
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.indexing && !self.setup_busy,
                        egui::Button::new(t("Update index")),
                    )
                    .clicked()
                {
                    self.start_index(false);
                }
                if ui
                    .add_enabled(
                        self.indexing,
                        egui::Button::new(t(if self.engine.control.paused.load(Ordering::Relaxed) {
                            "Resume"
                        } else {
                            "Pause"
                        })),
                    )
                    .clicked()
                {
                    self.engine
                        .control
                        .paused
                        .fetch_xor(true, Ordering::Relaxed);
                }
                if ui
                    .add_enabled(self.indexing, egui::Button::new(t("Cancel indexing")))
                    .clicked()
                {
                    self.engine.control.cancel_by_user();
                    self.status = "Cancelled; automatic work paused until Update index or Resume automatic indexing".into();
                }
                if ui
                    .add_enabled(!self.indexing, egui::Button::new(t("Rebuild full content")))
                    .clicked()
                {
                    self.start_index(true);
                }
            });
        });
        if ui
            .add_enabled(
                !self.setup_busy,
                egui::Button::new(t("Clear entire index")).stroke(egui::Stroke::new(
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
            ui.label(t("Stopping indexing and clearing the active database…"));
        }
        ui.checkbox(
            &mut self.s.retry_failed,
            t("Retry previously skipped files on the next update"),
        );
        ui.horizontal(|ui| {
            ui.label(t("Indexing order"));
            combo(
                ui,
                "index-order",
                &mut self.s.index_order,
                &["Quick first", "Full first"],
            );
        });
        ui.checkbox(&mut self.s.rank_calibration, t("Rank Calibration"));
        ui.weak(t("Adjusts semantic ranking across text, pictures and audio. Applies at search time to existing Full-indexed items; no index update is required. Does not change model embeddings."));
        ui.checkbox(
            &mut self.s.count_files,
            t("Count files before indexing (off starts processing immediately)"),
        );
        ui.checkbox(
            &mut self.s.skip_windows_churn,
            t("Exclude common Windows/cache folders"),
        );
        ui.weak(t("Excludes Windows, Temp and cache folders from Quick and Full sources. Quick sources can still search other AppData names. Save to remove excluded records from the index in the background."));
        ui.checkbox(
            &mut self.s.include_hidden,
            t("Include hidden files and folders"),
        );
        ui.horizontal(|ui| {
            combo(ui, "add_mode", &mut self.source_mode, &["quick", "full"]);
            if ui.button(t("Add folder / drive")).clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.s.roots.push(Source {
                        path: path.to_string_lossy().into_owned(),
                        mode: self.source_mode.clone(),
                    });
                }
            }
            if ui.button(t("Add files")).clicked() {
                if let Some(paths) = rfd::FileDialog::new().pick_files() {
                    for path in paths {
                        self.s.roots.push(Source {
                            path: path.to_string_lossy().into_owned(),
                            mode: self.source_mode.clone(),
                        });
                    }
                }
            }
            if ui.button(t("Select/Deselect all")).clicked() {
                let selected = !self.source_selected.iter().all(|v| *v);
                self.source_selected = vec![selected; self.s.roots.len()];
            }
            if ui.button(t("Remove selected")).clicked() {
                self.source_selected.resize(self.s.roots.len(), false);
                for i in (0..self.s.roots.len()).rev() {
                    if self.source_selected[i] {
                        self.s.roots.remove(i);
                        self.source_selected.remove(i);
                    }
                }
            }
            if ui.button(t("Apply mode to selected")).clicked() {
                for (i, root) in self.s.roots.iter_mut().enumerate() {
                    if self.source_selected.get(i) == Some(&true) {
                        root.mode = self.source_mode.clone();
                    }
                }
            }
        });
        self.source_selected.resize(self.s.roots.len(), false);
        let mut remove = None;
        let inclusion_list = egui::ScrollArea::vertical()
            .id_salt("inclusion-list")
            .max_width(ui.available_width() * 0.88)
            .max_height(220.0)
            .min_scrolled_height(220.0)
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
                        ui.label(RichText::new(t(&root.mode)).color(self.accent()));
                        if let Some(n) = self.source_stats.get(&root.path) {
                            ui.label(tr(&format!(
                                "{} files · +{} in past hour",
                                n.files, n.added_hour
                            )));
                        }
                        if ui.small_button(t("Remove")).clicked() {
                            remove = Some(i);
                        }
                    });
                }
            });
        if self.smoke.is_some() {
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("qa-inclusion"),
                    [
                        inclusion_list.inner_rect.height(),
                        inclusion_list.content_size.y,
                    ],
                )
            });
        }
        if let Some(i) = remove {
            self.s.roots.remove(i);
            self.source_selected.remove(i);
        }
        ui.separator();
        ui.heading(t("Exclude from indexing and searches"));
        ui.horizontal(|ui| {
            if ui.button(t("Exclude folder")).clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.s.exclusions.push(p.to_string_lossy().into_owned());
                }
            }
            if ui.button(t("Exclude files")).clicked() {
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
                    .hint_text(t("Enter a file or folder path"))
                    .desired_width(320.0),
            );
            if ui.button(t("Add exclusion")).clicked() && !self.manual_exclude.is_empty() {
                self.s
                    .exclusions
                    .push(std::mem::take(&mut self.manual_exclude));
            }
        });
        self.exclusion_selected
            .resize(self.s.exclusions.len(), false);
        ui.horizontal(|ui| {
            if ui.button(t("Select/Deselect all")).clicked() {
                let selected = !self.exclusion_selected.iter().all(|v| *v);
                self.exclusion_selected.fill(selected);
            }
            if ui.button(t("Remove selected")).clicked() {
                for i in (0..self.s.exclusions.len()).rev() {
                    if self.exclusion_selected[i] {
                        self.s.exclusions.remove(i);
                        self.exclusion_selected.remove(i);
                    }
                }
            }
        });
        let mut remove = None;
        let exclusion_list = egui::ScrollArea::vertical()
            .id_salt("exclusion-list")
            .max_width(ui.available_width().min(620.0))
            .max_height(220.0)
            .min_scrolled_height(if self.s.exclusions.is_empty() {
                0.0
            } else {
                (self.s.exclusions.len() as f32
                    * (ui.spacing().interact_size.y + ui.spacing().item_spacing.y)
                    - ui.spacing().item_spacing.y)
                    .clamp(0.0, 220.0)
            })
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (i, p) in self.s.exclusions.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.exclusion_selected[i], "");
                        ui.allocate_ui_with_layout(
                            egui::vec2((ui.available_width() - 100.0).clamp(100.0, 420.0), 22.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| ui.add(egui::Label::new(p).truncate()),
                        )
                        .response
                        .on_hover_text(p);
                        if ui.small_button(t("Remove")).clicked() {
                            remove = Some(i);
                        }
                    });
                }
            });
        if self.smoke.is_some() {
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("qa-exclusion"),
                    [
                        exclusion_list.inner_rect.height(),
                        exclusion_list.content_size.y,
                    ],
                )
            });
        }
        if let Some(i) = remove {
            self.exclusion_selected.remove(i);
            self.s.exclusions.remove(i);
        }
        ui.checkbox(
            &mut self.s.excluded_sensitive,
            t("Skip content with potential credentials"),
        );
        field(
            ui,
            t("Exclude file types (e.g. *.tmp, *.log, *.bak)"),
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
        ui.add_space(12.0);
        if self.smoke.is_some() && std::env::args().any(|arg| arg == "--smoke-source-options") {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        ui.heading(t("Indexing by file type"));
        ui.weak(t("These content settings apply to Full indexing. Quick indexing searches file and folder names."));
        ui.group(|ui| {
            ui.strong(t("Applications and source code"));
            ui.checkbox(
                &mut self.s.include_programs,
                t("Search and launch Windows applications"),
            );
            ui.checkbox(
                &mut self.s.code_search,
                t("Include source code in Full indexing"),
            );
        });
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.strong(t("Images"));
            ui.checkbox(&mut self.s.index_images, t("Index image meaning"));
        ui.checkbox(
            &mut self.s.index_raw,
            t("Index camera RAW photos (experimental; off by default)"),
        );
        if self.s.index_raw {
            ui.indent("raw_options", |ui| {
                ui.label(t("ARW, CR2/CR3, DNG, RAF and other LibRaw-supported cameras. Uses embedded previews first. Full sources only; originals stay untouched."));
                ui.checkbox(&mut self.s.raw_develop, t("Develop RAW when no usable preview exists (slower; off by default)"));
                ui.add(egui::Slider::new(&mut self.s.raw_max_mib, 16..=1024).text(t("Maximum RAW input size (MiB)")));
                ui.weak(t("One decoder at a time · 512 MiB worker limit · 45 seconds per attempt · 2048px output. Development is limited to 50MP sensor data. Unsupported files remain searchable by name."));
                if !crate::raw::installed() {ui.colored_label(Color32::RED, t("RAW decoder missing: reinstall this preview build."));}
                ui.weak(t("Save, then Update index to add visual content to existing RAW entries. Quick sources keep filename-only indexing."));
            });
        }


            if self.s.index_images {
                ui.label(t("Image detail"));
                combo(
                    ui,
                    "visual_detail",
                    &mut self.s.visual_detail,
                    &["Balanced", "Detailed"],
                );
            }
            ui.weak(t("OCR for images and scanned pages is configured in Hardware."));
        });
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.strong(t("Audio"));
            ui.checkbox(&mut self.s.index_audio, t("Index audio moments"));
            ui.weak(t("Embeds audio files and audio tracks in videos. Requires FFmpeg; video decoding settings below affect frames only."));
        });
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.strong(t("Video"));
            ui.checkbox(&mut self.s.skip_similar_frames,t("Skip near-identical video frames"));
            ui.weak(t("Reduces repeated scene embeddings. Update the index to apply this to existing videos; disable for frame-by-frame recall."));
            ui.checkbox(&mut self.s.index_video, t("Index video frames"));
            if self.s.index_video {
                ui.label(t("Video decoder"));
                combo(
                    ui,
                    "video-decoder",
                    &mut self.s.video_decoder,
                    &["Windows (native)", "FFmpeg"],
                );
                ui.label(t("Decoding device"));
                combo(
                    ui,
                    "video-decode-device",
                    &mut self.s.video_decode_device,
                    &["Auto", "CPU", "GPU"],
                );
                ui.weak(t("Video decoding is separate from embedding inference. Auto follows CPU/GPU hardware preference; available codecs determine acceleration."));
                if crate::native_video::gpu_requested(&self.s) {
                    let adapters = crate::native_video::adapters();
                    let name = adapters
                        .iter()
                        .find(|(id, _)| id == &self.s.video_gpu)
                        .map(|(_, name)| name.as_str())
                        .unwrap_or("Windows default GPU");
                    ui.label(t("Video decoding GPU"));
                    egui::ComboBox::from_id_salt("video-gpu")
                        .selected_text(name)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.s.video_gpu,
                                String::new(),
                                t("Windows default GPU"),
                            );
                            for (id, name) in adapters {
                                ui.selectable_value(&mut self.s.video_gpu, id, name);
                            }
                        });
                }
                if self.s.video_decoder != "FFmpeg" {
                    ui.checkbox(
                        &mut self.s.video_ffmpeg_fallback,
                        t("Use FFmpeg if Windows cannot decode this video"),
                    );
                }
                ui.weak(t("Native Windows decoding avoids FFmpeg for video frames. Audio extraction still uses FFmpeg when Index audio moments is enabled."));
                ui.add(
                    egui::Slider::new(&mut self.s.video_interval, 1.0..=60.0)
                        .text(t("Video frame sampling interval (seconds)")),
                );
            }
        });
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.strong(t("PDF"));
            ui.checkbox(&mut self.s.pdf_visuals, t("Embed PDF page images as well as page text"));
            ui.weak(t("Adds visual matching alongside extracted PDF text. Scanned-page OCR is configured in Hardware."));
        });
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.strong(t("Shared extraction limits"));
            ui.add(egui::Slider::new(&mut self.s.max_file_mib, 1..=256).text(t("Maximum extraction size (MiB)")));
            ui.weak(t("Applies to images, PDFs, text and Office documents. Audio and video use their own streaming extraction; file names remain searchable."));
        });
        ui.add_space(8.0);
        ui.label(t("Full-index extraction changes apply to changed files; use Rebuild Full to refresh existing content."));
        ui.separator();
        if ui
            .add_enabled(
                !self.indexing && !self.setup_busy,
                egui::Button::new(t("Import a copy of the existing VectorDash index")),
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
        self.devices_server = format!("{}|{}", self.s.server, self.s.backend());
        if !self.s.runtime_compatible() {
            self.devices_loading = false;
            self.devices.clear();
            self.status = "Download the selected backend runtime before detecting devices".into();
            return;
        }
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

    fn cycle_file_type(&mut self) {
        let next = (CATEGORIES
            .iter()
            .position(|c| *c == self.category)
            .unwrap_or(0)
            + 1)
            % CATEGORIES.len();
        self.category = CATEGORIES[next].into();
        self.type_flash = Some(Instant::now());
        self.changed();
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
        let bars = (rect.width() / 4.0).floor().max(2.0) as usize;
        for i in 0..bars {
            let position = i as f32 / (bars - 1) as f32 * (wave.len() - 1) as f32;
            let index = position.floor() as usize;
            let fraction = position.fract();
            let value =
                wave[index] * (1.0 - fraction) + wave[(index + 1).min(wave.len() - 1)] * fraction;
            let x = rect.left() + i as f32 / (bars - 1) as f32 * rect.width();
            let h = (value.sqrt() * 22.0).clamp(0.5, 12.0);
            ui.painter().line_segment(
                [egui::pos2(x, center - h), egui::pos2(x, center + h)],
                egui::Stroke::new(1.5, self.accent()),
            );
        }
    }
    fn query_uses_embeddings(&self) -> bool {
        crate::search_filters::Filters::parse(&self.query).is_ok_and(|f| f.uses_embeddings())
    }
    fn embedding_activity(&self) -> bool {
        if self.image_query.is_some() && matches!(self.tool.as_str(), "Search" | "Clipboard") {
            return self.searching;
        }

        self.page == 0
            && self.tool == "Search"
            && self.mode != "Names"
            && self.query_uses_embeddings()
            && (self.searching
                || self.model_warm_busy
                || self.engine.models.loading.load(Ordering::Acquire))
    }
    fn footer_text(&self) -> String {
        if self.page == 0 && self.image_preparing {
            return t("Preparing image…").into();
        }
        if self.page == 0 && self.image_query.is_some() && self.searching {
            return t("Finding similar images…").into();
        }
        if self.page == 0 && self.image_query.is_some() && !self.image_search_submitted {
            return t("Press Enter to find similar images").into();
        }

        if self.page != 0 {
            return t(&self.status).to_owned();
        }
        if self.tool == "Search" && self.model_warm_busy && self.query_uses_embeddings() {
            return t("Warming embedding model…").to_owned();
        }
        if self.dictation_busy {
            return "Transcribing…".into();
        }
        if crate::dictation::recording() {
            return "Listening…".into();
        }
        if self.tool == "Web" {
            return self.web_footer();
        }
        if self.ai_busy {
            return format!(
                "AI responding · {:.1} s",
                self.ai_started.map_or(0.0, |t| t.elapsed().as_secs_f64())
            );
        }
        if self.tts_busy {
            let state = if self.tts_paused.load(Ordering::Acquire) {
                "Read aloud paused"
            } else {
                "Reading aloud"
            };
            return self
                .ai_elapsed
                .map(|s| format!("{state} · AI {s:.1} s"))
                .unwrap_or_else(|| state.into());
        }
        if self.tool == "Search"
            && self.mode != "Names"
            && self.query_uses_embeddings()
            && !self.s.remote_embeddings()
        {
            if let Some(reason) = self.engine.models.search_hold(&self.s) {
                return format!("EmbeddingGemma 2 paused: {reason}");
            }
            if let Some(error) = &self.model_warm_error {
                return error.clone();
            }
        }
        if self.embedding_activity() {
            return if self.engine.models.loading.load(Ordering::Acquire) {
                format!(
                    "Loading EmbeddingGemma 2 · {}",
                    self.engine.models.loading_phase()
                )
            } else {
                "Waiting for EmbeddingGemma 2…".into()
            };
        }
        if self.searching {
            return "Searching…".into();
        }
        if let Some(error) = self.search_error.as_ref().filter(|_| self.tool != "AI") {
            return error.clone();
        }
        if self.tool == "AI" {
            if let Some(error) = &self.ai_error {
                return error.clone();
            }
        }
        for notice in [&self.dictation_status, &self.tts_status] {
            if !notice.is_empty() && !notice.starts_with("Recording") {
                return t(notice).to_owned();
            }
        }
        if !self.status.is_empty()
            && !self.status.contains(" results · ")
            && !self.status.starts_with("Targeted changes")
            && !self.status.starts_with("Recovery/")
        {
            return t(&self.status).to_owned();
        }
        match self.tool.as_str() {
            "Search" => format!(
                "{} {} · {:.1} ms",
                self.hits.len(),
                t("results"),
                self.search_ms
            ),
            "AI" => self
                .ai_elapsed
                .map(|s| format!("{} · {s:.1} s", t("AI response")))
                .unwrap_or_default(),
            "Clipboard" => format!("{} {}", self.tool_items.len(), t("items")),
            "Emoji" => format!("{} emoji", self.tool_items.len()),
            _ => String::new(),
        }
    }
    fn draw_footer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (row, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), ui.spacing().interact_size.y.max(40.0)),
            egui::Sense::hover(),
        );
        if self.smoke.is_some() {
            ctx.data_mut(|d| {
                d.insert_temp(egui::Id::new("qa-footer-row"), row);
            });
        }
        let mut left = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let start = if self.quick {
            let button = left.button(t("Open full app"));
            if self.smoke.is_some() {
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new("qa-open-full"), button.rect);
                });
            }
            if button.clicked() {
                let _ = self.engine.tx.send(Event::Full);
            }
            left.min_rect().right() + 8.0
        } else {
            row.left()
        };
        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.spacing_mut().item_spacing.x = 5.0;
        for (key, action) in [
            (
                "Esc",
                if self.ai_busy || self.web.busy {
                    "stop"
                } else {
                    "dismiss"
                },
            ),
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
                } else if self.tool == "Web" {
                    "search / open"
                } else {
                    "copy"
                },
            ),
            ("↑ ↓", if self.tool == "AI" { "page" } else { "select" }),
        ] {
            if key.is_empty() {
                continue;
            }
            right.label(RichText::new(t(action)).size(11.0));
            right.label(RichText::new(key).size(11.0).strong().color(self.accent()));
        }
        let end = (right.min_rect().left() - 8.0).max(start);
        let center =
            egui::Rect::from_min_max(egui::pos2(start, row.top()), egui::pos2(end, row.bottom()));
        let footer = tr(&self.footer_text());
        let mut middle = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(center)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let playback = self.page == 0 && self.tts_busy;
        let load_override = self.page == 0
            && self.tool == "Search"
            && self.query_uses_embeddings()
            && self.mode != "Names"
            && !self.s.remote_embeddings()
            && (self.engine.models.search_hold(&self.s).is_some()
                || self.model_warm_error.is_some());
        let controls_width = if playback {
            94.0
        } else if load_override {
            95.0
        } else {
            0.0
        };
        let text = footer.clone();
        let text_width = middle
            .painter()
            .layout_no_wrap(
                text.clone(),
                egui::FontId::proportional(12.0),
                middle.visuals().text_color(),
            )
            .size()
            .x
            .min((center.width() - controls_width).max(0.0));
        let needed = text_width + controls_width;
        let offset = ((center.width() - needed) / 2.0).max(0.0);
        middle.add_space(offset);
        let response = middle.add_sized(
            [text_width, 22.0],
            egui::Label::new(RichText::new(text).size(12.0)).truncate(),
        );
        if self.smoke.is_some() {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-footer-text"), response.rect));
        }
        response.on_hover_text(&footer);
        if playback {
            let paused = self.tts_paused.load(Ordering::Acquire);
            let pause = middle
                .add(egui::Button::image(crate::icons::image(
                    if paused {
                        crate::icons::Icon::Play
                    } else {
                        crate::icons::Icon::Pause
                    },
                    self.accent(),
                    18.0,
                )))
                .on_hover_text(t(if paused {
                    "Resume read aloud"
                } else {
                    "Pause read aloud"
                }));
            if self.smoke.is_some() {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-playback-pause"), pause.rect));
            }
            if pause.clicked() {
                self.set_read_aloud_paused(!paused);
            }
            if middle
                .add(egui::Button::image(crate::icons::image(
                    crate::icons::Icon::Stop,
                    self.accent(),
                    18.0,
                )))
                .on_hover_text(t("Stop playback"))
                .clicked()
            {
                self.stop_read_aloud();
            }
        }
        if load_override && middle.small_button(t("Load anyway")).clicked() {
            self.engine
                .models
                .session_override
                .store(true, Ordering::Release);
            self.model_warm_attempted = false;
            self.model_warm_error = None;
            self.dirty = true;
        }
        if self.smoke.is_some() {
            ctx.data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("qa-footer-status"),
                    egui::Rect::from_min_size(
                        egui::pos2(center.left() + offset, center.top()),
                        egui::vec2(needed, center.height()),
                    ),
                );
            });
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
                key == "runtime" || key == "embedding-bundle",
                key == "embedding" || key == "embedding-bundle",
                key == "encoder" || key == "embedding-bundle",
                key == "ocr",
            );
        }
    }
    fn components_ui(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui|{
            ui.heading(t("Status"));
            let loaded=self.engine.models.loaded.load(Ordering::Acquire);
            for(key,name)in [("runtime","Local llama-server"),("embedding","EmbeddingGemma 2"),("encoder","Photos / audio / video encoder"),("ocr","Tesseract OCR"),("dictation","Parakeet dictation"),("ffmpeg","FFmpeg"),("pdf","PDF renderer"),("raw","Camera RAW decoder")]{
                let usage=self.component_stats.get(key).cloned().unwrap_or_default();
                let installed=if key=="dictation"{crate::dictation::ready(&self.home,&self.s)}else if key=="ffmpeg"||key=="pdf"{usage.disk>0}else if key=="raw"{crate::raw::installed()}else{Path::new(match key{"runtime"=>&self.s.server,"embedding"=>&self.s.model,"encoder"=>&self.s.projector,_=>&self.s.ocr}).is_file()};
                let active=if key=="dictation"{crate::dictation::pid()!=0}else if key=="encoder"{self.engine.models.media_loaded()}else if key=="runtime"||key=="embedding"{loaded}else{usage.ram>0};
                let remote=if key=="dictation"{self.s.remote_dictation()}else{["runtime","embedding","encoder"].contains(&key)&&self.s.remote_embeddings()};
                let state=if remote{let server=&self.remote_servers[usize::from(key=="dictation")];if server.busy{"Remote · checking…"}else if server.error{"Remote · not ready"}else if key=="encoder"{match server.vision{Some(true)=>"Remote · mmproj detected",Some(false)=>"Remote · mmproj not detected",None=>"Remote · vision not reported"}}else if server.note.is_empty(){"Remote · not checked"}else{"Remote · ready"}}else if active{"Loaded"}else if installed{"Installed · on demand"}else{"Not installed"};
                let color=if remote{self.accent()}else{crate::appearance::component_color(ui.visuals().dark_mode, active, installed)};
                ui.horizontal_wrapped(|ui|{
                    if remote&&key=="runtime" {ui.label(t("Embedding model server"));}else if remote&&key=="dictation" {ui.label(tr(&format!("Dictation · {}",if self.s.dictation_model.is_empty(){"Select model"}else{self.s.dictation_model.as_str()})));}else {ui.label(if remote&&key=="embedding"&&!self.s.remote_model.is_empty(){self.s.remote_model.as_str()}else{t(name)});}
                    ui.label(RichText::new(t(state)).color(color));
                    if remote {let server=&self.remote_servers[usize::from(key=="dictation")];if key=="embedding"||key=="dictation" {if server.dimensions.is_some_and(|d|d<768){ui.colored_label(Color32::from_rgb(225,145,35),&server.note);}else{ui.weak(tr(&server.note));}}else{ui.weak(t("Server managed · no local model loaded"));} }
                    else if self.component_stats_busy&&self.component_stats.is_empty(){ui.weak(t("Measuring…"));}
                    else if key=="pdf"{ui.weak(tr(&format!("RAM shared with app · Disk {}",crate::resources::format_bytes(usage.disk))));}
                    else{ui.weak(tr(&format!("{}RAM {} · VRAM {} · Disk {}",if usage.shared{"Shared " }else{""},crate::resources::format_bytes(usage.ram),usage.vram.map(crate::resources::format_bytes).unwrap_or_else(||"Unavailable".into()),crate::resources::format_bytes(usage.disk))));}
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                        if remote {return;}
                        let managed=crate::resources::managed(&self.home,&self.s,key);
                        if installed{if ui.add_enabled(!self.setup_busy&&managed,egui::Button::new(t("Uninstall"))).on_hover_text(if managed{t("Remove this downloaded item")}else{t("External or bundled: managed by its installer")}).clicked(){self.remove_component=Some(key.into());}}
                        else if ui.add_enabled(!self.setup_busy&&!["ffmpeg","pdf","raw"].contains(&key),egui::Button::new(t("Install"))).clicked(){self.install_component(key);}
                    });
                });
            }
            ui.weak(t("EmbeddingGemma, encoder and llama-server share one inference process; shared RAM/VRAM totals are not separate allocations. VRAM is dedicated GPU memory sampled by Windows."));
            ui.horizontal(|ui|{
                let loaded=self.engine.models.loaded.load(Ordering::Acquire);
                if ui.add_enabled(!self.setup_busy&&!loaded&&!self.s.remote_embeddings(),egui::Button::new(t("Start"))).clicked(){
                    self.save();let settings=self.s.clone();let models=self.engine.models.clone();let media=Path::new(&self.s.projector).is_file();let tx=self.engine.tx.clone();self.setup_busy=true;
                    std::thread::spawn(move||{let _=tx.send(Event::Setup(models.start(&settings,media).map(|_|vec![]).map_err(|e|e.to_string())));});
                }
                if ui.add_enabled(loaded||crate::dictation::pid()!=0,egui::Button::new(t("Stop / unload models"))).clicked(){let models=self.engine.models.clone();std::thread::spawn(move||{let _=models.stop();crate::dictation::stop();});}
                if self.setup_busy&&ui.button(t("Cancel setup")).clicked(){self.setup.cancel.store(true,Ordering::Release);}
            });
            if self.setup_busy{if let Some((name,done,total))=&self.setup_progress{ui.label(t(name));if *total>0{ui.add(egui::ProgressBar::new(*done as f32/ *total as f32).show_percentage());}}}
            for(index,name)in [(2,"LLM Model"),(3,"Voice Model")]{if !self.remote_fields(index).2.is_empty(){let state=&self.remote_servers[index];ui.label(tr(&format!("{name} · Remote · {}",if state.busy{"Checking…"}else if state.error{"Not ready"}else if state.note.is_empty(){"Not checked"}else{"Ready"})));if !state.note.is_empty(){ui.weak(tr(&state.note));}}}

            ui.separator();let totals=self.component_stats.get("total").cloned().unwrap_or_default();let metrics=crate::scheduling::metrics();let percent=|v:Option<f32>|v.map(|v|format!("{v:.1}%")).unwrap_or_else(||"Unavailable".into());
            ui.label(tr(&format!("VectorDash + loaded local models · CPU {} · GPU {} · System memory {} · GPU memory {} · Storage {}",percent(metrics.owned_cpu),percent(metrics.owned_gpu),crate::resources::format_bytes(totals.ram),totals.vram.map(crate::resources::format_bytes).unwrap_or_else(||"Unavailable".into()),crate::resources::format_bytes(totals.disk))));
            ui.weak(t("Remote server resource use is not included. Shared model memory is counted once."));

        });
        ui.add_space(12.0);
    }
    fn remote_fields(&self, index: usize) -> (&str, &str, &str) {
        match index {
            0 => (&self.s.remote_url, &self.s.remote_key, &self.s.remote_model),
            1 => (
                &self.s.dictation_url,
                &self.s.dictation_key,
                &self.s.dictation_model,
            ),
            2 => (&self.s.llm_url, &self.s.llm_key, &self.s.llm_model),
            _ => (&self.s.tts_url, &self.s.tts_key, &self.s.tts_model),
        }
    }
    fn remote_signature(&self, index: usize) -> String {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.remote_fields(index).hash(&mut hash);
        hash.finish().to_string()
    }
    fn refresh_remote(&mut self, index: usize, check: bool) {
        if self.remote_servers[index].busy
            && self.remote_servers[index].signature == self.remote_signature(index)
        {
            return;
        }
        let signature = self.remote_signature(index);
        let (url, key, model) = self.remote_fields(index);
        let (url, key, model) = (url.to_owned(), key.to_owned(), model.to_owned());
        self.remote_servers[index] = RemoteState {
            signature: signature.clone(),
            busy: true,
            ..Default::default()
        };
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let result =
                crate::remote::service_catalog(&url, &key, &model, check, index == 0 || index == 2)
                    .map_err(|e| e.to_string());
            let _ = tx.send(Event::RemoteCatalog(index, signature, result));
        });
    }
    fn remote_server_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        if index == 0 {
            ui.checkbox(
                &mut self.s.remote_warm_on_open,
                t("Warm embedding model when search opens"),
            );
            ui.weak(t("Sends a small embedding request when Hybrid or Semantic search opens, before you type. The remote server may need a few seconds to load its model."));
        }

        let signature = self.remote_signature(index);
        if self.remote_servers[index].signature != signature {
            self.remote_servers[index] = RemoteState {
                signature: signature.clone(),
                ..Default::default()
            };
        }
        let busy = self.remote_servers[index].busy;
        let (url, key, model) = if index == 0 {
            (
                &mut self.s.remote_url,
                &mut self.s.remote_key,
                &mut self.s.remote_model,
            )
        } else {
            (
                &mut self.s.dictation_url,
                &mut self.s.dictation_key,
                &mut self.s.dictation_model,
            )
        };
        field(ui, t("API base URL"), url);
        ui.weak(t("Example: http://localhost:8080/v1"));
        secret(ui, t("API key (optional)"), key);
        let mut refresh = false;
        let mut check = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(t("Model"));
            egui::ComboBox::from_id_salt(("remote-model", index))
                .selected_text(if model.is_empty() {
                    t("Select model")
                } else {
                    model.as_str()
                })
                .width(260.0)
                .show_ui(ui, |ui| {
                    for id in &self.remote_servers[index].models {
                        ui.selectable_value(model, id.clone(), id);
                    }
                });
            refresh = ui
                .add_enabled(!busy, egui::Button::new(t("Refresh models / Test")))
                .clicked();
        });
        field(ui, t("Or enter a model ID"), model);
        if index == 0 {
            ui.checkbox(
                &mut self.s.remote_context_auto,
                t("Detect maximum context automatically"),
            );
            if ui
                .add(
                    egui::DragValue::new(&mut self.s.remote_context)
                        .range(128..=1048576)
                        .prefix(t("Max context: ")),
                )
                .changed()
            {
                self.s.remote_context_auto = false;
            }
            if self.s.remote_context_auto
                && !self.remote_servers[0].busy
                && !self.remote_servers[0].note.is_empty()
                && self.remote_servers[0].context.is_none()
            {
                ui.weak(t(
                    "Server did not report a context limit; enter it manually.",
                ));
            }
            ui.weak(t("Used as a conservative input budget; oversized text is split. Remote servers receive indexed text/media and semantic queries. Server configuration determines actual context and media support. Changing the embedding model requires re-indexing full-indexed files to keep vectors compatible."));
            ui.weak(t("Switching Local/Remote preserves existing embeddings. Use the same model, dimensions, pooling and preprocessing. Rebuilding is manual."));
            check = ui
                .add_enabled(
                    !busy && !model.is_empty(),
                    egui::Button::new(t("Check embeddings enabled")),
                )
                .clicked();
        } else {
            ui.weak(t(
                "Sends microphone audio to /v1/audio/transcriptions when you dictate.",
            ));
        }
        if index == 0
            && self.remote_servers[index]
                .dimensions
                .is_some_and(|d| d < 768)
        {
            ui.colored_label(Color32::from_rgb(225,145,35),t("Warning: embedding output is below 768 dimensions. Search quality may be reduced; indexed and query vectors must use the same dimensions."));
        }
        if busy {
            ui.spinner();
        } else if !self.remote_servers[index].note.is_empty() {
            ui.label(RichText::new(tr(&self.remote_servers[index].note)).color(
                if self.remote_servers[index].error {
                    Color32::RED
                } else {
                    Color32::from_rgb(40, 180, 85)
                },
            ));
        }
        if signature != self.remote_signature(index) {
            self.remote_due[index] = Some(Instant::now() + Duration::from_millis(500));
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        }
        if refresh || check {
            self.refresh_remote(
                index,
                check || (index == 0 && !self.s.remote_model.is_empty()),
            );
        }
    }
    fn models(&mut self, ui: &mut egui::Ui) {
        if !self.hardware_open {
            self.hardware_open = true;
            for (index, enabled) in [
                (0, self.s.remote_embeddings()),
                (1, self.s.remote_dictation()),
                (2, self.s.llm_enabled),
                (3, self.s.tts_enabled),
            ] {
                if enabled && !self.remote_fields(index).2.is_empty() {
                    self.refresh_remote(index, index == 0);
                }
            }
        }

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

        ui.heading(t("Hardware"));
        ui.heading(t("Inference hardware"));
        combo(
            ui,
            "hardware",
            &mut self.s.hardware,
            &["CPU / system RAM", "Dedicated GPU", "Integrated GPU"],
        );
        if self.s.hardware == "Integrated GPU"
            && (self.s.runtime_backend == "CUDA"
                || self.s.device.to_ascii_uppercase().starts_with("CUDA"))
        {
            self.s.runtime_backend = "Vulkan".into();
            self.s.device.clear();
        }
        if self.s.hardware != "CPU / system RAM" {
            let backends: &[&str] = if self.s.hardware == "Integrated GPU" {
                &["Auto", "Vulkan"]
            } else {
                &["Auto", "CUDA", "Vulkan"]
            };
            combo(ui, "backend", &mut self.s.runtime_backend, backends);
            if !self.s.runtime_compatible() {
                ui.label(RichText::new(t("The selected runtime does not support this backend. Download its runtime to detect devices.")).color(self.accent()));
                if ui
                    .add_enabled(
                        !self.setup_busy,
                        egui::Button::new(t("Download selected backend runtime")),
                    )
                    .clicked()
                {
                    self.install_component("runtime");
                }
            }
            if self.s.hardware == "Integrated GPU" {
                ui.weak(t("Intel iGPUs require Vulkan and an enabled graphics device with a compatible Intel driver. A CUDA runtime cannot detect Intel GPUs."));
            }
            ui.label(tr(&format!("Setup backend: {}. CUDA is recommended for NVIDIA; Vulkan supports Intel/AMD and compatible NVIDIA builds.",if self.s.backend()=="cuda-"{"CUDA"}else{"Vulkan"})));
            if self.devices_server != format!("{}|{}", self.s.server, self.s.backend())
                && !self.devices_loading
            {
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
                .selected_text(tr(&text))
                .width(ui.available_width().min(620.0))
                .show_ui(ui, |ui| {
                    for (id, name) in &self.devices {
                        ui.selectable_value(
                            &mut self.s.device,
                            id.clone(),
                            tr(&format!("{id} — {name}")),
                        );
                    }
                });
            if ui
                .add_enabled(
                    !self.devices_loading,
                    egui::Button::new(t("Refresh devices")),
                )
                .clicked()
            {
                self.refresh_devices();
            }
            if self.s.device.to_ascii_uppercase().starts_with("CUDA") {
                ui.label(t("Selected device backend: CUDA"));
            } else if !self.s.device.is_empty() {
                ui.label(t("Selected device backend: Vulkan"));
            }
        }
        ui.collapsing(t("Advanced device identifier"), |ui| {
            field(ui, t("Device identifier"), &mut self.s.device);
        });
        path_field(ui, t("Local llama-server runtime"), &mut self.s.server);
        if !Path::new(&self.s.server).is_file() && ui.button(t("Download/setup runtime")).clicked()
        {
            self.install_component("runtime");
        }
        ui.collapsing(t("Optional download tokens"), |ui| {
            secret(ui, t("GitHub download token"), &mut self.s.github_token);
            secret(ui, t("Hugging Face token"), &mut self.s.hf_token);
        });
        ui.add_space(12.0);
        ui.heading(t("OCR and media tools"));
        ui.checkbox(
            &mut self.s.ocr_enabled,
            t("Enable OCR text extraction for images and scanned PDF pages"),
        );
        path_field(ui, t("Tesseract OCR"), &mut self.s.ocr);
        if self.s.ocr_enabled {
            field(ui, t("OCR languages"), &mut self.s.ocr_languages);
        }
        path_field(ui, t("FFmpeg (bundled by default)"), &mut self.s.ffmpeg);
        ui.add_space(12.0);
        self.uninstall_button(ui, "Uninstall all runtimes…", "runtimes");
    }
    fn embedding_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(t("Embedding Model"));
        let mut server = if self.s.remote_embeddings() {
            "Remote"
        } else {
            "Local"
        }
        .to_owned();
        combo(ui, "embedding-server", &mut server, &["Local", "Remote"]);
        if server
            != if self.s.remote_embeddings() {
                "Remote"
            } else {
                "Local"
            }
        {
            self.s.embedding_server = server.clone();
        }
        if server == "Local" {
            ui.checkbox(&mut self.s.unload_idle, t("Unload local models when idle"));
            if self.s.unload_idle {
                ui.add(
                    egui::Slider::new(&mut self.s.unload_minutes, 1..=120)
                        .text(t("Idle unload (minutes)")),
                );
            }

            ui.label(t("An unloaded model may take a few seconds to load for the next Hybrid or Semantic search. File/Folder Names search needs no model."));
            ui.checkbox(
                &mut self.s.pause_for_game,
                t("Unload local models and pause indexing when a game is detected"),
            );
            ui.checkbox(
                &mut self.s.pause_for_fullscreen,
                t("Unload local models and pause indexing for full-screen applications"),
            );
            ui.horizontal(|ui| {
            ui.label(t("Additional game executable names"));
            ui.text_edit_singleline(&mut self.s.game_processes).on_hover_text(t("Comma-separated names, such as forza.exe, cs2.exe. Windows detection also recognizes full-screen Direct3D and common game install paths."));
        });
            ui.checkbox(
                &mut self.s.defer_search_model,
                t("Delay search model loading while CPU/GPU or power limits are not met"),
            );
            ui.label(t("Uses Scheduling resource limits; keyboard/mouse activity is ignored for searches. Load anyway applies only while the current search window remains open."));
            ui.checkbox(
                &mut self.s.show_model_notices,
                t("Show unloaded-model notices in search"),
            );

            ui.label(t("OCR and media helpers run only while needed. Remote servers are managed by their host."));
            combo(ui, "quant", &mut self.s.quant, &["Q8_0", "BF16"]);
            path_field(ui, t("EmbeddingGemma 2 GGUF"), &mut self.s.model);
            path_field(ui, t("Matching mmproj"), &mut self.s.projector);
            self.local_model_actions(ui, false);
        } else {
            self.remote_server_ui(ui, 0);
        }
    }
    fn dictation_settings(&mut self, ui: &mut egui::Ui) {
        let header = ui.heading(t("Dictation Model"));
        if self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-dictation-section") {
            ui.scroll_to_rect(header.rect, Some(egui::Align::TOP));
        }
        combo(
            ui,
            "dictation-server",
            &mut self.s.dictation_server,
            &["Local", "Remote"],
        );
        if !self.s.remote_dictation() {
            ui.label(t("Parakeet TDT 0.6B v2 · English speech-to-text"));
            combo(
                ui,
                "dictation-device",
                &mut self.s.dictation_device,
                &["CPU", "NVIDIA CUDA"],
            );
            egui::ComboBox::from_id_salt("dictation-quant").selected_text(match crate::dictation::quant(&self.s){"fp16"=>t("fp16"),_=>t("8-bit")}).show_ui(ui,|ui|{
            ui.selectable_value(&mut self.s.dictation_quant,"fp16".into(),t("fp16"));
            ui.selectable_value(&mut self.s.dictation_quant,"8-bit".into(),t("8-bit"));
        }).response.on_hover_text(t("Quantization is independent of device. CUDA can fall back to CPU for unsupported 8-bit operators."));

            ui.checkbox(
                &mut self.s.dictation_keep_loaded,
                t("Keep Parakeet loaded between transcriptions"),
            );
            if self.s.dictation_keep_loaded {
                ui.add(
                    egui::Slider::new(&mut self.s.dictation_unload_minutes, 1..=120)
                        .text(t("Unload after idle (minutes)")),
                );
            } else {
                ui.weak(t("Unloads immediately after each transcription."));
            }
            self.local_model_actions(ui, true);
        } else {
            self.remote_server_ui(ui, 1);
        }
        ui.checkbox(
            &mut self.s.dictation_auto_finish,
            t("Transcribe automatically after I stop speaking"),
        );
        if self.s.dictation_auto_finish {
            ui.add(
                egui::Slider::new(&mut self.s.dictation_silence_seconds, 0.6..=5.0)
                    .text(t("Silence delay (seconds)")),
            );
        }
    }
    fn uninstall_button(&mut self, ui: &mut egui::Ui, label: &str, key: &str) {
        if ui
            .add_enabled(
                !self.setup_busy,
                egui::Button::new(label)
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(210, 55, 65))),
            )
            .clicked()
        {
            self.remove_component = Some(key.into());
        }
    }
    fn local_model_actions(&mut self, ui: &mut egui::Ui, dictation: bool) {
        let ready = if dictation {
            crate::dictation::ready(&self.home, &self.s)
        } else {
            Path::new(&self.s.model).is_file()
                && Path::new(&self.s.projector).is_file()
                && Path::new(&self.s.server).is_file()
        };
        if !ready
            && ui
                .add_enabled(!self.setup_busy, egui::Button::new(t("Download/setup")))
                .clicked()
        {
            self.install_component(if dictation {
                "dictation"
            } else {
                "embedding-bundle"
            });
        }
        let owned = if dictation {
            crate::dictation::model_root(&self.home, &self.s).exists()
        } else {
            ["embedding", "encoder"].iter().any(|key| {
                crate::resources::managed(&self.home, &self.s, key)
                    && crate::resources::paths(&self.home, &self.s, key)
                        .iter()
                        .any(|p| p.exists())
            })
        };
        ui.add_enabled_ui(owned, |ui| {
            self.uninstall_button(
                ui,
                "Uninstall local model…",
                if dictation {
                    "dictation-model"
                } else {
                    "embedding-models"
                },
            );
        });
    }
    fn settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading(t("Settings"));
        ui.horizontal(|ui| {
            ui.label(t("Language"));
            egui::ComboBox::from_id_salt("interface-language")
                .selected_text(crate::i18n::language_name(&self.s.language))
                .show_ui(ui, |ui| {
                    for (code, name) in crate::i18n::LANGUAGES {
                        let name = if *code == "system" {
                            t("Use system language")
                        } else {
                            name
                        };
                        ui.selectable_value(&mut self.s.language, (*code).into(), name);
                    }
                });
        });
        ui.collapsing(t("Saved searches"), |ui| {
            let mut remove = None;
            for (i, search) in self.s.saved_searches.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut search.name));
                    if ui.button(t("Remove")).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.s.saved_searches.remove(i);
            }
        });

        ui.heading(t("Search and dictation"));
        self.search_order_settings(ui);
        self.ai_search_defaults_settings(ui);

        if !self.microphones_loaded && !self.microphones_busy {
            self.refresh_microphones();
        }
        ui.horizontal(|ui| {
            ui.label(t("Microphone"));
            egui::ComboBox::from_id_salt("microphone")
                .selected_text(if self.s.microphone.is_empty() {
                    t("Windows default")
                } else {
                    &self.s.microphone
                })
                .width(320.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.s.microphone,
                        String::new(),
                        t("Windows default"),
                    );
                    for name in &self.microphones {
                        ui.selectable_value(&mut self.s.microphone, name.clone(), name);
                    }
                });
            if ui
                .add_enabled(!self.microphones_busy, egui::Button::new(t("Refresh")))
                .clicked()
            {
                self.refresh_microphones();
            }
        });
        ui.heading(t("Appearance"));
        combo(ui, "theme", &mut self.s.theme, &["System", "Dark", "Light"]);
        ui.horizontal(|ui| {
            ui.label(t("App theme color"));
            ui.color_edit_button_srgb(&mut self.s.accent);
        });
        ui.horizontal(|ui| {
            ui.label(t("Window color tone"));
            ui.color_edit_button_srgb(&mut self.s.popup_tone);
        });
        let mut transparency = (1.0 - self.s.popup_opacity) * 100.0;
        if ui
            .add(
                egui::Slider::new(&mut transparency, 0.0..=90.0).text(t("Window transparency (%)")),
            )
            .changed()
        {
            self.s.popup_opacity = 1.0 - transparency / 100.0;
        }
        ui.add(
            egui::Slider::new(&mut self.s.popup_gradient, 0.0..=1.0)
                .text(t("Window gradient strength")),
        );
        ui.label(t("Applies to both windows. Higher transparency reveals more of the blurred desktop. Appearance changes preview immediately; click Save to keep them."));
        if ui.button(t("Reset appearance")).clicked() {
            self.s.accent = [133, 115, 255];
            self.s.popup_tone = [138, 115, 168];
            self.s.popup_opacity = 0.75;
            self.s.popup_gradient = 0.2;
        }
        ui.add_space(12.0);
        self.trail_color_settings(ui);
        ui.heading(t("Startup and window behavior"));
        ui.checkbox(
            &mut self.s.explorer_integration,
            t("Double Ctrl in File Explorer opens quick search"),
        );
        if self.s.explorer_integration {
            ui.checkbox(
                &mut self.s.explorer_scope,
                t("Explorer double Ctrl: search current folder/drive and subfolders"),
            );
        }
        ui.checkbox(
            &mut self.s.dialog_integration,
            t("Double Ctrl in Open/Save dialogs; send selected paths to the dialog"),
        );
        ui.label(t("Standard Windows dialogs are supported. Custom and elevated dialogs may require Copy path."));
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
        ui.label(t("Closing to tray keeps the global shortcut and scheduled indexing running. Exit stops the application and owned model server."));
        ui.checkbox(&mut self.s.start_with_windows, t("Start with Windows"));
        ui.checkbox(&mut self.s.start_hidden, t("Start hidden at startup"));
        combo(
            ui,
            "performance",
            &mut self.s.performance,
            &["Low memory", "Balanced", "Fast search"],
        );
        ui.horizontal(|ui| {
            ui.label(t("Semantic cache precision"));
            combo(
                ui,
                "cache-precision",
                &mut self.s.cache_precision,
                &["FP32", "FP16"],
            );
        });
        ui.weak(t("FP16 halves vector-cache storage; calculations stay FP32. Very close results may swap places. No re-indexing is needed; Low memory still streams FP32 from disk."));
        ui.checkbox(
            &mut self.s.picture_grid,
            t("Picture grid for Images and Videos searches"),
        );
        ui.label(t("Database and filename catalog remain on disk. Low memory: 8 MiB pages and streamed vectors. Balanced: 32 MiB pages and up to 128 MiB compact vectors. Fast search: 64 MiB pages and up to 512 MiB vectors. These are caps, not reserved memory; models/workers use additional RAM."));
        ui.separator();
        ui.separator();
        ui.checkbox(&mut self.s.encrypt_index, t("Encrypt database"));
        ui.label(t("Encryption protects filenames, text and vectors using your Windows account; it adds disk/CPU overhead. Portable encrypted data is account-bound. Turning encryption off makes database contents readable."));
        if self.engine.db.key.is_some() != self.s.encrypt_index {
            ui.label(t("Encryption selection is pending. Create a converted copy to apply it. The original is retained for recovery; an old plaintext copy remains readable."));
            if ui
                .add_enabled(
                    !self.indexing && !self.setup_busy,
                    egui::Button::new(t("Create and activate converted copy")),
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
        self.shortcut_settings(ui);
    }

    fn switch_tool(&mut self, tool: &str) {
        if self.tool != tool {
            self.image_generation.fetch_add(1, Ordering::AcqRel);
            self.clear_image_query();
        }
        if !self.saved_settings.tool_available(tool) {
            return;
        }
        self.filter_open = false;
        self.save_search_open = false;
        self.clipboard_texture = None;
        self.clipboard_preview_id = -1;
        if self.tool == "Web" && tool != "Web" {
            self.web.stop();
        }
        self.tool = tool.into();
        if tool == "AI"
            && self.llm_models.is_empty()
            && !self.llm_models_busy
            && !self.saved_settings.llm_url.is_empty()
        {
            self.llm_models_busy = true;
            self.refresh_remote(2, false);
        }
        // Empty tools can have different compact heights even when emptiness stays true.
        self.popup_size_state = None;
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
        if self.ai_busy {
            self.engine.models.cancel_query();
        }
        if self.ai_busy && self.ai_messages.last().is_some_and(|m| m.role == "user") {
            self.ai_messages.pop();
        }
        self.ai_busy = false;
        self.ai_started = None;
        self.ai_job += 1;
    }
    fn ask_ai(&mut self) {
        if !self.saved_settings.ai_chat_available() || self.ai_busy || self.dictation_busy {
            return;
        }
        self.ai_error = None;
        self.ai_answer.clear();
        self.ai_metrics = crate::ai::Metrics::default();
        self.ai_elapsed = None;
        self.ai_follow_bottom = true;
        self.stop_read_aloud();
        self.markdown = egui_commonmark::CommonMarkCache::default();
        self.ai_question = self.query.trim().into();
        let mut selected_settings = self.saved_settings.clone();
        selected_settings.llm_model = self.ai_session_model.clone();
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
                if self.ai_context_trimmed {
                    self.ai_message_thumbs.clear();
                    self.status = "Older turns removed to fit Max context.".into();
                }
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
        self.ai_started = Some(Instant::now());
        self.ai_elapsed = None;
        let cancel = self.ai_cancel.clone();
        // AI uses its saved provider independently of the Web tab's session picker.
        let use_web = self.ai_web_enabled;
        let local_engine = self.engine.clone();
        let use_local = self.ai_local_enabled;
        self.restore_capture_defaults();
        let s = selected_settings;
        let messages = self.ai_messages.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let local = use_local.then(|| {
                let settings = s.clone();
                let stop = cancel.clone();
                Arc::new(move |query: &str| local_engine.ai_local_search(query, &settings, &stop))
                    as Arc<dyn Fn(&str) -> anyhow::Result<serde_json::Value> + Send + Sync>
            });
            let result = crate::ai::ask_with_tools(
                &s,
                &messages,
                &cancel,
                use_web,
                local,
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
    fn set_read_aloud_paused(&mut self, paused: bool) {
        match crate::platform::pause_playback(paused) {
            Ok(()) => self.tts_paused.store(paused, Ordering::Release),
            Err(error) => self.tts_status = error.to_string(),
        }
    }
    fn stop_read_aloud(&mut self) {
        self.tts_cancel.store(true, Ordering::Release);
        self.tts_paused.store(false, Ordering::Release);
        if self.tts_busy {
            crate::platform::stop_playback();
        }
        self.tts_job += 1;
        self.tts_busy = false;
    }
    fn start_read_aloud(&mut self, text: String) {
        if !self.s.tts_enabled || !self.saved_settings.tts_enabled {
            return;
        }
        self.stop_read_aloud();
        self.tts_job += 1;
        let id = self.tts_job;
        self.tts_busy = true;
        self.ai_follow_bottom = true;
        self.tts_status = String::new();
        self.tts_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.tts_cancel.clone();
        let paused = self.tts_paused.clone();
        let s = self.saved_settings.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let result = crate::tts::speak(&s, &text, &cancel, &paused).map_err(|e| e.to_string());
            let _ = tx.send(Event::TtsDone(id, result));
        });
    }
    fn new_conversation(&mut self) {
        self.image_generation.fetch_add(1, Ordering::AcqRel);
        self.restore_capture_defaults();
        self.ai_web_enabled =
            self.saved_settings.ai_web_default && self.saved_settings.web.available();
        self.ai_message_thumbs.clear();
        self.stop_read_aloud();
        self.ai_messages.clear();
        self.ai_images.clear();
        self.ai_attachment = None;
        self.ai_answer.clear();
        self.ai_question.clear();
        self.ai_error = None;
        self.ai_context_trimmed = false;
        self.ai_metrics = crate::ai::Metrics::default();
        self.ai_elapsed = None;
        self.markdown = egui_commonmark::CommonMarkCache::default();
    }
    fn resume_read_aloud(&mut self) {
        if self.tts_paused.swap(false, Ordering::AcqRel) && self.tts_busy {
            if let Err(e) = crate::platform::pause_playback(false) {
                self.stop_read_aloud();
                self.tts_status = e.to_string();
            }
        }
    }
    fn toggle_dictation(&mut self) {
        if self.dictation_busy {
            self.dictation_stop.store(true, Ordering::Release);
            return;
        }
        if self.tts_busy {
            self.tts_paused.store(true, Ordering::Release);
            if let Err(e) = crate::platform::pause_playback(true) {
                self.stop_read_aloud();
                self.tts_status = e.to_string();
            }
        }
        self.dictation_job += 1;
        let id = self.dictation_job;
        self.dictation_stop = Arc::new(AtomicBool::new(false));
        self.dictation_cancel = Arc::new(AtomicBool::new(false));
        self.dictation_busy = true;
        self.query.clear();
        self.changed();
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

    fn popup_dialog_open(&self) -> bool {
        self.shortcut_conflict.is_some()
            || self.clip_edit.is_some()
            || self.capture_open
            || self.save_search_open
            || self.filter_open
            || self.clipboard_clear_confirm
            || self.ai_image_view.is_some()
            || self.show_changelog
            || self.delete_confirm.is_some()
            || self.remove_component.is_some()
            || self.uninstall_confirm
            || self.clear_index_confirm
    }

    fn capture_ui(&mut self, ctx: &egui::Context) {
        if !self.capture_open {
            return;
        }
        let mut open = true;
        dialog_window(ctx, t("Screenshot for AI"), 860.0).min_height((ctx.content_rect().height()-110.0).clamp(100.0,520.0)).open(&mut open).default_width(860.0).resizable(true).show(ctx,|ui|{
            ui.label(t("Choose a window or monitor and review its preview. Attach a screenshot; Send submits it to the selected vision model."));
            if ui.add_enabled(!self.capture_busy,egui::Button::new(t("Select area anywhere on screen"))).clicked(){self.start_area_capture(ctx);}
            let body_height = (ctx.content_rect().height()-220.0).clamp(100.0,330.0);
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(),body_height),egui::Layout::top_down(egui::Align::Min),|ui| {
            ui.columns(2,|columns|{
                egui::ScrollArea::vertical().id_salt("capture-choices").max_height(body_height).auto_shrink([false,false]).show(&mut columns[0],|ui|{
                    for target in &self.capture_targets{
                        if ui.add_enabled_ui(!self.capture_busy, |ui| ui.add_sized([ui.available_width(),34.0], egui::Button::new(tr(&target.label)).truncate().selected(self.capture_target.as_ref().is_some_and(|t|t.id==target.id&&t.window==target.window)))).inner.on_hover_text(tr(&target.label)).clicked(){self.capture_target=Some(target.clone());self.capture_bytes=None;self.capture_texture=None;}
                    }
                });
                let ui=&mut columns[1];
                if let Some(texture)=&self.capture_texture{ui.add(egui::Image::new(texture).max_size(egui::vec2(ui.available_width(),body_height)));}
                else if self.capture_busy{ui.spinner();}else{ui.centered_and_justified(|ui|{ui.label(t("Select a source to preview."));});}
            });
            });
            if let Some(target)=self.capture_target.clone(){
                let key=(target.id,target.window);
                if self.capture_preview_key!=Some(key)&&!self.capture_busy{
                    self.capture_preview_key=Some(key);self.capture_busy=true;self.capture_texture=None;self.capture_bytes=None;
                    let tx=self.engine.tx.clone();std::thread::spawn(move||{let _=tx.send(Event::Screenshot(crate::capture::grab(&target).map_err(|e|e.to_string())));});
                }
            }
            let attach = ui.add_enabled(!self.capture_busy&&self.capture_bytes.is_some(),egui::Button::new(t("Attach screenshot")));
            if self.smoke.is_some() {ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-attach-screenshot"),attach.rect));}
            if attach.clicked(){
                if let Some(bytes)=&self.capture_bytes{match crate::capture::vision(bytes,None){Ok(url)=>{self.restore_capture_defaults();self.ai_images=vec![url];self.ai_attachment=self.capture_texture.clone();self.capture_open=false;},Err(e)=>self.ai_error=Some(e.to_string())}}
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
        if self.tool == "Web" {
            self.web_results(ui, ctx);
            return;
        }
        if self.tool == "AI" {
            let scroll = self.ai_scroll;
            self.ai_scroll = 0.0;
            let output = egui::ScrollArea::vertical()
                .id_salt("ai-answer")
                .stick_to_bottom(self.ai_busy || self.tts_busy)
                .min_scrolled_height(0.0)
                .max_height(ui.available_height().max(0.0))
                .show(ui, |ui| {
                    for (turn, message) in self.ai_messages.iter().enumerate() {
                        ui.push_id(("ai-turn", turn), |ui| {
                            ui.label(
                                RichText::new(if message.role == "user" {
                                    t("You")
                                } else {
                                    t("AI")
                                })
                                .size(20.0)
                                .family(egui::FontFamily::Name("bold".into()))
                                .strong(),
                            );
                            for image in &message.images {
                                let key = image_key(image);
                                if !self.ai_message_thumbs.contains_key(&key) {
                                    if let Ok(decoded) =
                                        image_bytes(image).and_then(|b| crate::capture::rgba(&b))
                                    {
                                        let thumb = image::DynamicImage::ImageRgba8(decoded)
                                            .thumbnail(400, 260)
                                            .to_rgba8();
                                        self.ai_message_thumbs.insert(
                                            key,
                                            ctx.load_texture(
                                                format!("ai-thumb-{key}"),
                                                egui::ColorImage::from_rgba_unmultiplied(
                                                    [
                                                        thumb.width() as usize,
                                                        thumb.height() as usize,
                                                    ],
                                                    thumb.as_raw(),
                                                ),
                                                egui::TextureOptions::LINEAR,
                                            ),
                                        );
                                    }
                                }
                                if let Some(texture) = self.ai_message_thumbs.get(&key) {
                                    let response = ui.add(
                                        egui::Image::new(texture)
                                            .max_size(egui::vec2(200.0, 130.0))
                                            .sense(egui::Sense::click()),
                                    );
                                    if response.clicked() {
                                        self.ai_image_view = Some(image.clone());
                                    }
                                    response.context_menu(|ui| {
                                        if ui.button(t("Copy screenshot")).clicked() {
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
                        });
                    }
                    if self.ai_busy || self.ai_error.is_some() {
                        ui.label(
                            RichText::new(t("AI"))
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
                                .button(t("Copy"))
                                .on_hover_text(t("Copy last response"))
                                .clicked()
                            {
                                ctx.copy_text(self.ai_answer.clone());
                            }
                            if self.saved_settings.tts_enabled
                                && ui
                                    .add(egui::Button::image(crate::icons::image(
                                        crate::icons::Icon::Speaker,
                                        self.accent(),
                                        20.0,
                                    )))
                                    .on_hover_text(if self.tts_busy {
                                        t("Stop read aloud")
                                    } else {
                                        t("Read last response aloud")
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
            if self.smoke.is_some() {
                ctx.data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new("qa-answer-bottom"),
                        output.inner_rect.bottom(),
                    )
                });
            }
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
            self.clipboard_toolbar(ui, ctx);
            if !self.saved_settings.clipboard_enabled {
                ui.weak(t("Capture is off · enable it in Settings"));
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
            let mut columns = preview_columns(
                ui,
                "clipboard-preview-divider",
                &mut self.window_state.clipboard_split,
                0.50,
                height,
            );
            {
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
                        let fill = if self.selected == i || self.clip_selected.contains(&item.id) {
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
                            if item.label == "Copied image" {
                                t("Copied image").to_owned()
                            } else {
                                item.label.clone()
                            },
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
                            tr(&item.detail),
                            egui::FontId::proportional(11.0),
                            ui.visuals().weak_text_color(),
                        );
                        if response.clicked() {
                            self.selected = i;
                            if ctx.input(|i| i.modifiers.ctrl) {
                                if !self.clip_selected.insert(item.id) {
                                    self.clip_selected.remove(&item.id);
                                }
                            } else {
                                self.clip_selected.clear();
                            }
                        }
                        if response.double_clicked() {
                            self.copy_tool_item(ctx, item.clone());
                        }
                        response.context_menu(|ui| {
                            self.clipboard_item_menu(ui, ctx, &item);
                            if ui.button(t("Copy")).clicked() {
                                self.copy_tool_item(ctx, item.clone());
                                ui.close();
                            }
                            if ui.button(t("Remove from history")).clicked() {
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
                    ui.weak(tr(&item.detail));
                    if item.image {
                        if let Some(texture) = &self.clipboard_texture {
                            ui.add(
                                egui::Image::new(texture)
                                    .corner_radius(14)
                                    .fit_to_exact_size(preview_image_size(
                                        texture.size_vec2(),
                                        ui.available_size(),
                                    )),
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
                        ui.label(t("Select a clipboard item to preview it."));
                    });
                }
            }
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
    fn start_connection_test(&mut self, index: usize) {
        self.refresh_remote(if index == 0 { 2 } else { 3 }, false);
        let signature = connection_signature(&self.s, index == 1);
        let state = &mut self.connections[index];
        state.signature = signature;
        state.job += 1;
        state.busy = true;
        state.result = None;
        state.at = None;
        let job = state.job;
        let s = self.s.clone();
        let tx = self.engine.tx.clone();
        std::thread::spawn(move || {
            let result = crate::ai::test_connection(&s, index == 1).map_err(|e| e.to_string());
            let _ = tx.send(Event::ConnectionTest(index, job, result));
        });
    }
    fn connection_row(&mut self, ui: &mut egui::Ui, tts: bool) {
        let index = usize::from(tts);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 225.0).max(120.0);
            ui.add_sized(
                [width, 34.0],
                egui::TextEdit::singleline(if tts {
                    &mut self.s.tts_url
                } else {
                    &mut self.s.llm_url
                }),
            );
            if ui
                .add_enabled(!self.connections[index].busy, egui::Button::new(t("Test")))
                .on_hover_text(t(
                    "Test API connection and authentication using the model-list endpoint",
                ))
                .clicked()
            {
                self.start_connection_test(index);
            }
            if self.connections[index].busy {
                ui.spinner();
            }
            if let Some(result) = &self.connections[index].result {
                match result {
                    Ok(()) => {
                        ui.colored_label(
                            if ui.visuals().dark_mode {
                                Color32::from_rgb(70, 200, 115)
                            } else {
                                Color32::from_rgb(22, 120, 58)
                            },
                            t("Connection Successful"),
                        );
                    }
                    Err(e) => {
                        ui.colored_label(Color32::from_rgb(220, 70, 75), t("Connection failed"))
                            .on_hover_text(e);
                    }
                }
            }
        });
    }
    fn llm(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.models_ai_open {
            self.models_ai_open = true;
            for (index, enabled) in [(0, self.s.remote_embeddings()), (2, self.s.llm_enabled)] {
                if enabled && !self.remote_fields(index).2.is_empty() {
                    self.refresh_remote(index, index == 0);
                }
            }
        }
        let before_llm = self.remote_signature(2);

        ui.heading(t("Models and AI"));
        self.embedding_settings(ui);
        ui.separator();
        self.dictation_settings(ui);
        ui.separator();
        let llm_header = ui.heading(t("LLM Model"));
        if self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-llm-section") {
            ui.scroll_to_rect(llm_header.rect, Some(egui::Align::TOP));
        }
        ui.checkbox(&mut self.s.llm_enabled, t("Enable AI chat"));
        ui.weak(t(
            "Select a model and save settings to show AI in search. MCP works independently.",
        ));
        ui.label(t("Connect your own OpenAI-compatible server. AI requests run only when you submit a question."));
        ui.label(t("Base URL"));
        self.connection_row(ui, false);
        if self.remote_servers[2].context.is_some() {
            ui.weak(&self.remote_servers[2].note);
        }
        ui.weak(t("Example: http://localhost:8080/v1 (include /v1)"));
        secret(ui, t("API key (optional)"), &mut self.s.llm_key);
        ui.horizontal(|ui| {
            ui.label(t("Model"));
            egui::ComboBox::from_id_salt("llm-model")
                .selected_text(if self.s.llm_model.is_empty() {
                    t("Select a model")
                } else {
                    &self.s.llm_model
                })
                .show_ui(ui, |ui| {
                    for model in &self.llm_models {
                        ui.selectable_value(&mut self.s.llm_model, model.clone(), model);
                    }
                });
            if ui
                .add_enabled(
                    !self.llm_models_busy,
                    egui::Button::new(t("Refresh models")),
                )
                .clicked()
            {
                self.llm_models_busy = true;
                self.refresh_remote(2, false);
            }
            if self.llm_models_busy {
                ui.spinner();
            }
        });
        ui.label(t("Or enter a model ID"));
        ui.text_edit_singleline(&mut self.s.llm_model);

        ui.checkbox(
            &mut self.s.llm_vision,
            t("Selected model supports vision (screenshots)"),
        );
        ui.weak(t("Screenshots are attached only after capture and pressing Send / Enter. Your configured server receives the image."));

        ui.checkbox(
            &mut self.s.llm_context_auto,
            t("Detect maximum context automatically"),
        );
        if ui
            .add(
                egui::DragValue::new(&mut self.s.llm_context)
                    .range(256..=1048576)
                    .prefix(t("Max context: "))
                    .suffix(t(" tokens")),
            )
            .changed()
        {
            self.s.llm_context_auto = false;
        }
        if self.s.llm_context_auto
            && !self.remote_servers[2].busy
            && !self.remote_servers[2].note.is_empty()
            && self.remote_servers[2].context.is_none()
        {
            ui.weak(t(
                "Server did not report a context limit; enter it manually.",
            ));
        }
        ui.weak(t("Estimated conversation + response budget, not a server context resize. Tokenizers differ. Old turns are removed automatically when needed."));
        ui.separator();
        ui.heading(t("Voice Model"));
        if ui
            .checkbox(&mut self.s.tts_enabled, t("Enable text to speech"))
            .changed()
            && !self.s.tts_enabled
        {
            self.stop_read_aloud();
        }
        let previous_tts = (self.s.tts_url.clone(), self.s.tts_model.clone());
        let previous_voice = self.s.tts_voice.clone();
        ui.add_enabled_ui(self.s.tts_enabled, |ui| {
        ui.label(t("API base URL"));
        self.connection_row(ui, true);
        ui.weak(t("Example: http://localhost:8880/v1 (include /v1)"));
        secret(ui, t("TTS API key (optional)"), &mut self.s.tts_key);
        if ui
            .add_enabled(
                !self.tts_catalog_busy,
                egui::Button::new(t("Refresh TTS models and voices")),
            )
            .clicked()
        {
            self.tts_catalog_busy = true;
            crate::tts::reset_stream_cache();
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
                t("Select TTS model")
            } else {
                &self.s.tts_model
            })
            .show_ui(ui, |ui| {
                for model in &self.tts_models {
                    ui.selectable_value(&mut self.s.tts_model, model.clone(), model);
                }
            });
        ui.label(t("Model ID"));
        ui.text_edit_singleline(&mut self.s.tts_model);
        egui::ComboBox::from_id_salt("tts-voice")
            .selected_text(if self.s.tts_voice.is_empty() {
                t("Select voice")
            } else {
                &self.s.tts_voice
            })
            .show_ui(ui, |ui| {
                for voice in &self.tts_voices {
                    ui.selectable_value(&mut self.s.tts_voice, voice.clone(), voice);
                }
            });
        ui.label(t("Voice ID"));
        ui.text_edit_singleline(&mut self.s.tts_voice);
        ui.checkbox(&mut self.s.tts_auto, t("Read AI responses aloud by default"));
        ui.weak(t("Generated voice audio. Answers are sent to your TTS server only when read aloud is enabled or requested. Some servers do not offer voice discovery; enter the voice ID manually."));
        });
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
            ui.weak(t("Refreshing voices…"));
        }
        if !self.tts_notice.is_empty() && self.s.tts_voice.trim().is_empty() {
            ui.weak(tr(&self.tts_notice));
        }
        ui.separator();
        ui.heading(t("MCP"));
        ui.checkbox(&mut self.s.mcp_enabled, t("Enable read-only agent access"));
        ui.checkbox(&mut self.s.mcp_lan, t("Allow local-network access"));
        ui.checkbox(
            &mut self.s.mcp_auth,
            t("Require HTTP bearer authentication"),
        );
        ui.add(
            egui::DragValue::new(&mut self.s.mcp_port)
                .range(1024..=65535)
                .prefix(t("Port ")),
        );
        secret(ui, t("Bearer token"), &mut self.s.mcp_token);
        if ui.button(t("Generate token")).clicked() {
            self.s.mcp_token = crate::config::random_key();
        }
        if ui.button(t("Copy MCP connection")).clicked() {
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
        if before_llm != self.remote_signature(2) {
            self.remote_due[2] = Some(Instant::now() + Duration::from_millis(500));
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        ui.label(t(
            "LAN HTTP is unencrypted. Use trusted clients and keep authentication enabled.",
        ));
        ui.separator();
        self.uninstall_button(ui, "Uninstall all local models…", "models");
    }

    fn about(&mut self, ui: &mut egui::Ui) {
        ui.heading(t("About VectorDash"));
        ui.label(tr(&format!(
            "Version {} · Native Rust · Windows 10 (beta) / 11 x64",
            env!("CARGO_PKG_VERSION")
        )));
        ui.label(t(
            "Local universal file search and launcher, powered by EmbeddingGemma 2.",
        ));
        ui.label(t("License: MIT · VectorDash contributors"));
        ui.hyperlink_to(
            "GitHub and support",
            "https://github.com/ampersandru/VectorDash",
        );
        ui.hyperlink_to(
            "License and acknowledgements",
            "https://github.com/ampersandru/VectorDash/blob/main/ACKNOWLEDGEMENTS.md",
        );
        ui.label(t("Developed with AI assistance; verify important results."));
        ui.separator();
        ui.heading(t("Updates"));
        ui.label(tr(&format!(
            "Installed version: {}",
            env!("CARGO_PKG_VERSION")
        )));
        ui.horizontal(|ui| {
            ui.label(t("Update channel"));
            egui::ComboBox::from_id_salt("update-channel")
                .selected_text(t(&self.s.update_channel))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.s.update_channel, "Release".into(), t("Release"));
                    ui.selectable_value(&mut self.s.update_channel, "Beta".into(), t("Beta"));
                });
        });
        ui.weak(t(
            "Release: official releases only. Beta: official releases and pre-releases.",
        ));
        if self.s.update_channel != self.saved_settings.update_channel {
            ui.label(t("Save your update channel before checking for updates."));
        }
        ui.checkbox(
            &mut self.s.auto_check_updates,
            t("Check for updates at startup"),
        );
        let portable = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("portable.flag").exists()))
            .unwrap_or(false);
        if !portable {
            ui.checkbox(
                &mut self.s.auto_update,
                t("Automatically download and install updates when indexing is idle"),
            );
        } else {
            ui.label(t(
                "Portable updates: download the new portable package from the release page.",
            ));
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.update_busy
                        && self.s.update_channel == self.saved_settings.update_channel,
                    egui::Button::new(t("Check for updates")),
                )
                .clicked()
            {
                self.check_update();
            }
            if self.update_release.is_some()
                && !portable
                && self.update_installer.is_none()
                && ui
                    .add_enabled(!self.update_busy, egui::Button::new(t("Download update")))
                    .clicked()
            {
                self.download_update();
            }
            if self.update_installer.is_some()
                && ui
                    .add_enabled(
                        !self.indexing && !self.setup_busy,
                        egui::Button::new(t("Install update and restart")),
                    )
                    .clicked()
            {
                self.install_update(ui.ctx());
            }
            if self.update_release.is_some() && ui.button(t("View changelog")).clicked() {
                self.show_changelog = true;
            }
            if let Some(release) = &self.update_release {
                ui.hyperlink_to("Release notes", &release.page);
            }
        });
        ui.label(tr(&self.update_status));
        ui.label(t("Updates come from the VectorDash GitHub repository and must pass SHA-256 verification. The installer is unsigned."));
        ui.separator();
    }
    fn scheduling(&mut self, ui: &mut egui::Ui) {
        ui.heading(t("Scheduling"));
        ui.checkbox(
            &mut self.s.watch_changes,
            t("Automatically index changed files and folders (respects idle rules)"),
        );
        if self.s.watch_changes {
            ui.label(tr(&self.watch_status));
        } else {
            ui.label(t("File monitoring is off"));
        }
        if self.engine.control.automatic_hold.load(Ordering::Acquire)
            && !self.engine.control.sources_pending.load(Ordering::Acquire)
            && !self
                .engine
                .control
                .restart_requested
                .load(Ordering::Acquire)
            && ui.button(t("Resume automatic indexing")).clicked()
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

        ui.label(t("Automatic file changes obey the idle conditions below, including when the periodic scan schedule is Manual. Manual only disables periodic scans."));
        ui.checkbox(
            &mut self.s.ntfs_journal,
            t("Use NTFS change journal when available"),
        );
        ui.label(t("Requires volume-read permission and an existing NTFS journal. Unsupported or inaccessible drives keep using folder notifications and recovery scans."));
        ui.weak(t("Administrator access may be required to read existing journals. The count shows active readers."));
        ui.weak(t("Missing/expired journal positions or changed sources require catch-up for affected sources. Network drives use folder notifications and source-specific catch-up."));
        ui.checkbox(&mut self.s.always_admin, t("Always start as administrator"));
        ui.weak(t("Save to apply on next launch. Windows will ask for approval. Mapped drives may be unavailable when elevated."));
        if crate::admin::elevated() {
            ui.label(t("Running as administrator"));
        } else if ui.button(t("Relaunch as administrator")).clicked() {
            if self.save() {
                match crate::admin::relaunch(&self.home, &[]) {
                    Ok(()) => {
                        self.exiting = true;
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
        }
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
                ui.label(t("Time (this PC's local time)"));
                ui.add(egui::DragValue::new(&mut self.s.schedule_hour).range(0..=23));
                ui.label(":");
                ui.add(egui::DragValue::new(&mut self.s.schedule_minute).range(0..=59));
            });
            ui.horizontal(|ui| {
                for (i, name) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                    .iter()
                    .enumerate()
                {
                    ui.checkbox(&mut self.s.schedule_days[i], t(name));
                }
            });
        }
        if self.s.schedule == "When idle" {
            ui.add(
                egui::Slider::new(&mut self.s.idle_interval_minutes, 5..=1440)
                    .text(t("Minimum minutes between idle updates")),
            );
        }
        if idle {
            ui.heading(t("Idle conditions"));
            ui.checkbox(
                &mut self.s.check_user_idle,
                t("Require keyboard/mouse inactivity"),
            );
            if self.s.check_user_idle {
                ui.add(
                    egui::Slider::new(&mut self.s.idle_seconds, 30..=7200)
                        .text(t("Inactive seconds")),
                );
            }
            ui.checkbox(&mut self.s.check_cpu, t("Require low CPU usage"));
            if self.s.check_cpu {
                ui.add(
                    egui::Slider::new(&mut self.s.cpu_limit, 1.0..=100.0)
                        .text(t("CPU usage below (%)")),
                );
            }
            ui.checkbox(&mut self.s.check_gpu, t("Require low GPU usage"));
            if self.s.check_gpu {
                ui.add(
                    egui::Slider::new(&mut self.s.gpu_limit, 1.0..=100.0)
                        .text(t("GPU usage below (%)")),
                );
            }
            ui.label(t("Thresholds measure other apps, excluding VectorDash, its model server and extraction helpers. Conditions are checked between files and embedding batches. If GPU usage cannot be read, turn off the GPU condition to allow scheduled indexing."));
        }
        if self.s.schedule != "Manual" || self.s.watch_changes {
            ui.checkbox(
                &mut self.s.ac_only,
                t("Automatic indexing only on AC power"),
            );
            ui.add(
                egui::Slider::new(&mut self.s.max_run_minutes, 5..=1440)
                    .text(t("Maximum automatic run (minutes)")),
            );
        }
        ui.add_space(12.0);
        ui.heading(t("Schedule status"));
        ui.label(tr(&self.schedule_status));
        if idle {
            let metrics = crate::scheduling::metrics();
            ui.label(tr(&format!(
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
            )));
        }
        if self.s.last_success > 0 {
            if let Some(date) = chrono::DateTime::from_timestamp(self.s.last_success, 0) {
                ui.label(tr(&format!(
                    "Last successful update: {}",
                    date.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")
                )));
            }
        }
        ui.collapsing(t("Recent automatic runs"), |ui| {
            for item in &self.s.schedule_history {
                ui.label(item);
            }
        });
        if ui
            .add_enabled(!self.indexing, egui::Button::new(t("Update now")))
            .clicked()
        {
            self.start_index(false);
        }
        ui.label(t("VectorDash must remain running. Closing to tray keeps schedules active; it does not wake a sleeping PC."));
    }
    fn logs(&mut self, ui: &mut egui::Ui) {
        ui.heading(t("Logs"));
        ui.horizontal(|ui| {
            if ui
                .selectable_label(!self.model_logs && !self.file_logs, t("VectorDash logs"))
                .clicked()
            {
                self.model_logs = false;
                self.file_logs = false;
            }
            if ui
                .selectable_label(self.model_logs, t("Model logs"))
                .clicked()
            {
                self.model_logs = true;
                self.file_logs = false;
            }
            if ui.selectable_label(self.file_logs, t("File log")).clicked() {
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
                    ui.checkbox(&mut self.log_filter[i], t(name));
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
                            ui.label(
                                RichText::new(tr(&format!("{level} {text}")))
                                    .monospace()
                                    .color(if i == 2 {
                                        crate::appearance::component_color(
                                            ui.visuals().dark_mode,
                                            false,
                                            true,
                                        )
                                    } else if i == 3 {
                                        Color32::LIGHT_RED
                                    } else {
                                        ui.visuals().text_color()
                                    }),
                            );
                        }
                    }
                });
        }
    }
}
impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        input.events.append(&mut self.qa_web_events);
        if crate::platform::take_image_paste()
            && (self.tool == "AI" && !self.ai_busy
                || matches!(self.tool.as_str(), "Search" | "Clipboard")
                    && self.saved_settings.image_search_enabled)
        {
            input.events.push(egui::Event::Paste(String::new()));
        }
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        crate::i18n::apply(ctx, &self.s.language);
        if self.smoke.is_some()
            && self.frames == 2
            && std::env::args().any(|a| a == "--smoke-tray-exit")
        {
            self.close_window(ctx);
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(400));
                let _ = tx.send(Event::Quit);
            });
        }

        self.events(ctx);
        if self.exiting {
            self.web_smoke_closed_report(ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        use raw_window_handle::HasWindowHandle;
        if self.smoke.is_some()
            && self.startup.elapsed() > Duration::from_secs(2)
            && self.qa_selection.is_some()
            && !self.hits.is_empty()
            && !self.searching
        {
            let repeat = std::env::args().any(|a| a == "--smoke-repeat-search");
            let focus = std::env::args().any(|a| a == "--smoke-ctrl-f");
            if repeat || focus {
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new("qa-reset-selected"), self.selected);
                    d.insert_temp(egui::Id::new("qa-reset-offset"), self.result_scroll_offset);
                });
                self.qa_selection = None;
                if repeat {
                    self.query.push(' ');
                    self.changed();
                } else {
                    ctx.input_mut(|i| {
                        i.modifiers = egui::Modifiers::CTRL;
                        i.events.push(egui::Event::Key {
                            key: egui::Key::F,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::CTRL,
                        });
                    });
                }
            }
        }
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
        if self.smoke.is_some()
            && self.tts_busy
            && std::env::args().any(|a| a == "--smoke-pause-playback")
        {
            let seconds = self.startup.elapsed().as_secs_f64();
            let paused = seconds >= 4.0 && seconds < 8.0;
            if self.tts_paused.load(Ordering::Acquire) != paused {
                self.set_read_aloud_paused(paused);
            }
            if paused && self.tts_paused.load(Ordering::Acquire) {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-pause-seen"), true));
            }
        }
        if self.smoke.is_some()
            && self.startup.elapsed() > Duration::from_secs(3)
            && std::env::args().any(|a| a == "--smoke-filter-roundtrip")
        {
            self.filter_open = false;
        }
        if self.smoke.is_some()
            && self.startup.elapsed() > Duration::from_secs(3)
            && std::env::args().any(|a| a == "--smoke-dialog-close")
        {
            self.capture_open = false;
            self.save_search_open = false;
            self.filter_open = false;
            self.clipboard_clear_confirm = false;
            self.ai_image_view = None;
            self.show_changelog = false;
            self.delete_confirm = None;
            self.remove_component = None;
            self.uninstall_confirm = false;
            self.clear_index_confirm = false;
            self.query.clear();
        }
        if self.smoke.is_some()
            && self.tool == "AI"
            && self.startup.elapsed() > Duration::from_secs(1)
            && std::env::args().any(|a| a == "--smoke-local-ai")
            && !ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new("qa-local-submitted"))
                    .unwrap_or(false)
            })
        {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-local-submitted"), true));
            self.ai_local_enabled = true;
            self.ask_ai();
        }
        if self.smoke.is_some()
            && self.startup.elapsed() > Duration::from_secs(1)
            && std::env::args().any(|a| a == "--smoke-manual-area")
            && !ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new("qa-manual-area"))
                    .unwrap_or(false)
            })
        {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-manual-area"), true));
            self.start_area_capture(ctx);
        }
        self.image_drop_input(ctx);
        self.image_search_input(ctx);
        self.manager_shortcuts(ctx);
        self.web_smoke_tick(ctx);
        if self.smoke.is_some()
            && self.tool == "Web"
            && self.startup.elapsed()
                > Duration::from_secs(if std::env::args().any(|a| a == "--smoke-web-pick") {
                    2
                } else {
                    1
                })
            && std::env::args().any(|a| a == "--smoke-web-enter")
            && !ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new("qa-web-submitted"))
                    .unwrap_or(false)
            })
        {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-web-submitted"), true));
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("query")));
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: Some(egui::Key::Enter),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
            });
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::F)) {
            self.reset_result_selection();
            self.page = 0;
            self.select_query = true;
            self.filter_open = false;
            self.save_search_open = false;
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("query")));
        }
        if self.page == 0
            && self.shortcut_capture.is_none()
            && self.shortcut_conflict.is_none()
            && !self.hotkey_capture
            && !self.llm_hotkey_capture
            && !self.voice_hotkey_capture
            && !self.type_hotkey_capture
            && !self.global_voice_capture
            && !self.filter_open
            && !self.save_search_open
            && !self.capture_open
        {
            let cycle=ctx.input_mut(|i| {
                let found=i.events.iter().any(|e|matches!(e,egui::Event::Key{key,pressed:true,modifiers,..} if crate::shortcuts::matches(&crate::ai::shortcut(*key,*modifiers),&self.saved_settings.llm_hotkey,true)));
                if found{i.events.retain(|e|!matches!(e,egui::Event::Key{key,modifiers,..} if crate::shortcuts::matches(&crate::ai::shortcut(*key,*modifiers),&self.saved_settings.llm_hotkey,true)));}found
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
        if self.type_hotkey_capture {
            let key = ctx.input_mut(|i| {
                let key = i.events.iter().find_map(|e| {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = e
                    {
                        Some((*key, *modifiers))
                    } else {
                        None
                    }
                });
                if key.is_some() {
                    i.events.retain(|e| !matches!(e, egui::Event::Key { .. }));
                }
                key
            });
            if let Some((key, modifiers)) = key {
                if key != egui::Key::Escape {
                    self.s.type_hotkey = crate::ai::shortcut(key, modifiers);
                }
                self.type_hotkey_capture = false;
            }
        }
        if self.frames == 2
            && self.smoke.is_some()
            && std::env::args().any(|a| a == "--smoke-cycle-file-type")
        {
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: Some(egui::Key::Tab),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                })
            });
        }
        if self.page == 0
            && matches!(self.tool.as_str(), "Search" | "Clipboard")
            && !self.type_hotkey_capture
        {
            let cycle=ctx.input_mut(|i|{let found=i.events.iter().any(|e|matches!(e,egui::Event::Key{key,pressed:true,modifiers,..} if crate::ai::shortcut(*key,*modifiers)==self.saved_settings.type_hotkey));if found{i.events.retain(|e|!matches!(e,egui::Event::Key{key,modifiers,..} if crate::ai::shortcut(*key,*modifiers)==self.saved_settings.type_hotkey));}found});
            if cycle {
                if self.tool == "Clipboard" {
                    let filters = ["All", "Pinned items", "Saved", "Text", "Images", "Links"];
                    let i = filters
                        .iter()
                        .position(|f| *f == self.clip_filter)
                        .unwrap_or(0);
                    self.clip_filter = filters[(i + 1) % filters.len()].into();
                    self.changed();
                } else {
                    self.cycle_file_type();
                }
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("query")));
            }
        }
        if self.page == 0 && !self.voice_hotkey_capture {
            let invoke=ctx.input_mut(|i|{
                let found=i.events.iter().any(|e|matches!(e,egui::Event::Key{key,pressed:true,modifiers,..} if crate::ai::shortcut(*key,*modifiers)==self.saved_settings.dictation_hotkey));
                if found{i.events.retain(|e|!matches!(e,egui::Event::Key{key,modifiers,..} if crate::ai::shortcut(*key,*modifiers)==self.saved_settings.dictation_hotkey));}found
            });
            if invoke {
                if self.tool == "AI" {
                    self.read_aloud =
                        self.saved_settings.tts_enabled && self.saved_settings.voice_read_aloud;
                    self.response_length = self.saved_settings.voice_response.clone();
                }
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
                    crate::appearance::WINDOW_RADIUS * ctx.pixels_per_point(),
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
                if std::env::args().any(|a| a == "--smoke-tray-saturated") {
                    let tx = self.engine.tx.clone();
                    let legacy = std::env::args().any(|a| a == "--smoke-tray-legacy");
                    let accent = self.saved_settings.accent;
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(700));
                        let count = tx.smoke_fill_notifications();
                        assert_eq!(count, 512);
                        let _ = crate::platform::smoke_tray_open(tx, legacy, accent);
                    });
                } else if std::env::args().any(|a| a == "--smoke-capture-hidden") {
                    let tx = self.engine.tx.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(700));
                        let _ = tx.send(Event::Shortcut(0, "capture_ai".into()));
                    });
                } else if !std::env::args().any(|a| a == "--smoke-tray") {
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
        if self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-tool-roundtrip") {
            if self.frames == 8 {
                self.switch_tool("AI");
            }
            if self.frames == 15 {
                let height = ctx.content_rect().height();
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("qa-ai-height"), height));
                self.switch_tool("Search");
            }
        }
        if self.page != 2 {
            self.hardware_open = false;
        }
        if self.page != 5 {
            self.models_ai_open = false;
        }
        for index in 0..4 {
            if self.remote_due[index].is_some_and(|t| Instant::now() >= t) {
                self.remote_due[index] = None;
                if !self.remote_fields(index).2.is_empty() {
                    self.refresh_remote(index, index == 0);
                }
            }
        }
        if self.smoke.is_some()
            && self.frames == 3
            && std::env::args().any(|a| a == "--smoke-voice-summon")
        {
            let _ = self.engine.tx.send(Event::VoiceSummon(0));
        }

        if self.frames == 2
            && self.smoke.is_none()
            && (self.saved_settings.auto_check_updates || self.saved_settings.auto_update)
        {
            self.check_update();
        }
        if self.saved_settings.auto_update
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
        for index in 0..2 {
            let signature = connection_signature(&self.s, index == 1);
            let state = &mut self.connections[index];
            if state.signature != signature {
                state.signature = signature;
                state.job += 1;
                state.busy = false;
                state.result = None;
                state.at = None;
            }
            if state
                .at
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(3))
            {
                state.result = None;
                state.at = None;
            }
        }
        let now = Instant::now();
        for (index, value, busy) in [
            (0, &mut self.status, self.indexing || self.setup_busy),
            (1, &mut self.dictation_status, self.dictation_busy),
            (2, &mut self.tts_status, self.tts_busy),
        ] {
            if let Some(delay) = expire_notice(value, &mut self.status_clocks[index], busy, now) {
                ctx.request_repaint_after(delay);
            }
        }
        for (index, notice) in [
            (3, &mut self.search_error),
            (4, &mut self.ai_error),
            (5, &mut self.model_warm_error),
        ] {
            if let Some(value) = notice.as_mut() {
                if let Some(delay) =
                    expire_notice(value, &mut self.status_clocks[index], false, now)
                {
                    ctx.request_repaint_after(delay);
                }
                if value.is_empty() {
                    *notice = None;
                }
            } else {
                self.status_clocks[index] = None;
            }
        }
        if !self.saved_settings.tool_available(&self.tool) {
            if self.tool == "AI" {
                self.stop_ai();
            }
            self.switch_tool("Search");
        }
        if !crate::platform::main_window_visible()
            || ctx.input(|i| i.viewport().minimized.unwrap_or(false))
        {
            if self.tts_busy && !self.capture_open {
                self.stop_read_aloud();
            }
            self.web.stop();
            if self.search_visible {
                self.engine.log("Info",&format!("Window hidden: releasing {} file thumbnails, {} clipboard thumbnails, {} AI thumbnails and {:.1} MiB semantic cache",self.thumbs.len(),self.clipboard_thumbs.len(),self.ai_message_thumbs.len(),self.engine.vectors.bytes() as f64/1048576.0));
                self.thumbs.clear();
            }
            self.texture = None;
            self.clipboard_texture = None;
            self.clipboard_thumbs.clear();
            self.ai_message_thumbs.clear();
            self.web_thumbs.clear();
            self.web_thumb_pending.clear();
            self.clipboard_preview_id = -1;
            self.color_emoji.clear(ctx);
            self.engine.vectors.clear();
            self.preview_pending = None;
            self.engine.models.cancel_query();
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
        let copy_web_request = copy_event
            && self.page == 0
            && self.tool == "Web"
            && !self.web.hits.is_empty()
            && (!ctx.wants_keyboard_input() || query_focus && !query_selection);
        if copy_web_request {
            if let Some(hit) = self.web.hits.get(self.selected) {
                ctx.copy_text(hit.url.clone());
            }
        }
        let copy_file_request = copy_event
            && self.page == 0
            && self.tool == "Search"
            && !self.query.trim().is_empty()
            && !self.hits.is_empty()
            && (!ctx.wants_keyboard_input() || query_focus && !query_selection);
        if copy_ai_request {
            ctx.copy_text(self.ai_answer.clone());
        }
        if copy_file_request || copy_tool_request || copy_ai_request || copy_web_request {
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
        if !crate::platform::window_moving() {
            if self.position_mode != Some(self.quick) {
                let _ = self.window_state.save(&self.home);
                let physical = if self.quick {
                    self.window_state.physical_popup
                } else {
                    self.window_state.physical_main
                };
                if let Some(point) = physical {
                    crate::platform::position_main_window(point);
                } else if let Some(pos) = if self.quick {
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
                if let Some(physical) = crate::platform::main_window_position() {
                    if self.quick {
                        self.window_state.physical_popup = Some(physical);
                    } else {
                        self.window_state.physical_main = Some(physical);
                    }
                }
                if point.iter().all(|v| v.is_finite()) {
                    if self.quick {
                        self.window_state.popup = Some(point);
                    } else {
                        self.window_state.main = Some(point);
                    }
                }
            }
        }
        crate::platform::clear_drag_regions();
        if self.quick {
            crate::platform::add_drag_region(
                [16.0, 0.0, ctx.content_rect().right() - 16.0, 9.0],
                ctx.pixels_per_point(),
            );
        }
        if !self.quick {
            self.titlebar(ctx);
        }
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(20, 9)))
            .show(ctx, |ui| {
                if self.smoke.is_some() {
                    ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::new("qa-footer-top"), ui.max_rect().top() - 9.0)
                    });
                }
                self.draw_footer(ui, ctx);
            });
        if !self.quick {
            egui::SidePanel::left("navigation")
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 18)))
                .exact_width(if self.collapsed { 68.0 } else { 204.0 })
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("nav-pages")
                        .max_height((ui.available_height() - 48.0).max(100.0))
                        .show(ui, |ui| {
                            for i in [0, 1, 2, 3, 5, 8, 9, 4, 6, 7] {
                                let label = &t(PAGES[i]);
                                let icon = [
                                    crate::icons::Icon::Search,
                                    crate::icons::Icon::Sources,
                                    crate::icons::Icon::Models,
                                    crate::icons::Icon::Schedule,
                                    crate::icons::Icon::Settings,
                                    crate::icons::Icon::Robot,
                                    crate::icons::Icon::Logs,
                                    crate::icons::Icon::About,
                                    crate::icons::Icon::Web,
                                    crate::icons::Icon::Clipboard,
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
                                    .on_hover_text(t(label));
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
                            .on_hover_text(tr(&format!("{} VectorDash", t("Exit"))))
                        } else {
                            ui.button(tr(&format!("{} VectorDash", t("Exit"))))
                        };
                        if exit.clicked() {
                            self.exiting = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
            let qa_outside = self.web_smoke_outside(ctx);
            let outside_click = outside_down && !self.popup_outside_down && !focused || qa_outside;
            self.popup_outside_down = outside_down;
            if (self.ai_busy
                || self.web.busy
                || self.tts_busy
                || self.dictation_busy
                || self.popup_dialog_open())
                && !focused
            {
                self.popup_had_focus = false;
            }
            if (self.popup_had_focus && !focused || outside_click)
                && (qa_outside
                    || !(self.smoke.is_some()
                        && std::env::args().any(|a| a == "--smoke-keep-open")))
                && !self.large_preview
                && !self.ai_busy
                && !self.web.busy
                && !self.tts_busy
                && !self.dictation_busy
                && !self.popup_dialog_open()
            {
                self.popup_had_focus = false;
                self.engine.models.cancel_query();
                self.search_visible = false;
                self.engine
                    .models
                    .session_override
                    .store(false, Ordering::Release);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            let empty = match self.tool.as_str() {
                "Search" => self.hits.is_empty(),
                "Web" => self.web.hits.is_empty(),
                "AI" => {
                    self.ai_messages.is_empty()
                        && self.ai_answer.is_empty()
                        && self.ai_error.is_none()
                }
                _ => false,
            };
            let expansion =
                ctx.animate_bool_with_time(egui::Id::new("popup-expansion"), !empty, 0.18);
            let panel_height =
                if self.capture_open || self.ai_image_view.is_some() || self.show_changelog {
                    640
                } else if self.popup_dialog_open() || egui::Popup::is_any_open(ctx) {
                    500
                } else {
                    0
                };
            let image_extra = if self.image_query.is_some() || self.image_preparing {
                64.0
            } else {
                0.0
            };
            if !crate::platform::window_moving()
                && (self.popup_size_state != Some(empty)
                    || self.popup_panel_height != panel_height
                    || (expansion > 0.0 && expansion < 1.0))
            {
                self.popup_panel_height = panel_height;
                self.popup_size_state = Some(empty);
                ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                    780.0,
                    if self.tool == "AI" {
                        194.0 + image_extra
                    } else if self.tool == "Search" {
                        224.0 + image_extra
                    } else {
                        194.0 + image_extra
                    },
                )));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    940.0,
                    if self.tool == "Web" {
                        if empty {
                            if self.tool == "Search" {
                                224.0_f32
                            } else {
                                194.0_f32
                            }
                        } else {
                            640.0_f32
                        }
                    } else if self.tool == "AI" {
                        194.0 + 446.0 * expansion
                    } else {
                        if self.tool == "Search" {
                            224.0 + 416.0 * expansion
                        } else {
                            194.0 + 446.0 * expansion
                        }
                    }
                    .max(panel_height as f32)
                        + image_extra,
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
                        ui.label(t("Unsaved changes"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !self.index_clearing,
                                    egui::Button::new(t("Undo"))
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
                                        RichText::new(t("Save")).strong().color(Color32::WHITE),
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
            .frame(egui::Frame::NONE.inner_margin(if self.quick {
                egui::Margin::symmetric(20, 9)
            } else {
                egui::Margin::same(20)
            }))
            .show(ctx, |ui| {
                if self.page == 0 {
                    self.search_ui(ui, ctx);
                } else {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, true])
                        .show(ui, |ui| match self.page {
                            1 => self.sources(ui),
                            2 => self.models(ui),
                            3 => self.scheduling(ui),
                            4 => self.settings(ui, ctx),
                            5 => self.llm(ui, ctx),
                            6 => self.logs(ui),
                            8 => self.web_settings(ui),
                            9 => self.clipboard_settings(ui),
                            _ => self.about(ui),
                        });
                }
            });
        if self.page == 0
            && self.tool == "Search"
            && self.s.picture_grid
            && matches!(self.category.as_str(), "Images" | "Videos")
            && !ctx.memory(|m| m.has_focus(egui::Id::new("query")))
        {
            let delta = ctx.input(|i| {
                if i.key_pressed(egui::Key::ArrowRight) {
                    1
                } else if i.key_pressed(egui::Key::ArrowLeft) {
                    -1
                } else {
                    0
                }
            });
            if delta != 0 {
                self.selected = self
                    .selected
                    .saturating_add_signed(delta)
                    .min(self.hits.len().saturating_sub(1));
                self.texture = None;
                self.preview_pending = None;
                self.preview_error = None;
                self.scroll_to_selection = true;
            }
        }
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
            } else if self.tool == "Search"
                && self.s.picture_grid
                && matches!(self.category.as_str(), "Images" | "Videos")
            {
                self.grid_columns.max(1)
            } else {
                1
            };
            self.selected = (self.selected + step).min(
                if self.tool == "Search" {
                    self.hits.len()
                } else {
                    if self.tool == "Web" {
                        self.web.hits.len()
                    } else {
                        self.tool_items.len()
                    }
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
            } else if self.tool == "Search"
                && self.s.picture_grid
                && matches!(self.category.as_str(), "Images" | "Videos")
            {
                self.grid_columns.max(1)
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
            && (!self.query.trim().is_empty()
                || self.image_query.is_some()
                || self.image_preparing
                || self.tool == "Emoji"
                || self.tool == "Clipboard")
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
            } else if self.web.busy {
                self.web.stop();
                self.web.error = Some(t("Web search stopped").into());
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
            paint_activity_outline(
                ctx,
                ActivityTrail::Microphone,
                self.s.activity_colors("Microphone"),
            );
        } else if self.web.busy && self.search_visible {
            paint_activity_outline(ctx, ActivityTrail::Web, self.s.activity_colors("Web"));
        } else if (self.ai_busy
            || self.smoke.is_some() && std::env::args().any(|a| a == "--smoke-ai-processing"))
            && self.search_visible
        {
            paint_activity_outline(ctx, ActivityTrail::Ai, self.s.activity_colors("AI"));
        } else if (self.embedding_activity()
            || self.smoke.is_some()
                && std::env::args().any(|a| a == "--smoke-embedding-processing"))
            && self.search_visible
        {
            paint_activity_outline(
                ctx,
                ActivityTrail::Embedding,
                self.s.activity_colors(&self.tool),
            );
        }
        self.manager_dialogs(ctx);
        self.capture_ui(ctx);
        if let Some(image) = self.ai_image_view.clone() {
            let mut open = true;
            dialog_window(ctx, t("Sent screenshot"), 700.0)
                .min_height((ctx.content_rect().height() - 110.0).clamp(100.0, 520.0))
                .open(&mut open)
                .default_width(700.0)
                .show(ctx, |ui| {
                    if let Ok(bytes) = image_bytes(&image) {
                        ui.add(
                            egui::Image::from_bytes(
                                format!("bytes://ai-screenshot-{}.jpg", image_key(&image)),
                                bytes,
                            )
                            .max_size(egui::vec2(
                                ui.available_width(),
                                (ctx.content_rect().height() - 180.0).clamp(100.0, 440.0),
                            )),
                        );
                    }
                    let copy = ui.button(t("Copy screenshot"));
                    if self.smoke.is_some() {
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new("qa-copy-screenshot"), copy.rect)
                        });
                    }
                    if copy.clicked() {
                        self.run(copy_ai_image(&image));
                    }
                });
            if !open {
                ctx.forget_image(&format!("bytes://ai-screenshot-{}.jpg", image_key(&image)));
                self.ai_image_view = None;
            }
        }
        if let Some(key) = self.remove_component.clone() {
            dialog_window(ctx, t("Uninstall downloaded item?"), 440.0).collapsible(false).resizable(false).show(ctx,|ui|{
                ui.label(tr(&format!("Remove the downloaded {key} files? Index data is kept. Active indexing and model work will stop.")));
                ui.horizontal(|ui|{
                    if ui.button(t("Uninstall")).clicked(){
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
                    if ui.button(t("Cancel")).clicked(){self.remove_component=None;}
                });
            });
        }
        if self.save_search_open {
            let mut open = true;
            dialog_window(ctx, t("Save search"), 340.0)
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(340.0)
                .show(ctx, |ui| {
                    ui.label(t("Name"));
                    ui.text_edit_singleline(&mut self.save_search_name);
                    ui.weak(&self.query);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.save_search_name.trim().is_empty()
                                    && !self.query.trim().is_empty(),
                                egui::Button::new(t("Save")),
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
                        if ui.button(t("Cancel")).clicked() {
                            self.save_search_open = false;
                        }
                    });
                });
            self.save_search_open &= open;
        }
        if self.clipboard_clear_confirm {
            dialog_window(ctx, t("Clear clipboard history?"), 440.0).collapsible(false).resizable(false).show(ctx,|ui|{
                ui.label(t("Remove all saved clipboard entries? Your current Windows clipboard is kept."));
                ui.horizontal(|ui|{
                    if ui.button(t("Clear history")).clicked(){self.clipboard_clear_confirm=false;let store=crate::launcher::Clipboard::new(&self.home);let tx=self.engine.tx.clone();std::thread::spawn(move||{let _=tx.send(Event::ClipboardCleared(store.clear().map_err(|e|e.to_string())));});}
                    if ui.button(t("Cancel")).clicked(){self.clipboard_clear_confirm=false;}
                });
            });
        }
        if self.show_changelog {
            let mut open = true;
            dialog_window(ctx, t("Changes since your installed version"), 720.0)
                .id(egui::Id::new("update-changelog"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size([720.0, 520.0])
                .vscroll(true)
                .show(ctx, |ui| {
                    ui.label(tr(&format!("Installed: {}", env!("CARGO_PKG_VERSION"))));
                    if let Some(release) = &self.update_release {
                        for note in &release.changes {
                            ui.separator();
                            ui.heading(&note.title);
                            ui.label(tr(&format!("Version {} · {}", note.version, note.date)));
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
                        dialog_window(child, t("Quick Look"), 700.0)
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
            dialog_window(ctx, t("Delete selected item?"), 440.0)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(&hit.path);
                    ui.horizontal(|ui| {
                        if ui.button(t("Move to Recycle Bin")).clicked() {
                            self.run(crate::platform::recycle(&hit.path));
                            self.delete_confirm = None;
                            self.changed();
                        }
                        if ui.button(t("Delete permanently")).clicked() {
                            self.run(crate::platform::remove(&hit.path));
                            self.delete_confirm = None;
                            self.changed();
                        }
                        if ui.button(t("Cancel")).clicked() {
                            self.delete_confirm = None;
                        }
                    });
                });
        }
        if self.uninstall_confirm {
            dialog_window(ctx, t("Uninstall downloaded models and runtimes?"), 440.0).collapsible(false).show(ctx,|ui|{
                ui.label(t("Remove managed downloads from the current and legacy VectorDash folders? Source files, external tools and the index are kept. Automatic indexing will pause."));
                if ui.button(t("Uninstall downloads")).clicked(){self.uninstall_downloads();}
                if ui.button(t("Cancel")).clicked(){self.uninstall_confirm=false;}
            });
        }
        if self.clear_index_confirm {
            dialog_window(ctx, t("Clear entire index?"), 440.0).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(t("Remove all indexed names, document/OCR text, embeddings and indexing history from the active database?"));
                ui.label(t("Your original files, sources, settings and downloaded models are kept. Retained conversion/import backups are not deleted."));
                ui.label(t("Automatic indexing will pause. Remove unwanted sources or add exclusions before rebuilding."));
                ui.horizontal(|ui| {
                    if ui.button(t("Clear entire index")).clicked() {
                        self.clear_index_confirm = false; self.index_clearing = true; self.setup_busy = true;
                        self.sequence += 1; self.hits.clear(); self.texture = None; self.large_preview = false;
                        self.status = "Stopping indexing and clearing the active index…".into();
                        self.engine.clear_index();
                    }
                    if ui.button(t("Cancel")).clicked() { self.clear_index_confirm = false; }
                });
            });
        }
        if let Some(report) = &self.smoke {
            if self.startup.elapsed() > Duration::from_secs(self.smoke_seconds)
                && (self.qa_screenshot.is_none() || self.qa_captured)
            {
                let mut data = serde_json::json!({"ok":true,"native_rust":true,"tool":self.tool,"tools":self.saved_settings.search_tools(),"footer":self.footer_text(),"embedding_activity":self.embedding_activity(),"ai_elapsed":self.ai_elapsed,"dictation_status":self.dictation_status,"query":self.query,"ai_busy":self.ai_busy,"ai_answer":self.ai_answer,"model_loaded":self.engine.models.loaded.load(Ordering::Relaxed),"frames":self.frames,"max_frame_interval_ms":self.max_frame_ms,"dark_mode":ctx.style().visuals.dark_mode,"hidden_reopened":self.qa_hidden_reopened,"ocr_enabled":self.s.ocr_enabled,"ocr_setup_selected":self.ocr_setup,"results":self.hits.len(),"selected":self.selected,"result_scroll_offset":self.result_scroll_offset,"scope":self.scope,"quick_look":self.large_preview,"quick_look_size":self.qa_child_size,"cpu":crate::scheduling::metrics().cpu,"gpu":crate::scheduling::metrics().gpu,"query_focus":ctx.memory(|m|m.has_focus(egui::Id::new("query"))),"ai_metrics":self.ai_metrics.label(),"tts_busy":self.tts_busy,"tts_paused":self.tts_paused.load(Ordering::Acquire),"connection_results":self.connections.iter().map(|s|s.result.as_ref().map(|r|r.is_ok())).collect::<Vec<_>>(),"ai_scroll_offset":self.ai_scroll_offset,"qa_ai_scroll_before":self.qa_ai_scroll_before,"qa_ai_followup_sent":self.qa_ai_followup_sent,"tts_status":self.tts_status,"tts_notice":self.tts_notice,"tts_voices":self.tts_voices,"response_length":self.response_length,"toolbar_top":self.qa_toolbar_top,"filters_top":self.qa_filter_top,"query_selected":egui::text_edit::TextEditState::load(ctx,egui::Id::new("query")).and_then(|s|s.cursor.char_range()).map(|r|r.as_sorted_char_range().len()).unwrap_or(0)});
                data["manager"] = serde_json::json!({"capture_attached":!self.ai_images.is_empty(),"ai_binding":self.s.binding("ai"),"web_binding":self.s.binding("web"),"conflict":self.shortcut_conflict,"clip_items":self.tool_items.len(),"clip_labels":self.tool_items.iter().take(10).map(|i|&i.label).collect::<Vec<_>>(),"page":self.page});
                data["monitor_moves"] = serde_json::json!(ctx.data(|d| {
                    d.get_temp::<Vec<([i32; 2], [i32; 2])>>(egui::Id::new("qa-monitor-moves"))
                        .unwrap_or_default()
                }));
                data["clipboard_texture_size"] =
                    serde_json::json!(self.clipboard_texture.as_ref().map(|t| t.size()));
                data["quick"] = serde_json::json!(self.quick);
                data["window_visible"] = serde_json::json!(crate::platform::main_window_visible());
                data["ai_error"] = serde_json::json!(self.ai_error);
                data["manager"]["capture_busy"] = serde_json::json!(self.capture_busy);
                data["manager"]["capture_open"] = serde_json::json!(self.capture_open);
                data["manager"]["clip_filter"] = serde_json::json!(self.clip_filter);
                data["manager"]["category"] = serde_json::json!(self.category);
                data["manager"]["ai_image_turns"] = serde_json::json!(
                    self.ai_messages
                        .iter()
                        .filter(|m| !m.images.is_empty())
                        .count()
                );
                data["manager"]["ai_thumbnail_count"] =
                    serde_json::json!(self.ai_message_thumbs.len());

                data["web"] = serde_json::json!({"busy":self.web.busy,"results":self.web.hits,"provider":self.web.provider,"error":self.web.error,"ms":self.web.elapsed_ms,"opened":ctx.data(|d|d.get_temp::<String>(egui::Id::new("qa-web-opened"))),"popup_protected":self.web.busy,"window_visible":self.search_visible,"outside_during":ctx.data(|d|d.get_temp::<bool>(egui::Id::new("qa-web-outside-during")).unwrap_or(false)),"outside_after":ctx.data(|d|d.get_temp::<bool>(egui::Id::new("qa-web-outside-after")).unwrap_or(false)),"provider_menu_last":ctx.data(|d|d.get_temp::<egui::Rect>(egui::Id::new("qa-web-provider-last"))).map(|r|[r.min.x,r.min.y,r.max.x,r.max.y])});
                data["image_query_ready"] = serde_json::json!(self.image_query.is_some());
                data["clipboard_image_results"] = serde_json::json!(
                    self.tool_items
                        .iter()
                        .map(|h| (h.id, h.image))
                        .collect::<Vec<_>>()
                );
                data["image_search_submitted"] = serde_json::json!(self.image_search_submitted);
                data["image_results"] = serde_json::json!(
                    self.hits
                        .iter()
                        .map(|h| (&h.path, h.score))
                        .collect::<Vec<_>>()
                );
                data["ai_local_enabled"] = serde_json::json!(self.ai_local_enabled);
                data["ai_web_enabled"] = serde_json::json!(self.ai_web_enabled);
                data["capture_defaults_pending"] =
                    serde_json::json!(self.ai_capture_restore.is_some());
                data["web_columns"] =
                    serde_json::json!(
                        ctx.data(|d| d.get_temp::<(egui::Rect, Option<egui::Rect>)>(
                            egui::Id::new("qa-web-columns")
                        ))
                        .map(|(text, image)| (
                            [text.left(), text.top(), text.right(), text.bottom()],
                            image.map(|r| [r.left(), r.top(), r.right(), r.bottom()])
                        ))
                    );
                data["clipboard_settings_width"] = serde_json::json!(ctx.data(|d| {
                    d.get_temp::<(f32, f32)>(egui::Id::new("qa-clipboard-settings-width"))
                }));
                data["main_visible"] = serde_json::json!(crate::platform::main_window_visible());
                data["tray_color"] = serde_json::json!(crate::tray::current().color());
                data["administrator"] = serde_json::json!(crate::admin::elevated());
                data["always_admin"] = serde_json::json!(self.s.always_admin);
                data["selection_before_reset"] = serde_json::json!(
                    ctx.data(|d| d.get_temp::<usize>(egui::Id::new("qa-reset-selected")))
                );
                data["scroll_before_reset"] = serde_json::json!(
                    ctx.data(|d| d.get_temp::<f32>(egui::Id::new("qa-reset-offset")))
                );
                data["language"] = serde_json::json!(self.s.language);
                data["localized_search"] = serde_json::json!(t("Search"));
                data["source_list"] = serde_json::json!(
                    ctx.data(|d| d.get_temp::<[f32; 2]>(egui::Id::new("qa-inclusion")))
                );
                data["exclusion_list"] = serde_json::json!(
                    ctx.data(|d| d.get_temp::<[f32; 2]>(egui::Id::new("qa-exclusion")))
                );
                data["answer_bottom"] = serde_json::json!(
                    ctx.data(|d| d.get_temp::<f32>(egui::Id::new("qa-answer-bottom")))
                );
                for (key, id) in [
                    ("preview_pane", "qa-preview-pane"),
                    ("footer_status", "qa-footer-status"),
                    ("footer_row", "qa-footer-row"),
                    ("filter_bounds", "qa-filter-bounds"),
                    ("attach_screenshot", "qa-attach-screenshot"),
                    ("copy_screenshot", "qa-copy-screenshot"),
                    ("type_last_choice", "qa-type-last"),
                    ("playback_pause", "qa-playback-pause"),
                    ("open_full_button", "qa-open-full"),
                    ("footer_text_rect", "qa-footer-text"),
                    ("preview_image", "qa-preview-image"),
                    ("search_body", "qa-search-body"),
                ] {
                    data[key] = serde_json::json!(
                        ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(id)))
                            .map(|r| [r.left(), r.top(), r.right(), r.bottom()])
                    );
                }
                data["preview_fit_bounds"] = serde_json::json!(
                    ctx.data(|d| d.get_temp::<egui::Vec2>(egui::Id::new("qa-preview-fit-bounds")))
                        .map(|v| [v.x, v.y])
                );
                data["footer_top"] = serde_json::json!(
                    ctx.data(|d| d.get_temp::<f32>(egui::Id::new("qa-footer-top")))
                );
                data["ai_compact_height"] = serde_json::json!(
                    ctx.data(|d| d.get_temp::<f32>(egui::Id::new("qa-ai-height")))
                );
                data["window_size"] =
                    serde_json::json!([ctx.content_rect().width(), ctx.content_rect().height()]);
                let mut dialogs = std::collections::BTreeMap::new();
                for title in [
                    "Screenshot for AI",
                    "Sent screenshot",
                    "Uninstall downloaded item?",
                    "Save search",
                    "Clear clipboard history?",
                    "Delete selected item?",
                    "Uninstall downloaded models and runtimes?",
                    "Clear entire index?",
                ] {
                    if let Some(rect) =
                        ctx.memory(|m| m.area_rect(egui::Id::new(("app-dialog", title))))
                    {
                        dialogs.insert(
                            title,
                            [rect.left(), rect.top(), rect.right(), rect.bottom()],
                        );
                    }
                }
                if let Some(rect) = ctx.memory(|m| m.area_rect(egui::Id::new("update-changelog"))) {
                    dialogs.insert(
                        "Changes since your installed version",
                        [rect.left(), rect.top(), rect.right(), rect.bottom()],
                    );
                }
                data["dialogs"] = serde_json::json!(dialogs);
                data["filter_open"] = serde_json::json!(self.filter_open);
                data["qa_pause_seen"] = serde_json::json!(
                    ctx.data(|d| d.get_temp::<bool>(egui::Id::new("qa-pause-seen")))
                        .unwrap_or(false)
                );
                data["web_thumbnail_count"] =
                    serde_json::json!(self.web_thumbs.values().filter(|v| v.is_some()).count());
                data["thumbnail_count"] = serde_json::json!(self.thumbs.len());
                data["category"] = serde_json::json!(self.category);
                data["remote_context"] = serde_json::json!(self.s.remote_context);
                data["llm_context"] = serde_json::json!(self.s.llm_context);
                data["remote_vision"] = serde_json::json!(self.remote_servers[0].vision);
                data["remote_dimensions"] = serde_json::json!(self.remote_servers[0].dimensions);
                data["read_aloud"] = serde_json::json!(self.read_aloud);
                data["tts_enabled"] = serde_json::json!(self.saved_settings.tts_enabled);
                data["remote_status"] = serde_json::json!(
                    self.remote_servers
                        .iter()
                        .map(|r| r.note.clone())
                        .collect::<Vec<_>>()
                );
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
fn cache_chat_models(home: &Path, url: &str, models: &[String]) {
    let value = serde_json::json!({"url":url,"models":models});
    let _ = std::fs::write(
        home.join("llm-model-catalog.json"),
        serde_json::to_vec(&value).unwrap(),
    );
}
fn load_chat_models(home: &Path, url: &str) -> Vec<String> {
    let value = std::fs::read(home.join("llm-model-catalog.json"))
        .ok()
        .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok());
    value
        .filter(|v| v["url"].as_str() == Some(url))
        .and_then(|v| serde_json::from_value(v["models"].clone()).ok())
        .unwrap_or_default()
}
fn chat_model_choices(current: &str, discovered: &[String]) -> Vec<String> {
    let mut models = Vec::new();
    for value in std::iter::once(current).chain(discovered.iter().map(String::as_str)) {
        if !value.is_empty() && !models.iter().any(|item| item == value) {
            models.push(value.to_owned());
        }
    }
    models
}

fn dialog_window<'a>(ctx: &egui::Context, title: &'a str, width: f32) -> egui::Window<'a> {
    let bounds = ctx.content_rect().shrink(12.0);
    let width = width.min((bounds.width() - 24.0).max(120.0));
    egui::Window::new(title)
        .id(egui::Id::new(("app-dialog", title)))
        .constrain_to(bounds)
        .default_pos(egui::pos2(
            (bounds.center().x - width * 0.5).max(bounds.left()),
            bounds.top(),
        ))
        .default_width(width)
        .min_width(120.0)
        .max_width((bounds.width() - 24.0).max(120.0))
        .min_height(0.0)
        .max_height((bounds.height() - 48.0).max(40.0))
        .vscroll(true)
}

fn combo(ui: &mut egui::Ui, id: &str, value: &mut String, items: &[&str]) -> egui::Response {
    egui::ComboBox::from_id_salt(id)
        .selected_text(t(if *value == "Both" {
            "Hybrid"
        } else if *value == "Names" {
            "File/Folder Names"
        } else {
            value.as_str()
        }))
        .show_ui(ui, |ui| {
            for item in items {
                ui.selectable_value(
                    value,
                    (*item).into(),
                    t(if *item == "Both" {
                        "Hybrid"
                    } else if *item == "Names" {
                        "File/Folder Names"
                    } else {
                        *item
                    }),
                );
            }
        })
        .response
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(t(label));
        ui.add(egui::TextEdit::singleline(value).desired_width(360.0));
    });
}
fn secret(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(t(label));
        ui.add(
            egui::TextEdit::singleline(value)
                .password(true)
                .desired_width(360.0),
        );
    });
}
fn path_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(t(label));
        ui.add(egui::TextEdit::singleline(value).desired_width(360.0));
        if ui.small_button(t("Browse…")).clicked() {
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
    // A popup can still have a zero-height body when results arrive. Align the
    // selected row's top rather than scrolling its bottom past the viewport.
    if height <= row {
        return top;
    }
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
    fn collapsed_popup_keeps_first_grid_row_visible() {
        assert_eq!(super::selection_offset(0, 158.0, 0.0, 0.0), 0.0);
        assert_eq!(super::selection_offset(0, 158.0, 474.0, 80.0), 0.0);
        assert_eq!(super::selection_offset(2, 158.0, 0.0, 80.0), 316.0);
    }
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
                    .selected_text(crate::i18n::t("Saved searches"))
                    .show_ui(ui, |ui| {
                        let _ = ui.button(crate::i18n::t("Saved"));
                    });
                let _ = ui.button(crate::i18n::t("Save search"));
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
                    .selected_text(crate::i18n::t("Saved searches"))
                    .show_ui(ui, |ui| {
                        let _ = ui.button(crate::i18n::t("Saved"));
                    });
                let _ = ui.button(crate::i18n::t("Save search"));
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
                    .selected_text(crate::i18n::t("Saved searches"))
                    .show_ui(ui, |ui| {
                        let _ = ui.button(crate::i18n::t("Saved"));
                    });
                let _ = ui.button(crate::i18n::t("Save search"));
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
    Embedding,
    Web,
}
fn trail_style(kind: ActivityTrail, behind: f32, colors: [[u8; 3]; 2]) -> Option<(Color32, f32)> {
    if !(0.0..ACTIVITY_TRAIL_LENGTH).contains(&behind) {
        return None;
    }
    let position = behind / ACTIVITY_TRAIL_LENGTH;
    let [head, tail] = colors.map(|c| c.map(|v| v as f32));
    let mix = if matches!(kind, ActivityTrail::Microphone) {
        ((position - 0.5) * 2.0).clamp(0.0, 1.0)
    } else {
        position
    };
    let color = Color32::from_rgb(
        (head[0] + (tail[0] - head[0]) * mix) as u8,
        (head[1] + (tail[1] - head[1]) * mix) as u8,
        (head[2] + (tail[2] - head[2]) * mix) as u8,
    );
    Some((color, (1.0 - position).powf(1.3)))
}
fn paint_activity_outline(ctx: &egui::Context, kind: ActivityTrail, colors: [[u8; 3]; 2]) {
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
        if let Some((color, intensity)) = trail_style(kind, (head - phase).rem_euclid(1.0), colors)
        {
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
    fn trail_style(kind: ActivityTrail, behind: f32) -> Option<(Color32, f32)> {
        let mode = match kind {
            ActivityTrail::Microphone => "Microphone",
            ActivityTrail::Ai => "AI",
            ActivityTrail::Web => "Web",
            _ => "Search",
        };
        super::trail_style(kind, behind, Settings::default().activity_colors(mode))
    }
    #[test]
    fn both_trails_have_the_same_fifty_percent_longer_span_and_distinct_gradients() {
        assert!((ACTIVITY_TRAIL_LENGTH / 0.34 - 1.5).abs() < 0.0001);
        for kind in [
            ActivityTrail::Microphone,
            ActivityTrail::Ai,
            ActivityTrail::Embedding,
        ] {
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

#[derive(Default)]
struct ConnectionState {
    signature: u64,
    job: u64,
    busy: bool,
    result: Option<std::result::Result<(), String>>,
    at: Option<Instant>,
}
fn connection_signature(s: &Settings, tts: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if tts {
        (&s.tts_url, &s.tts_key).hash(&mut h);
    } else {
        (&s.llm_url, &s.llm_key).hash(&mut h);
    }
    h.finish()
}
fn expire_notice(
    value: &mut String,
    clock: &mut Option<(String, Instant)>,
    busy: bool,
    now: Instant,
) -> Option<Duration> {
    if value.is_empty() {
        *clock = None;
        return None;
    }
    if clock.as_ref().is_none_or(|(old, _)| old != value) {
        *clock = Some((value.clone(), now));
    }
    let (_, at) = clock.as_mut().unwrap();
    if busy {
        *at = now;
        return None;
    }
    let age = now.saturating_duration_since(*at);
    if age >= Duration::from_secs(3) {
        value.clear();
        *clock = None;
        None
    } else {
        Some(Duration::from_secs(3) - age)
    }
}
#[cfg(test)]
mod notice_tests {
    #[test]
    fn completed_notices_expire_but_active_work_stays_visible() {
        let now = super::Instant::now();
        let mut value = "Dictation complete".to_owned();
        let mut clock = None;
        assert!(super::expire_notice(&mut value, &mut clock, false, now).is_some());
        super::expire_notice(
            &mut value,
            &mut clock,
            false,
            now + super::Duration::from_secs(3),
        );
        assert!(value.is_empty());
        value = "Transcribing".into();
        super::expire_notice(&mut value, &mut clock, true, now);
        super::expire_notice(
            &mut value,
            &mut clock,
            true,
            now + super::Duration::from_secs(10),
        );
        assert!(!value.is_empty());
        super::expire_notice(
            &mut value,
            &mut clock,
            false,
            now + super::Duration::from_secs(13),
        );
        assert!(value.is_empty());
    }
}

// Reserve one bounded row so both panes remain independent of their content widths.
fn preview_columns(
    ui: &mut egui::Ui,
    id: &str,
    saved: &mut Option<f32>,
    default: f32,
    height: f32,
) -> [egui::Ui; 2] {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let gap = 12.0_f32.min(rect.width());
    let usable = (rect.width() - gap).max(1.0);
    let minimum = (200.0 / usable).min(0.45);
    let ratio = saved
        .filter(|v| v.is_finite())
        .unwrap_or(default)
        .clamp(minimum, 1.0 - minimum);
    let x = rect.left() + usable * ratio;
    let grab = egui::Rect::from_min_max(
        egui::pos2(x, rect.top()),
        egui::pos2(x + gap, rect.bottom()),
    );
    let response = ui
        .interact(
            grab,
            ui.make_persistent_id(id),
            egui::Sense::click_and_drag(),
        )
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
        .on_hover_text(crate::i18n::t(
            "Drag to resize preview · double-click to reset",
        ));
    let mut ratio = ratio;
    if response.dragged() {
        if let Some(pointer) = response.interact_pointer_pos() {
            ratio = ((pointer.x - rect.left() - gap * 0.5) / usable).clamp(minimum, 1.0 - minimum);
            *saved = Some(ratio);
        }
    }
    if response.double_clicked() {
        ratio = default.clamp(minimum, 1.0 - minimum);
        *saved = Some(ratio);
    }
    let x = rect.left() + usable * ratio;
    let stroke = if response.hovered() || response.dragged() {
        ui.visuals().selection.stroke
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke
    };
    ui.painter().line_segment(
        [
            egui::pos2(x + gap * 0.5, rect.top()),
            egui::pos2(x + gap * 0.5, rect.bottom()),
        ],
        stroke,
    );
    let panes = [
        egui::Rect::from_min_max(rect.min, egui::pos2(x, rect.bottom())),
        egui::Rect::from_min_max(egui::pos2(x + gap, rect.top()), rect.max),
    ];
    panes.map(|pane| {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(pane)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(pane.intersect(ui.clip_rect()));
        child
    })
}
fn preview_image_size(image: egui::Vec2, bounds: egui::Vec2) -> egui::Vec2 {
    let scale = (bounds.x.max(1.0) / image.x.max(1.0)).min(bounds.y.max(1.0) / image.y.max(1.0));
    image * scale
}

#[cfg(test)]
mod preview_resize_tests {
    use super::*;
    #[test]
    fn divider_drag_changes_width_and_bounds_both_panes() {
        let ctx = egui::Context::default();
        let mut saved = None;
        let mut panes = [egui::Rect::NOTHING; 2];
        let mut frame = |events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 500.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let children = preview_columns(ui, "test-preview", &mut saved, 0.60, 400.0);
                        panes = children.map(|ui| ui.max_rect());
                    });
                },
            );
            (saved, panes)
        };
        let (_, initial) = frame(vec![]);
        let point = egui::pos2((initial[0].right() + initial[1].left()) * 0.5, 100.0);
        frame(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ]);
        let (ratio, resized) = frame(vec![egui::Event::PointerMoved(
            point + egui::vec2(100.0, 0.0),
        )]);
        assert!(ratio.unwrap() > 0.60);
        assert!(resized[0].width() > initial[0].width());
        assert!(resized[1].width() < initial[1].width());
        assert!(resized[0].right() < resized[1].left());
        let (_, bounded) = frame(vec![egui::Event::PointerMoved(
            point + egui::vec2(4000.0, 0.0),
        )]);
        assert!(bounded[0].width() >= 199.0 && bounded[1].width() >= 199.0);
    }
    #[test]
    fn portrait_and_landscape_fill_available_space_without_distortion() {
        for (image, bounds) in [
            (egui::vec2(1600.0, 900.0), egui::vec2(300.0, 500.0)),
            (egui::vec2(900.0, 1600.0), egui::vec2(500.0, 300.0)),
        ] {
            let size = preview_image_size(image, bounds);
            assert!((size.x / size.y - image.x / image.y).abs() < 0.0001);
            assert!(size.x <= bounds.x + 0.001 && size.y <= bounds.y + 0.001);
            assert!((size.x - bounds.x).abs() < 0.001 || (size.y - bounds.y).abs() < 0.001);
        }
    }
}

#[cfg(test)]
mod dialog_bounds_tests {
    use super::*;
    #[test]
    fn long_dialogs_stay_inside_small_or_resized_hosts() {
        let ctx = egui::Context::default();
        for size in [
            egui::vec2(940.0, 640.0),
            egui::vec2(780.0, 500.0),
            egui::vec2(780.0, 254.0),
        ] {
            for _ in 0..4 {
                let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,size)),..Default::default()},|ctx|{
                    let response=dialog_window(ctx,crate::i18n::t("Bounds regression"),860.0).show(ctx,|ui|{
                        for _ in 0..50 {ui.label(crate::i18n::t("Long dialog content that must scroll rather than grow beyond the host window."));}
                        let _ = ui.button(crate::i18n::t("Action"));
                    }).unwrap();
                    let r=response.response.rect;
                    assert!(r.left()>=0.0 && r.top()>=0.0 && r.right()<=size.x+1.0 && r.bottom()<=size.y+1.0,"{r:?} outside {size:?}");
                });
            }
        }
    }
}

#[cfg(test)]
mod search_open_tests {
    use super::*;
    #[test]
    fn warmup_is_opt_in_for_remote_and_never_used_for_names() {
        let mut settings = Settings::default();
        assert!(settings.warm_on_search_open("Both"));
        assert!(!settings.warm_on_search_open("Names"));
        settings.embedding_server = "Remote".into();
        assert!(!settings.warm_on_search_open("Both"));
        settings.remote_warm_on_open = true;
        assert!(settings.warm_on_search_open("Semantic"));
        assert!(!settings.warm_on_search_open("Names"));
    }
    #[test]
    fn chat_catalog_is_url_scoped_and_selection_is_unique() {
        let home = tempfile::tempdir().unwrap();
        let values = vec!["model-a".into(), "model-b".into(), "model-a".into()];
        cache_chat_models(home.path(), "https://example.test/v1", &values);
        assert_eq!(
            load_chat_models(home.path(), "https://example.test/v1"),
            values
        );
        assert!(load_chat_models(home.path(), "https://other.test/v1").is_empty());
        assert_eq!(
            chat_model_choices("model-b", &values),
            vec!["model-b", "model-a"]
        );
    }
}

#[cfg(test)]
mod ai_markdown_layout_tests {
    use eframe::egui::{self, epaint::Shape};
    fn text_shapes(shape: &Shape, spans: &mut Vec<(String, egui::Rect)>) {
        match shape {
            Shape::Text(text) => spans.push((
                text.galley.job.text.clone(),
                text.galley.rect.translate(text.pos.to_vec2()),
            )),
            Shape::Vec(shapes) => {
                for s in shapes {
                    text_shapes(s, spans);
                }
            }
            _ => {}
        }
    }
    #[test]
    fn quoted_sources_in_lists_use_a_new_full_width_row() {
        let markdown = "## Key facts\n\n- A [linked source](https://example.org/news) published an investigation with a long italic *title about a developing event*, which explicitly states:\n  > QUOTE_SENTINEL The quoted source should wrap across the response panel rather than becoming a tiny column beside the preceding paragraph.\n\n- NEXT_SENTINEL The following list item remains readable.\n";
        for width in [360.0, 600.0, 900.0] {
            let ctx = egui::Context::default();
            let mut cache = egui_commonmark::CommonMarkCache::default();
            let mut output = None;
            for _ in 0..2 {
                output = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 1500.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            egui_commonmark::CommonMarkViewer::new().show(ui, &mut cache, markdown);
                        });
                    },
                ));
            }
            let mut spans = Vec::new();
            for shape in output.unwrap().shapes {
                text_shapes(&shape.shape, &mut spans);
            }
            let (_, quote) = spans
                .iter()
                .find(|(s, _)| s.contains("QUOTE_SENTINEL"))
                .expect("quote text");
            assert!(
                quote.left() < width * 0.3,
                "quote starts too far right at width {width}: {quote:?}"
            );
            assert!(
                quote.right() <= width + 1.0,
                "quote overflows panel: {quote:?}"
            );
            assert!(
                quote.height() < 180.0,
                "quote collapsed into a narrow column: {quote:?}"
            );
        }
    }
}
