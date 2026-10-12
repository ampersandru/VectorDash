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
const PAGES: [&str; 6] = [
    "Search",
    "Sources",
    "Models & hardware",
    "Scheduling",
    "Settings",
    "Logs",
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
    exclude_types: String,
    hits: Vec<Hit>,
    selected: usize,
    sequence: u64,
    changed: Instant,
    dirty: bool,
    searching: bool,
    search_pending: bool,
    search_ms: f64,
    indexing: bool,
    setup_busy: bool,
    progress: Progress,
    stats: Stats,
    source_stats: HashMap<String, SourceStats>,
    status: String,
    logs: Vec<(String, String)>,
    log_filter: [bool; 4],
    model_logs: bool,
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
    child_theme_key: String,
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
        if smoke.is_none() {
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
        for name in ["segoeui.ttf", "Nirmala.ttf", "msyh.ttc"] {
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
        cc.egui_ctx.set_fonts(fonts);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut app = Self {
            home,
            collapsed: s.sidebar_collapsed,
            mode: s.default_search_mode.clone(),
            exclude_types: s.search_excluded_types.clone(),
            runtime_setup: s.setup_runtime,
            model_setup: s.setup_model,
            encoder_setup: s.setup_encoder,
            ocr_setup: s.setup_ocr,
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
            hits: vec![],
            selected: 0,
            sequence: 0,
            changed: Instant::now(),
            dirty: false,
            searching: false,
            search_pending: false,
            search_ms: 0.0,
            indexing: false,
            setup_busy: false,
            progress: Progress::default(),
            stats: Stats::default(),
            source_stats: HashMap::new(),
            status: "Ready — models load only when needed".into(),
            logs: vec![],
            log_filter: [false, true, true, true],
            model_logs: false,
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
            manual_exclude: String::new(),
            devices: vec![],
            uninstall_confirm: false,
            delete_confirm: None,
            hotkey_capture: false,
            startup: Instant::now(),
            last_schedule: String::new(),
            smoke,
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
            child_theme_key: String::new(),
        };
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
            app.quick = args.iter().any(|a| a == "--smoke-quick");
            app.qa_hidden = args.iter().any(|a| a == "--smoke-hidden");
            app.qa_screenshot = get("--smoke-screenshot").map(PathBuf::from);
            app.page = get("--smoke-page")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if let Some(query) = get("--smoke-query") {
                app.query = query;
                app.mode = "Names".into();
                app.dirty = true;
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
            }
        }
        if app.s.mcp_enabled {
            app.apply_mcp();
        }
        Ok(app)
    }
    fn accent(&self) -> Color32 {
        Color32::from_rgb(self.s.accent[0], self.s.accent[1], self.s.accent[2])
    }
    fn save(&mut self) {
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
        *self.engine.control.schedule.lock().unwrap() = self.s.clone();
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
            Ok(()) => self.status = "Settings saved".into(),
            Err(e) => self.status = e.to_string(),
        }
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
            match event {
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
                    self.exiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Event::Full => {
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
                                self.selected = 0;
                                self.texture = None;
                                self.preview_error = None;
                                self.preview_pending = None;
                                self.search_ms = ms;
                                self.status = format!("{} results · {ms:.1} ms", self.hits.len());
                            }
                            Err(e) => self.status = e,
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
                Event::Summon(target, text) => {
                    if self.qa_hidden {
                        self.qa_hidden_reopened = true;
                    }
                    self.large_preview = false;
                    self.quick = true;
                    self.popup_size_state = None;
                    self.popup_had_focus = false;
                    self.page = 0;
                    self.target = target;
                    self.query = text;
                    self.hits.clear();
                    self.dirty = true;
                    self.changed = Instant::now();
                    ctx.memory_mut(|m| m.request_focus(egui::Id::new("query")));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        940.0, 245.0,
                    )));
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        egui::WindowLevel::AlwaysOnTop,
                    ));
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
    fn dispatch(&mut self) {
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
        self.sequence += 1;
        self.changed = Instant::now();
        self.dirty = true;
    }
    fn start_index(&mut self, force: bool) {
        if self.indexing || self.setup_busy {
            return;
        }
        self.save();
        self.indexing = true;
        self.progress = Progress::default();
        self.engine.index(self.s.clone(), force);
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
                    let response = ui.add(
                        egui::Label::new(
                            RichText::new("VectorDash")
                                .strong()
                                .size(if self.quick { 19.0 } else { 24.0 })
                                .color(self.accent()),
                        )
                        .sense(egui::Sense::click_and_drag()),
                    );
                    if response.is_pointer_button_down_on()
                        && ctx.input(|i| i.pointer.any_pressed())
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if !self.quick
                        && ui
                            .add(
                                egui::Button::image(icons::image(Icon::Menu, self.accent(), 20.0))
                                    .frame(false),
                            )
                            .on_hover_text(if self.collapsed {
                                "Expand sidebar"
                            } else {
                                "Collapse sidebar"
                            })
                            .clicked()
                    {
                        self.collapsed = !self.collapsed;
                        self.save();
                    }
                    let reserve = if self.quick { 220.0 } else { 112.0 };
                    let (_, response) = ui.allocate_exact_size(
                        egui::vec2((ui.available_width() - reserve).max(0.0), 30.0),
                        egui::Sense::click_and_drag(),
                    );
                    if response.is_pointer_button_down_on()
                        && ctx.input(|i| i.pointer.any_pressed())
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
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
                                    .frame(false),
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
                                        .frame(false),
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
                                    .frame(false),
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
        ui.horizontal(|ui| {
            let drag = ui.add(
                crate::icons::image(crate::icons::Icon::Search, self.accent(), 26.0)
                    .sense(egui::Sense::drag()),
            );
            if self.quick && drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            let response = ui.add_sized(
                [ui.available_width(), 42.0],
                egui::TextEdit::singleline(&mut query)
                    .hint_text("Search Anything")
                    .font(egui::TextStyle::Heading)
                    .id(egui::Id::new("query")),
            );
            if self.quick && self.startup.elapsed() < Duration::from_secs(2) {
                response.request_focus();
            }
            if response.changed() {
                self.query = query;
                self.changed();
            }
        });
        let old = (
            self.mode.clone(),
            self.category.clone(),
            self.scope.clone(),
            self.exclude_types.clone(),
        );
        ui.horizontal_wrapped(|ui| {
            combo(
                ui,
                "search_mode",
                &mut self.mode,
                &["Names", "Semantic", "Both"],
            );
            combo(ui, "category", &mut self.category, &CATEGORIES);
            egui::ComboBox::from_id_salt("scope")
                .selected_text(if self.scope.is_empty() {
                    "All sources"
                } else {
                    self.scope.as_str()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.scope, String::new(), "All sources");
                    for root in &self.s.roots {
                        ui.selectable_value(&mut self.scope, root.path.clone(), &root.path);
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.exclude_types)
                    .hint_text("Exclude types: .zip, .log")
                    .desired_width(190.0),
            );
        });
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
        if self.quick
            && self.query.is_empty()
            && ctx.animate_bool_with_time(egui::Id::new("popup-expansion"), false, 0.18) <= 0.0
        {
            return;
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
                    egui::ScrollArea::vertical().id_salt("results").show_rows(
                        ui,
                        64.0,
                        self.hits.len(),
                        |ui, range| {
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
                                if !["settings", "calculator", "folder"]
                                    .contains(&hit.kind.as_str())
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
                                        egui::Image::new(texture)
                                            .fit_to_exact_size(icon_rect.size()),
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
                                    .truncate(),
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
                                metadata_ui.add(egui::Label::new(job).truncate());
                                if response.clicked() {
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
                                            !["calculator", "settings"]
                                                .contains(&hit.kind.as_str()),
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
                        },
                    );
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
                        crate::content::decode(Path::new(&path), &s)
                            .and_then(|i| crate::content::jpeg(&i, 1024))
                    }
                    .map_err(|e| e.to_string());
                    let _ = tx.send(Event::Preview(path, result));
                });
            }
            if let Some((path, texture)) = &self.texture {
                if path == &hit.path {
                    let image = egui::Image::new(texture).max_size(egui::vec2(
                        ui.available_width(),
                        ui.available_height() * if self.large_preview { 0.80 } else { 0.65 },
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
                self.run(crate::platform::play_at(&hit.path, hit.seconds));
            }
            if ui.button("Stop playback").clicked() {
                crate::platform::stop_playback();
            }
            ui.label(format!(
                "Moment {}:{:02}",
                hit.seconds as u32 / 60,
                hit.seconds as u32 % 60
            ));
        }
        egui::ScrollArea::vertical()
            .id_salt("preview_text")
            .show(ui, |ui| {
                ui.label(&hit.text);
            });
    }
    fn open_selected(&mut self, ctx: &egui::Context) {
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
                    self.engine.control.cancel.store(true, Ordering::Relaxed);
                }
                if ui
                    .add_enabled(!self.indexing, egui::Button::new("Rebuild full content"))
                    .clicked()
                {
                    self.start_index(true);
                }
            });
        });
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
                    self.save();
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
                    self.save();
                }
            }
            if ui.button("Select all").clicked() {
                self.source_selected = vec![true; self.s.roots.len()];
            }
            if ui.button("Apply mode to selected").clicked() {
                for (i, root) in self.s.roots.iter_mut().enumerate() {
                    if self.source_selected.get(i) == Some(&true) {
                        root.mode = self.source_mode.clone();
                    }
                }
                self.save();
            }
        });
        self.source_selected.resize(self.s.roots.len(), false);
        let mut remove = None;
        for (i, root) in self.s.roots.clone().iter().enumerate() {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.source_selected[i], "");
                ui.label(&root.path);
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
        if let Some(i) = remove {
            self.s.roots.remove(i);
            self.source_selected.remove(i);
            self.save();
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
        let mut remove = None;
        for (i, p) in self.s.exclusions.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(p);
                if ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.s.exclusions.remove(i);
        }
        ui.checkbox(
            &mut self.s.excluded_sensitive,
            "Skip content with potential credentials",
        );
        field(ui, "Exclude file types", &mut self.s.excluded_types);
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
        if ui.button("Save source settings").clicked() {
            self.save();
            self.changed();
        }
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
    fn models(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.s.unload_idle, "Unload local models when idle");
        if self.s.unload_idle {
            ui.add(
                egui::Slider::new(&mut self.s.unload_minutes, 1..=120)
                    .text("Idle unload (minutes)"),
            );
        }
        ui.label("OCR and media helpers run only while needed. Remote servers are managed by their host.");
        if ui.button("Save unload settings").clicked() {
            self.save();
        }
        ui.heading("Models & hardware");
        ui.label("Choose features to set up. Installed tools and enabled features are separate from this checklist.");
        let before = (
            self.runtime_setup,
            self.model_setup,
            self.encoder_setup,
            self.ocr_setup,
        );
        ui.group(|ui|{
            ui.set_min_width(ui.available_width());ui.heading("Status");
            let loaded=self.engine.models.loaded.load(Ordering::Relaxed);let media=self.engine.models.media_loaded();
            for(name,path,selected,active,enabled)in [("Local llama-server",&self.s.server,&mut self.runtime_setup,loaded,true),("EmbeddingGemma 2",&self.s.model,&mut self.model_setup,loaded,true),("Photos / audio / video encoder",&self.s.projector,&mut self.encoder_setup,media,true),("Tesseract OCR",&self.s.ocr,&mut self.ocr_setup,!self.s.ocr.is_empty()&&self.s.ocr_enabled,self.s.ocr_enabled)]{
                ui.horizontal(|ui|{ui.checkbox(selected,name);ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{let installed=Path::new(path).is_file();let(text,color)=if name=="Local llama-server"&&installed&&!self.s.runtime_compatible(){("Different backend",Color32::YELLOW)}else if !enabled{("Disabled",Color32::YELLOW)}else if !installed{("Not installed",Color32::LIGHT_RED)}else if active{(if name=="Tesseract OCR"{"Ready · on demand"}else{"Loaded"},Color32::GREEN)}else{("Installed · unloaded",Color32::YELLOW)};ui.label(RichText::new(text).strong().color(color));});});
            }
            ui.horizontal(|ui|{ui.label("FFmpeg / PDF renderer");ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.label(RichText::new("Bundled · ready").color(Color32::GREEN));});});
            ui.label("Selected checkboxes are saved. Models load only for Full indexing or semantic search; OCR and decoder tools run on demand.");
            let all_ready=[(&self.s.server,self.runtime_setup),(&self.s.model,self.model_setup),(&self.s.projector,self.encoder_setup),(&self.s.ocr,self.ocr_setup)].iter().all(|(path,selected)|!*selected||Path::new(path).is_file())&&(!self.runtime_setup||self.s.runtime_compatible());
            let any=self.runtime_setup||self.model_setup||self.encoder_setup||self.ocr_setup;
            let model_selected=self.runtime_setup||self.model_setup||self.encoder_setup;
            let already_started=loaded&&(!self.encoder_setup||media);
            ui.horizontal(|ui|{
                let label=if all_ready{if model_selected{"Start"}else{"Ready"}}else{"Download and install selected"};
                if ui.add_enabled(!self.setup_busy&&any&&(!all_ready||model_selected&&!already_started),egui::Button::new(label).fill(self.accent())).clicked(){
                    self.save();if all_ready{let models=self.engine.models.clone();let settings=self.s.clone();let media=self.encoder_setup;let tx=self.engine.tx.clone();self.setup_busy=true;std::thread::spawn(move||{let result=models.start(&settings,media).map(|_|vec![]).map_err(|e|e.to_string());let _=tx.send(Event::Setup(result));});}
                    else{self.setup_busy=true;self.setup_progress=None;self.setup.install(self.s.clone(),self.runtime_setup,self.model_setup,self.encoder_setup,self.ocr_setup);}
                }
                if ui.add_enabled(loaded,egui::Button::new("Stop / unload models")).clicked(){let models=self.engine.models.clone();std::thread::spawn(move||{let _=models.stop();});}
                if self.setup_busy&&ui.button("Cancel setup").clicked(){self.setup.cancel.store(true,Ordering::Relaxed);}
            });
            if self.setup_busy{if let Some((name,done,total))=&self.setup_progress{ui.label(name);if *total>0{ui.add(egui::ProgressBar::new(*done as f32/ *total as f32).show_percentage());}}}
        });
        if before
            != (
                self.runtime_setup,
                self.model_setup,
                self.encoder_setup,
                self.ocr_setup,
            )
        {
            self.save();
        }
        ui.add_space(12.0);
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
        ui.horizontal(|ui| {
            ui.label("Text batch size");
            ui.add(egui::Slider::new(&mut self.s.batch_size, 1..=16));
        });
        if ui.button("Save model settings").clicked() {
            self.save();
        }
        if ui
            .button("Uninstall all downloaded models and runtimes…")
            .clicked()
        {
            self.uninstall_confirm = true;
        }
    }
    fn settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Settings");
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
        ui.label("Higher transparency reveals more of the blurred desktop. Appearance changes are applied immediately and saved.");
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
        if ui.button("Save / apply settings").clicked() {
            self.save();
            self.apply_mcp();
            self.run(crate::platform::startup(self.s.start_with_windows));
            self.shutdown.store(true, Ordering::Relaxed);
            let tx = self.engine.tx.clone();
            let s = self.s.clone();
            let signal = Arc::new(AtomicBool::new(false));
            self.shutdown = signal.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                crate::platform::hotkeys(tx, signal, s);
            });
            crate::appearance::apply(ctx, &self.s);
        }
    }
    fn scheduling(&mut self, ui: &mut egui::Ui) {
        ui.heading("Scheduling");
        combo(
            ui,
            "schedule",
            &mut self.s.schedule,
            &["Manual", "At selected time", "When idle", "Time + idle"],
        );
        let timed = ["At selected time", "Time + idle"].contains(&self.s.schedule.as_str());
        let idle = ["When idle", "Time + idle"].contains(&self.s.schedule.as_str());
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
            ui.label("Conditions are checked between files and embedding batches. If GPU usage cannot be read, turn off the GPU condition to allow scheduled indexing.");
        }
        if self.s.schedule != "Manual" {
            ui.checkbox(&mut self.s.ac_only, "Automatic indexing only on AC power");
            ui.add(
                egui::Slider::new(&mut self.s.max_run_minutes, 5..=1440)
                    .text("Maximum automatic run (minutes)"),
            );
        }
        if ui.button("Save schedule").clicked() {
            self.s.schedule_enabled_at = chrono::Local::now().timestamp();
            self.s.last_schedule_slot = 0;
            self.save();
        }
        ui.add_space(12.0);
        ui.heading("Schedule status");
        ui.label(&self.schedule_status);
        if idle {
            let metrics = crate::scheduling::metrics();
            ui.label(format!(
                "CPU {} · GPU {} · {}",
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
            ui.selectable_value(&mut self.model_logs, false, "VectorDash logs");
            ui.selectable_value(&mut self.model_logs, true, "Model logs");
        });
        if self.model_logs {
            let text = tail_log(&self.home.join("model.log"))
                .unwrap_or_else(|_| "No model has loaded yet".into());
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
                                    Color32::YELLOW
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
        let appearance = format!(
            "{}{:?}{:?}{}{}",
            self.s.theme,
            self.s.accent,
            self.s.popup_tone,
            self.s.popup_opacity,
            self.s.popup_gradient
        ) + &format!("{}", ctx.style().visuals.dark_mode);
        if appearance != self.appearance_key {
            let changed = !self.appearance_key.is_empty();
            crate::appearance::apply(ctx, &self.s);
            self.appearance_key = appearance;
            if changed {
                self.save();
            }
        }
        crate::appearance::background(ctx, &self.s, self.quick);
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
            | ((self.s.popup_tone[2] as u64) << 49);
        if self.material_key != Some(key) {
            if let Ok(handle) = frame.window_handle() {
                if let raw_window_handle::RawWindowHandle::Win32(win) = handle.as_raw() {
                    crate::platform::material(
                        win.hwnd.get(),
                        self.quick,
                        self.s.popup_tone,
                        self.s.popup_opacity,
                    );
                    self.material_key = Some(key);
                }
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
                let tx = self.engine.tx.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(700));
                    let _ = tx.send(Event::Summon(0, "settings: display".into()));
                });
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
        self.max_frame_ms = self
            .max_frame_ms
            .max(now.duration_since(self.last_frame).as_secs_f64() * 1000.0);
        self.last_frame = now;
        self.frames += 1;
        self.events(ctx);
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
        if !self.engine.control.running.load(Ordering::Relaxed) {
            self.engine.models.idle_unload(&self.s);
        }
        self.indexing = self.engine.control.running.load(Ordering::Relaxed);
        let metrics = crate::scheduling::metrics();
        self.schedule_status =
            if self.indexing && self.engine.control.automatic.load(Ordering::Relaxed) {
                if ["When idle", "Time + idle"].contains(&self.s.schedule.as_str()) {
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
        ctx.request_repaint_after(Duration::from_millis(50));
        if !self.quick {
            self.titlebar(ctx);
        }
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(20, 9)))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if self.quick && ui.button("Open full app").clicked() {
                        let _ = self.engine.tx.send(Event::Full);
                    }
                    ui.label(&self.status);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (key, action) in [
                            ("Esc", "dismiss"),
                            ("Tab", "types"),
                            ("Ctrl+C", "copy"),
                            ("Space", "preview"),
                            ("Enter", "open"),
                            ("↑ ↓", "select"),
                        ] {
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
                    for (i, label) in PAGES.iter().enumerate() {
                        let icon = [
                            crate::icons::Icon::Search,
                            crate::icons::Icon::Sources,
                            crate::icons::Icon::Models,
                            crate::icons::Icon::Schedule,
                            crate::icons::Icon::Settings,
                            crate::icons::Icon::Logs,
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
            if self.popup_had_focus && !focused && !self.large_preview {
                self.popup_had_focus = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            let empty = self.query.is_empty();
            let expansion =
                ctx.animate_bool_with_time(egui::Id::new("popup-expansion"), !empty, 0.18);
            if self.popup_size_state != Some(empty) || (expansion > 0.0 && expansion < 1.0) {
                self.popup_size_state = Some(empty);
                ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                    780.0, 200.0,
                )));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    940.0,
                    200.0 + 370.0 * expansion,
                )));
            }
        } else {
            self.popup_size_state = None;
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
                        _ => self.logs(ui),
                    });
                }
            });
        if self.page == 0 && ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("query")));
            self.selected = (self.selected + 1).min(self.hits.len().saturating_sub(1));
            self.texture = None;
            self.preview_error = None;
        }
        if self.page == 0 && ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            self.selected = self.selected.saturating_sub(1);
            self.texture = None;
            self.preview_error = None;
        }
        if self.page == 0
            && !ctx.memory(|m| m.has_focus(egui::Id::new("query")))
            && ctx.input(|i| i.key_pressed(egui::Key::Space))
        {
            self.large_preview = !self.large_preview;
        }
        if self.page == 0 && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.open_selected(ctx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.large_preview {
                self.large_preview = false;
            } else {
                self.close_window(ctx);
            }
        }
        if self.page == 0 && ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
            let current = CATEGORIES
                .iter()
                .position(|c| *c == self.category)
                .unwrap_or(0);
            self.category = CATEGORIES[if ctx.input(|i| i.modifiers.shift) {
                (current + CATEGORIES.len() - 1) % CATEGORIES.len()
            } else {
                (current + 1) % CATEGORIES.len()
            }]
            .into();
            self.changed();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::C))
            && !ctx.wants_keyboard_input()
        {
            if let Some(hit) = self.hits.get(self.selected).cloned() {
                if hit.kind == "calculator" {
                    ctx.copy_text(hit.path);
                } else {
                    self.run(crate::platform::copy_file(&hit.path));
                }
            }
        }
        if !self.large_preview {
            self.child_theme_key.clear();
        }
        if self.large_preview {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("vectordash-quick-look"),
                egui::ViewportBuilder::default()
                    .with_title("VectorDash — Quick Look")
                    .with_position(ctx.input(|i| {
                        i.viewport()
                            .outer_rect
                            .map(|r| r.min)
                            .unwrap_or(egui::pos2(100.0, 100.0))
                    }))
                    .with_inner_size([1040.0, 800.0])
                    .with_min_inner_size([700.0, 500.0])
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
            egui::Window::new("Uninstall downloaded models and runtimes?").collapsible(false).show(ctx,|ui|{ui.label("Only managed downloads are removed. Source files, external tools and your index are retained.");if ui.button("Uninstall downloads").clicked(){let _=self.engine.models.stop();let result=self.setup.uninstall();if result.is_ok(){for path in [&mut self.s.server,&mut self.s.model,&mut self.s.projector]{if Path::new(path).starts_with(&self.home){path.clear();}}self.save();}else{self.run(result);}self.uninstall_confirm=false;}if ui.button("Cancel").clicked(){self.uninstall_confirm=false;}});
        }
        if let Some(report) = &self.smoke {
            if self.startup.elapsed() > Duration::from_secs(4)
                && (self.qa_screenshot.is_none() || self.qa_captured)
            {
                let data = serde_json::json!({"ok":true,"native_rust":true,"model_loaded":self.engine.models.loaded.load(Ordering::Relaxed),"frames":self.frames,"max_frame_interval_ms":self.max_frame_ms,"dark_mode":ctx.style().visuals.dark_mode,"hidden_reopened":self.qa_hidden_reopened,"ocr_enabled":self.s.ocr_enabled,"ocr_setup_selected":self.ocr_setup,"results":self.hits.len(),"quick_look":self.large_preview,"quick_look_size":self.qa_child_size,"cpu":crate::scheduling::metrics().cpu,"gpu":crate::scheduling::metrics().gpu,"query_focus":ctx.memory(|m|m.has_focus(egui::Id::new("query")))});
                let _ = std::fs::write(report, data.to_string());
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.shutdown.store(true, Ordering::Relaxed);
        self.scheduler_shutdown.store(true, Ordering::Relaxed);
        self.engine.control.cancel.store(true, Ordering::Relaxed);
        self.setup.cancel.store(true, Ordering::Relaxed);
        crate::platform::stop_playback();
        let _ = self.engine.models.stop();
        self.mcp = None;
    }
}
fn combo(ui: &mut egui::Ui, id: &str, value: &mut String, items: &[&str]) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.as_str())
        .show_ui(ui, |ui| {
            for item in items {
                ui.selectable_value(value, (*item).into(), *item);
            }
        });
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
