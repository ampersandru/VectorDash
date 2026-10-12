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
}
impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        home: PathBuf,
        smoke: Option<PathBuf>,
    ) -> Result<Self> {
        let s = Settings::load(&home)?;
        let db = Database::new(&home, &s)?;
        let (tx, rx) = crossbeam_channel::bounded(512);
        let tx = crate::engine::NotifySender::new(tx, Some(cc.egui_ctx.clone()));
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
        engine.refresh_stats(&s);
        if s.include_programs {
            crate::platform::warm_programs();
        }
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.visuals = if s.theme == "Light" {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        style.visuals.selection.bg_fill = Color32::from_rgb(s.accent[0], s.accent[1], s.accent[2]);
        style.visuals.widgets.hovered.bg_fill = style.visuals.selection.bg_fill.gamma_multiply(0.6);
        style.visuals.widgets.active.bg_fill = style.visuals.selection.bg_fill;
        for w in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            w.corner_radius = egui::CornerRadius::same(7);
        }
        style.visuals.window_corner_radius = egui::CornerRadius::same(16);
        cc.egui_ctx.set_style(style);
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
        cc.egui_ctx.set_fonts(fonts);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut app = Self {
            home,
            s,
            engine,
            setup,
            rx,
            shutdown,
            mcp: None,
            page: 0,
            collapsed: false,
            query: String::new(),
            mode: "Both".into(),
            category: "All files".into(),
            scope: String::new(),
            exclude_types: String::new(),
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
            preview_pending: None,
            preview_error: None,
            source_mode: "quick".into(),
            source_selected: vec![],
            manual_exclude: String::new(),
            runtime_setup: true,
            model_setup: true,
            encoder_setup: true,
            ocr_setup: false,
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
        };
        if app.s.mcp_enabled {
            app.apply_mcp();
        }
        Ok(app)
    }
    fn accent(&self) -> Color32 {
        Color32::from_rgb(self.s.accent[0], self.s.accent[1], self.s.accent[2])
    }
    fn save(&mut self) {
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
                Event::Devices(devices) => {
                    self.devices = devices;
                    self.status = "Select a device from the list".into();
                }
                Event::Thumbnail(path, result) => {
                    self.thumb_pending.remove(&path);
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
                Event::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
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
                Event::Log(level, text) => self.log(level, text),
                Event::Setup(result) => {
                    self.setup_busy = false;
                    match result {
                        Ok(paths) => {
                            for (k, p) in paths {
                                match k.as_str() {
                                    "server" => self.s.server = p,
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
                    self.quick = true;
                    self.page = 0;
                    self.target = target;
                    self.query = text;
                    self.hits.clear();
                    self.dirty = true;
                    self.changed = Instant::now();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        940.0, 185.0,
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
    fn titlebar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("title")
            .frame(egui::Frame::NONE.inner_margin(12.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let title = ui.add(
                        egui::Label::new(
                            RichText::new("VectorDash")
                                .color(self.accent())
                                .strong()
                                .size(20.0),
                        )
                        .sense(egui::Sense::drag()),
                    );
                    if title.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if ui
                        .button(if self.collapsed { "☰" } else { "‹" })
                        .on_hover_text("Collapse sidebar")
                        .clicked()
                    {
                        self.collapsed = !self.collapsed;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✕").clicked() {
                            if self.quick || self.s.close_to_tray {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                            } else {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        if ui.button("−").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        if self.quick && ui.button("Open full app").clicked() {
                            self.quick = false;
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
            ui.label(RichText::new("⌕").color(self.accent()).size(28.0));
            let response = ui.add_sized(
                [ui.available_width(), 42.0],
                egui::TextEdit::singleline(&mut query)
                    .hint_text("Describe a file, or search for exact words…")
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
        if self.quick && self.query.is_empty() {
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
                                } else if hit.kind == "settings" {
                                    hit.text.clone()
                                } else {
                                    name
                                };
                                if (content_image(&hit.path) || hit.kind == "application")
                                    && !self.thumbs.contains_key(&hit.path)
                                    && !self.thumb_pending.contains(&hit.path)
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
                                let button = egui::Button::new(
                                    RichText::new(format!(
                                        "{}  {}\n{} · {}",
                                        if hit.kind == "folder" {
                                            "▣"
                                        } else if content_image(&hit.path) || hit.page > 0 {
                                            "▧"
                                        } else {
                                            "▤"
                                        },
                                        title,
                                        hit.kind,
                                        Path::new(&hit.path)
                                            .parent()
                                            .unwrap_or(Path::new(""))
                                            .display()
                                    ))
                                    .size(13.0),
                                )
                                .selected(self.selected == i)
                                .min_size(egui::vec2(100.0, 58.0));
                                let icon = self.thumbs.get(&hit.path).cloned();
                                let response = ui
                                    .horizontal(|ui| {
                                        if let Some(texture) = icon {
                                            ui.add(
                                                egui::Image::new(&texture)
                                                    .fit_to_exact_size(egui::vec2(44.0, 44.0)),
                                            );
                                        }
                                        ui.add_sized(
                                            [ui.available_width(), 58.0],
                                            button.wrap_mode(egui::TextWrapMode::Truncate),
                                        )
                                    })
                                    .inner;
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
                    ui.add(egui::Image::new(texture).max_size(egui::vec2(
                        ui.available_width(),
                        ui.available_height() * 0.60,
                    )));
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
        ui.label(RichText::new(format!("{} · {}", hit.kind, hit.path)).weak());
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
    fn models(&mut self, ui: &mut egui::Ui) {
        ui.heading("Models & hardware");
        ui.group(|ui|{ui.heading("Status");for (name,path,selected)in [("Local llama-server",&self.s.server,&mut self.runtime_setup),("Text model",&self.s.model,&mut self.model_setup),("Photos/audio/video encoder",&self.s.projector,&mut self.encoder_setup),("OCR",&self.s.ocr,&mut self.ocr_setup)]{ui.horizontal(|ui|{ui.checkbox(selected,name);ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{let installed=Path::new(path).is_file();let loaded=installed&&self.engine.models.loaded.load(Ordering::Relaxed)&&name!="OCR";ui.label(RichText::new(if loaded{"Loaded"}else if installed{"Installed · unloaded"}else{"Not installed"}).color(if loaded{Color32::GREEN}else if installed{Color32::YELLOW}else{Color32::LIGHT_RED}));});});}ui.label("Models load lazily for Full indexing or semantic search, and unload after five minutes idle. Names, apps and settings do not load a model.");ui.horizontal(|ui|{if ui.add_enabled(!self.setup_busy,egui::Button::new("Download and install selected")).clicked(){self.save();self.setup_busy=true;self.setup.install(self.s.clone(),self.runtime_setup,self.model_setup,self.encoder_setup,self.ocr_setup);}if ui.button("Stop / unload models").clicked(){let models=self.engine.models.clone();std::thread::spawn(move||{let _=models.stop();});}if self.setup_busy&&ui.button("Cancel setup").clicked(){self.setup.cancel.store(true,Ordering::Relaxed);}});});
        combo(
            ui,
            "hardware",
            &mut self.s.hardware,
            &["CPU / system RAM", "Dedicated GPU", "Integrated GPU"],
        );
        egui::ComboBox::from_id_salt("device_choice")
            .selected_text(if self.s.device.is_empty() {
                "Select a GPU / iGPU"
            } else {
                &self.s.device
            })
            .show_ui(ui, |ui| {
                for (id, description) in &self.devices {
                    ui.selectable_value(
                        &mut self.s.device,
                        id.clone(),
                        format!("{id} — {description}"),
                    );
                }
            });
        field(ui, "Device identifier", &mut self.s.device);
        if ui.button("List devices").clicked() {
            let server = self.s.server.clone();
            let tx = self.engine.tx.clone();
            std::thread::spawn(move || {
                let mut c = std::process::Command::new(server);
                c.arg("--list-devices");
                crate::content::hidden(&mut c);
                let result = c
                    .output()
                    .map(|o| {
                        format!(
                            "{}\n{}",
                            String::from_utf8_lossy(&o.stdout),
                            String::from_utf8_lossy(&o.stderr)
                        )
                    })
                    .unwrap_or_else(|e| e.to_string());
                let devices = crate::platform::parse_devices(&result);
                let _ = tx.send(Event::Devices(devices));
                let _ = tx.send(Event::Log("Info".into(), result));
            });
            self.status = "Devices will appear in Logs; enter the device identifier".into();
        }
        combo(ui, "quant", &mut self.s.quant, &["Q8_0", "BF16"]);
        path_field(ui, "llama-server", &mut self.s.server);
        path_field(ui, "EmbeddingGemma 2 GGUF", &mut self.s.model);
        path_field(ui, "Matching mmproj", &mut self.s.projector);
        field(
            ui,
            "Remote llama-server URL (blank uses local)",
            &mut self.s.remote_url,
        );
        secret(ui, "Remote bearer key", &mut self.s.remote_key);
        secret(
            ui,
            "GitHub download token (optional)",
            &mut self.s.github_token,
        );
        secret(ui, "Hugging Face token (optional)", &mut self.s.hf_token);
        path_field(ui, "Tesseract OCR", &mut self.s.ocr);
        field(ui, "OCR languages", &mut self.s.ocr_languages);
        path_field(ui, "FFmpeg (bundled by default)", &mut self.s.ffmpeg);
        ui.checkbox(&mut self.s.index_images, "Index image meaning");
        ui.checkbox(&mut self.s.index_audio, "Index audio moments");
        ui.checkbox(&mut self.s.index_video, "Index video frames");
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
        combo(ui, "theme", &mut self.s.theme, &["Dark", "Light"]);
        ui.horizontal(|ui| {
            ui.label("Theme color");
            ui.color_edit_button_srgb(&mut self.s.accent);
        });
        ui.add(egui::Slider::new(&mut self.s.popup_opacity, 0.1..=1.0).text("Popup opacity"));
        ui.add(egui::Slider::new(&mut self.s.popup_gradient, 0.0..=1.0).text("Popup gradient"));
        ui.checkbox(
            &mut self.s.explorer_integration,
            "Double Ctrl in File Explorer opens quick search",
        );
        ui.checkbox(
            &mut self.s.dialog_integration,
            "Double Ctrl in Open/Save dialogs; send selected paths to the dialog",
        );
        ui.label("Standard Windows dialogs are supported. Custom and elevated dialogs may require Copy path.");
        ui.checkbox(
            &mut self.s.close_to_tray,
            "Close hides the window; use Exit to stop VectorDash",
        );
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
            ctx.set_visuals(if self.s.theme == "Light" {
                egui::Visuals::light()
            } else {
                egui::Visuals::dark()
            });
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
        ui.horizontal(|ui| {
            ui.label("Time");
            ui.add(egui::DragValue::new(&mut self.s.schedule_hour).range(0..=23));
            ui.add(egui::DragValue::new(&mut self.s.schedule_minute).range(0..=59));
        });
        ui.add(egui::Slider::new(&mut self.s.idle_seconds, 30..=3600).text("User idle seconds"));
        ui.label("Idle eligibility is checked between files, not between embedding batches, so the model does not repeatedly pause because of its own activity.");
        if ui.button("Save schedule").clicked() {
            self.save();
        }
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
        let key = self.s.popup_opacity.to_bits() as u64
            | ((self.quick as u64) << 32)
            | ((self.s.accent[0] as u64) << 33)
            | ((self.s.accent[1] as u64) << 41)
            | ((self.s.accent[2] as u64) << 49);
        if self.material_key != Some(key) {
            if let Ok(handle) = frame.window_handle() {
                if let raw_window_handle::RawWindowHandle::Win32(win) = handle.as_raw() {
                    crate::platform::material(
                        win.hwnd.get(),
                        self.quick,
                        self.s.accent,
                        self.s.popup_opacity,
                    );
                    self.material_key = Some(key);
                }
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
        self.engine.models.idle_unload();
        if !self.indexing && !self.setup_busy && self.s.schedule != "Manual" && self.smoke.is_none()
        {
            use chrono::Timelike;
            let now = chrono::Local::now();
            let slot = if self.s.schedule == "When idle" {
                format!("{}-{}", now.date_naive(), now.hour())
            } else {
                now.date_naive().to_string()
            };
            let time = now.hour() == self.s.schedule_hour && now.minute() >= self.s.schedule_minute;
            let idle = crate::platform::idle_seconds() >= self.s.idle_seconds as u64;
            if slot != self.last_schedule
                && match self.s.schedule.as_str() {
                    "At selected time" => time,
                    "When idle" => idle,
                    "Time + idle" => time && idle,
                    _ => false,
                }
            {
                self.last_schedule = slot;
                self.start_index(false);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(50));
        self.titlebar(ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (key, action) in [
                        ("Esc", "dismiss"),
                        ("Tab", "types"),
                        ("Ctrl+C", "copy"),
                        ("Space", "preview"),
                        ("Enter", "open"),
                    ] {
                        ui.label(action);
                        ui.label(RichText::new(key).strong().color(self.accent()));
                    }
                });
            });
        });
        if !self.quick {
            egui::SidePanel::left("navigation")
                .exact_width(if self.collapsed { 60.0 } else { 190.0 })
                .show(ctx, |ui| {
                    for (i, label) in PAGES.iter().enumerate() {
                        let title = if self.collapsed {
                            ["⌕", "▣", "▦", "◷", "⚙", "▤"][i]
                        } else {
                            label
                        };
                        let response = ui.add(
                            egui::Button::new(title)
                                .fill(if i == 0 {
                                    self.accent()
                                } else if self.page == i {
                                    self.accent().gamma_multiply(0.5)
                                } else {
                                    Color32::TRANSPARENT
                                })
                                .min_size(egui::vec2(ui.available_width(), 42.0)),
                        );
                        if response.clicked() {
                            self.page = i;
                        }
                    }
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        if ui.button("Exit VectorDash").clicked() {
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
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                780.0, 185.0,
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                940.0,
                if self.query.is_empty() { 185.0 } else { 570.0 },
            )));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(18.0).fill(if self.quick {
                Color32::from_rgba_unmultiplied(42, 27, 53, (self.s.popup_opacity * 255.0) as u8)
            } else {
                ctx.style().visuals.panel_fill
            }))
            .show(ctx, |ui| {
                if self.quick {
                    let rect = ui.max_rect();
                    let a = self.s.accent;
                    let alpha = (self.s.popup_opacity * 200.0) as u8;
                    let base: [u8; 3] = if self.s.theme != "Light" {
                        [24, 18, 34]
                    } else {
                        [245, 243, 250]
                    };
                    let g = (self.s.popup_gradient * 45.0) as u8;
                    let top = Color32::from_rgba_unmultiplied(
                        base[0].saturating_add(g),
                        base[1].saturating_add(g / 2),
                        base[2].saturating_add(g),
                        alpha,
                    );
                    let bottom = Color32::from_rgba_unmultiplied(base[0], base[1], base[2], alpha);
                    let mut mesh = egui::Mesh::default();
                    for (pos, color) in [
                        (rect.left_top(), top),
                        (
                            rect.right_top(),
                            Color32::from_rgba_unmultiplied(a[0] / 4, a[1] / 4, a[2] / 3, alpha),
                        ),
                        (rect.right_bottom(), bottom),
                        (rect.left_bottom(), bottom),
                    ] {
                        mesh.colored_vertex(pos, color);
                    }
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    ui.painter().add(egui::Shape::mesh(mesh));
                }
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
            } else if self.quick {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
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
        if self.large_preview {
            let mut open = true;
            egui::Window::new("Quick Look")
                .open(&mut open)
                .default_size([760.0, 550.0])
                .show(ctx, |ui| self.preview(ui, ctx));
            if !open {
                self.large_preview = false;
            }
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
            if self.startup.elapsed() > Duration::from_secs(3) {
                let data = serde_json::json!({"ok":true,"native_rust":true,"model_loaded":self.engine.models.loaded.load(Ordering::Relaxed),"frames":self.frames,"max_frame_interval_ms":self.max_frame_ms});
                let _ = std::fs::write(report, data.to_string());
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        if self.quick {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            [0.055, 0.055, 0.075, 1.0]
        }
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.shutdown.store(true, Ordering::Relaxed);
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
