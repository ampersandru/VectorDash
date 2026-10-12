use crate::{
    config::{Settings, mode},
    content,
    db::{Database, Hit, Stats},
    models::Models,
};
use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Progress {
    pub phase: String,
    pub found: u64,
    pub completed: u64,
    pub changed: u64,
    pub reused: u64,
    pub skipped: u64,
    pub total: Option<u64>,
    pub current: String,
    pub requests: u64,
    pub started: i64,
}
#[derive(Clone)]
pub struct NotifySender {
    inner: crossbeam_channel::Sender<Event>,
    ctx: Option<eframe::egui::Context>,
    log_path: Option<PathBuf>,
}
impl NotifySender {
    pub fn new(
        inner: crossbeam_channel::Sender<Event>,
        ctx: Option<eframe::egui::Context>,
    ) -> Self {
        Self {
            inner,
            ctx,
            log_path: None,
        }
    }
    pub fn with_log(mut self, path: PathBuf) -> Self {
        self.log_path = Some(path);
        self
    }
    fn record(&self, event: &Event) {
        if let (Event::Log(level, message), Some(path)) = (event, &self.log_path) {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(
                    file,
                    "{level} {} {message}",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                );
            }
        }
    }
    pub fn send(&self, event: Event) -> Result<(), crossbeam_channel::SendError<Event>> {
        self.record(&event);
        let wake = matches!(&event, Event::Summon(..) | Event::Full | Event::Quit);
        // Windows must be shown before a full channel can wait for its UI receiver.
        if wake {
            crate::platform::wake_main_window();
        }
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
        if matches!(
            &event,
            Event::Progress(..) | Event::Download(..) | Event::Log(..)
        ) {
            return match self.inner.try_send(event) {
                Ok(()) | Err(crossbeam_channel::TrySendError::Full(_)) => Ok(()),
                Err(crossbeam_channel::TrySendError::Disconnected(event)) => {
                    Err(crossbeam_channel::SendError(event))
                }
            };
        }
        self.inner.send(event)
    }
    pub fn try_send(&self, event: Event) -> Result<(), crossbeam_channel::TrySendError<Event>> {
        self.record(&event);
        if matches!(&event, Event::Summon(..) | Event::Full | Event::Quit) {
            crate::platform::wake_main_window();
        }
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
        self.inner.try_send(event)
    }
}
impl From<crossbeam_channel::Sender<Event>> for NotifySender {
    fn from(inner: crossbeam_channel::Sender<Event>) -> Self {
        Self::new(inner, None)
    }
}
#[derive(Clone)]
pub enum Event {
    Devices(Vec<(String, String)>),
    Download(String, u64, u64),
    Thumbnail(String, Result<Vec<u8>, String>),
    Converted(Result<Database, String>),
    Full,
    AutomaticStarted,
    Quit,
    Progress(Progress),
    IndexDone(Result<String, String>),
    Search(u64, Result<Vec<Hit>, String>, f64),
    Stats(Stats, HashMap<String, crate::db::SourceStats>),
    Log(String, String),
    Setup(Result<Vec<(String, String)>, String>),
    Summon(isize, String),
    Preview(String, Result<Vec<u8>, String>),
    Imported(Result<Settings, String>),
}
#[derive(Clone, Default)]
pub struct Control {
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
    pub automatic: Arc<AtomicBool>,
    pub running: Arc<AtomicBool>,
    pub automatic_started: Arc<std::sync::atomic::AtomicI64>,
    pub schedule: Arc<std::sync::Mutex<Settings>>,
}
impl Control {
    pub fn file_checkpoint(&self) -> Result<()> {
        self.check()?;
        while self.automatic.load(Ordering::Relaxed) {
            let settings = self.schedule.lock().unwrap().clone();
            anyhow::ensure!(settings.schedule != "Manual", "Automatic schedule disabled");
            if settings.schedule != "When idle" && settings.schedule != "Time + idle" {
                break;
            }
            if crate::scheduling::idle_allowed(&settings, crate::scheduling::metrics()).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
            self.check()?;
        }
        Ok(())
    }
    pub fn check(&self) -> Result<()> {
        if self.automatic.load(Ordering::Relaxed) {
            let settings = self.schedule.lock().unwrap();
            anyhow::ensure!(settings.schedule != "Manual", "Automatic schedule disabled");
            anyhow::ensure!(
                chrono::Local::now().timestamp() - self.automatic_started.load(Ordering::Relaxed)
                    < settings.max_run_minutes as i64 * 60,
                "Maximum automatic run duration reached"
            );
        }
        while self.paused.load(Ordering::Relaxed) {
            anyhow::ensure!(
                !self.cancel.load(Ordering::Relaxed),
                "Indexing cancelled; completed files retained"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        anyhow::ensure!(
            !self.cancel.load(Ordering::Relaxed),
            "Indexing cancelled; completed files retained"
        );
        Ok(())
    }
}
#[derive(Clone)]
pub struct Engine {
    pub db: Database,
    pub models: Models,
    pub control: Control,
    pub tx: NotifySender,
    pub searching: Arc<AtomicBool>,
    pub vectors: crate::vector_cache::Cache,
}
impl Engine {
    pub fn log(&self, level: &str, message: impl Into<String>) {
        let text = message.into();
        let _ = self.tx.send(Event::Log(level.into(), text));
    }
    pub fn refresh_stats(&self, s: &Settings) {
        let d = self.db.clone();
        let tx = self.tx.clone();
        let roots = s.roots.clone();
        std::thread::spawn(move || {
            if let Ok(stats) = d.stats() {
                let sources = roots
                    .iter()
                    .filter_map(|r| d.source_stats(&r.path).ok().map(|v| (r.path.clone(), v)))
                    .collect();
                let _ = tx.send(Event::Stats(stats, sources));
            }
        });
    }
    pub fn start_scheduler(&self, shutdown: Arc<AtomicBool>) {
        let engine = self.clone();
        std::thread::spawn(move || {
            let mut retry = Instant::now();
            while !shutdown.load(Ordering::Relaxed) {
                engine.models.idle_unload();
                if !engine.control.running.load(Ordering::Relaxed) && Instant::now() >= retry {
                    let settings = engine.control.schedule.lock().unwrap().clone();
                    if !settings.roots.is_empty() {
                        if let Ok(slot) = crate::scheduling::due(
                            &settings,
                            chrono::Local::now(),
                            crate::scheduling::metrics(),
                        ) {
                            if engine
                                .control
                                .running
                                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                                .is_ok()
                            {
                                let mut settings = settings;
                                settings.last_schedule_slot = slot;
                                *engine.control.schedule.lock().unwrap() = settings.clone();
                                let _ = settings.save(&engine.db.home);
                                engine.control.automatic.store(true, Ordering::Relaxed);
                                engine
                                    .control
                                    .automatic_started
                                    .store(chrono::Local::now().timestamp(), Ordering::Relaxed);
                                let _ = engine.tx.send(Event::AutomaticStarted);
                                engine.launch_index(settings, false);
                            }
                            retry = Instant::now() + Duration::from_secs(300);
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
    }
    pub fn index(&self, s: Settings, force: bool) {
        if self
            .control
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        self.control.automatic.store(false, Ordering::Relaxed);
        self.launch_index(s, force);
    }
    fn launch_index(&self, s: Settings, force: bool) {
        self.control.cancel.store(false, Ordering::Relaxed);
        self.control.paused.store(false, Ordering::Relaxed);
        let owner = self.clone();
        std::thread::spawn(move || {
            let result = owner.index_inner(&s, force).map_err(|e| e.to_string());
            if owner.control.automatic.load(Ordering::Relaxed) {
                let mut settings = owner.control.schedule.lock().unwrap();
                if result.is_ok() {
                    settings.last_success = chrono::Local::now().timestamp();
                }
                settings.schedule_history.insert(
                    0,
                    format!(
                        "{} — {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M"),
                        result
                            .as_ref()
                            .map(String::as_str)
                            .unwrap_or("Stopped / failed")
                    ),
                );
                settings.schedule_history.truncate(20);
                let _ = settings.save(&owner.db.home);
            }
            owner.control.automatic.store(false, Ordering::Relaxed);
            owner.control.running.store(false, Ordering::SeqCst);
            owner.refresh_stats(&s);
            let _ = owner.tx.send(Event::IndexDone(result));
        });
    }
    fn index_inner(&self, s: &Settings, force: bool) -> Result<String> {
        anyhow::ensure!(!s.roots.is_empty(), "Add a source before indexing");
        let start = Instant::now();
        let epoch = chrono::Utc::now().timestamp_millis();
        let mut progress = Progress {
            phase: if s.count_files {
                "Counting"
            } else {
                "Indexing"
            }
            .into(),
            started: chrono::Utc::now().timestamp(),
            ..Default::default()
        };
        let conn = self.db.connect(false)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS native_content(path TEXT PRIMARY KEY,fingerprint TEXT,error TEXT); CREATE TABLE IF NOT EXISTS source_seen(path TEXT PRIMARY KEY,source TEXT,epoch INTEGER,added INTEGER);CREATE INDEX IF NOT EXISTS seen_source ON source_seen(source,epoch);")?;
        // Counting is optional and happens only when explicitly requested. Streaming
        // indexing processes the first eligible file without waiting for a full scan.
        let mut roots = s
            .roots
            .iter()
            .map(|r| PathBuf::from(&r.path))
            .collect::<Vec<_>>();
        roots.sort_by_key(|p| p.components().count());
        let mut selected = vec![];
        for root in roots {
            if !selected.iter().any(|p: &PathBuf| root.starts_with(p)) {
                selected.push(root);
            }
        }
        if s.count_files {
            let mut total = 0;
            for root in &selected {
                self.control.check()?;
                for entry in walker(root, s, &self.db.home) {
                    match entry {
                        Ok(entry) => {
                            self.control.check()?;
                            if entry.file_type().is_file() {
                                total += 1;
                                if total % 1000 == 0 {
                                    progress.found = total;
                                    let _ = self.tx.send(Event::Progress(progress.clone()));
                                }
                            }
                        }
                        Err(e) => {
                            self.log(
                                "Warning",
                                format!("Counting skipped an inaccessible path: {e}"),
                            );
                            if e.io_error()
                                .and_then(|e| e.raw_os_error())
                                .is_some_and(|n| [53, 64, 67, 121, 1231, 1232].contains(&n))
                            {
                                self.log(
                                    "Warning",
                                    "Network source unavailable; stopping its count early",
                                );
                                break;
                            }
                        }
                    }
                }
            }
            progress.total = Some(total);
            progress.found = 0;
            progress.phase = "Indexing".into();
        }
        let mut last = Instant::now();
        let mut tx_entries = 0u32;
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let mut complete_roots = vec![];
        let fingerprint = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            s.model,
            s.projector,
            s.remote_url,
            s.index_images,
            s.index_audio,
            s.index_video,
            s.ocr,
            s.code_search
        );
        let outcome = (|| -> Result<()> {
            for root in selected {
                self.control.file_checkpoint()?;
                let root_s = root.to_string_lossy().into_owned();
                if !root.exists() {
                    self.log(
                        "Warning",
                        format!("Source unavailable; previous records retained: {root_s}"),
                    );
                    progress.skipped += 1;
                    continue;
                }
                let mut source_failed = false;
                for entry in walker(&root, s, &self.db.home) {
                    self.control.file_checkpoint()?;
                    let entry = match entry {
                        Ok(v) => v,
                        Err(e) => {
                            self.log("Warning", format!("Skipped inaccessible path: {e}"));
                            progress.skipped += 1;
                            source_failed = true;
                            if e.io_error()
                                .and_then(|x| x.raw_os_error())
                                .is_some_and(|n| [53, 64, 67, 121, 1231, 1232].contains(&n))
                            {
                                self.log("Warning","Network source failed; skipping remaining paths and retaining existing records");
                                break;
                            }
                            continue;
                        }
                    };
                    let path = entry.path();
                    if !s.allowed(path) {
                        continue;
                    }
                    let meta = match entry.metadata() {
                        Ok(v) => v,
                        Err(e) => {
                            self.log("Warning", format!("Metadata failed: {e}"));
                            progress.skipped += 1;
                            source_failed = true;
                            if e.io_error()
                                .and_then(|e| e.raw_os_error())
                                .is_some_and(|n| [53, 64, 67, 121, 1231, 1232].contains(&n))
                            {
                                break;
                            }
                            continue;
                        }
                    };
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::MetadataExt;
                        let attrs = meta.file_attributes();
                        if attrs & (0x2 | 0x400 | 0x1000 | 0x40000 | 0x400000) != 0 {
                            continue;
                        }
                    }
                    let is_file = entry.file_type().is_file();
                    let mt = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|t| t.as_nanos().min(i64::MAX as u128) as i64)
                        .unwrap_or(-1);
                    let indexing_mode = mode(&path.to_string_lossy(), s);
                    let changed = Database::touch_name(
                        &conn,
                        path,
                        if is_file { "file" } else { "folder" },
                        mt,
                        meta.len(),
                        indexing_mode,
                        &root_s,
                        epoch,
                    )?;
                    tx_entries += 1;
                    if !is_file {
                        continue;
                    }
                    progress.found += 1;
                    progress.current = path.to_string_lossy().into_owned();
                    if changed {
                        progress.changed += 1;
                    } else {
                        progress.reused += 1;
                    }
                    if indexing_mode == "full" {
                        let extension = content::extension(path);
                        let mut detail = String::new();
                        if content::is_image(&extension) || extension == "pdf" {
                            if s.ocr_enabled && !s.ocr.is_empty() {
                                detail.push_str("|ocr:png-v1");
                            }
                            if !s.ocr_enabled {
                                detail.push_str("|ocr:off");
                            }
                            if s.ocr_languages != "eng" {
                                detail.push_str(&format!("|ocr-lang:{}", s.ocr_languages));
                            }
                        }
                        if content::is_image(&extension) && s.visual_detail == "Detailed" {
                            detail.push_str("|visual:detailed");
                        }
                        if extension == "pdf" && s.pdf_visuals {
                            detail.push_str("|pdf:visual");
                        }
                        if content::is_video(&extension)
                            && (s.video_interval - 5.0).abs() > f32::EPSILON
                        {
                            detail.push_str(&format!("|video:{}", s.video_interval));
                        }
                        let key = format!("{mt}|{}|{fingerprint}{detail}", meta.len());
                        let previous: Option<(String, String)> = conn
                            .query_row(
                                "SELECT fingerprint,error FROM native_content WHERE path=?1",
                                [progress.current.as_str()],
                                |r| Ok((r.get(0)?, r.get(1)?)),
                            )
                            .optional()?;
                        let legacy = previous.is_none()
                            && !changed
                            && !force
                            && conn
                                .query_row(
                                    "SELECT 1 FROM segments WHERE path=?1 LIMIT 1",
                                    [progress.current.as_str()],
                                    |r| r.get::<_, i64>(0),
                                )
                                .optional()?
                                .is_some();
                        if force
                            || !legacy
                                && previous.as_ref().map(|p| p.0.as_str()) != Some(key.as_str())
                            || s.retry_failed && previous.as_ref().is_some_and(|p| !p.1.is_empty())
                        {
                            // Commit names before potentially slow parsing/inference. No writer lock is
                            // held while a GPU or remote model is working, and searches stay available.
                            let _ = self.tx.send(Event::Progress(progress.clone()));
                            conn.execute_batch("COMMIT")?;
                            tx_entries = 0;
                            let file_start = Instant::now();
                            let file_result = (|| -> Result<()> {
                                let token = crate::config::random_key();
                                let mut batch = vec![];
                                let mut count = 0usize;
                                let mut had_warning = false;
                                let process = |batch: &mut Vec<crate::content::Segment>,
                                               count: &mut usize|
                                 -> Result<()> {
                                    if batch.is_empty() {
                                        return Ok(());
                                    }
                                    anyhow::ensure!(
                                        !s.excluded_sensitive
                                            || !batch.iter().any(|r| sensitive(&r.text)),
                                        "Potential credentials detected; content excluded"
                                    );
                                    let vectors =
                                        self.models.segments(s, batch, &|| self.control.check())?;
                                    Database::stage(&conn, &token, *count, batch, &vectors)?;
                                    *count += batch.len();
                                    batch.clear();
                                    Ok(())
                                };
                                let parsed =
                                    crate::worker::visit(path, s, &self.control, &mut |record| {
                                        if record.kind == "warning" {
                                            had_warning = true;
                                            self.log("Warning", &record.text);
                                            return Ok(());
                                        }
                                        let media = !record.media.is_empty();
                                        if media {
                                            process(&mut batch, &mut count)?;
                                        }
                                        batch.push(record);
                                        if media || batch.len() >= s.batch_size.clamp(1, 16) {
                                            process(&mut batch, &mut count)?;
                                        }
                                        Ok(())
                                    });
                                if let Err(error) = parsed {
                                    let _ = conn.execute(
                                        "DELETE FROM native_staging WHERE token=?1",
                                        [&token],
                                    );
                                    return Err(error);
                                }
                                process(&mut batch, &mut count)?;
                                if count == 0 {
                                    Database::stage(&conn, &token, 0, &[], &[])?;
                                }
                                anyhow::ensure!(
                                    count > 0 || !had_warning,
                                    "No readable content was extracted; previous file content retained"
                                );
                                Database::finish_stage(&conn, &token, &progress.current, &key)?;
                                progress.requests +=
                                    count.div_ceil(s.batch_size.clamp(1, 16)) as u64;
                                Ok(())
                            })();
                            match file_result {
                                Ok(()) => self.log(
                                    "Info",
                                    format!(
                                        "Indexed {} in {:.2}s",
                                        progress.current,
                                        file_start.elapsed().as_secs_f64()
                                    ),
                                ),
                                Err(e) => {
                                    if self.control.cancel.load(Ordering::Relaxed) {
                                        return Err(e);
                                    }
                                    progress.skipped += 1;
                                    self.log(
                                        "Warning",
                                        format!("Skipped {}: {e}", progress.current),
                                    );
                                    conn.execute(
                                        "INSERT OR REPLACE INTO native_content VALUES(?1,?2,?3)",
                                        params![progress.current, key, e.to_string()],
                                    )?;
                                }
                            }
                            conn.execute_batch("BEGIN IMMEDIATE")?;
                        }
                    }
                    progress.completed += 1;
                    if tx_entries >= 1000 {
                        conn.execute_batch("COMMIT;BEGIN IMMEDIATE")?;
                        tx_entries = 0;
                        self.db.generation.fetch_add(1, Ordering::Relaxed);
                    }
                    if last.elapsed() > Duration::from_millis(100) {
                        let _ = self.tx.send(Event::Progress(progress.clone()));
                        last = Instant::now();
                    }
                }
                if !source_failed {
                    complete_roots.push(root_s);
                }
            }
            Ok(())
        })();
        if !conn.is_autocommit() {
            conn.execute_batch("COMMIT")?;
        }
        if outcome.is_ok() {
            for root in complete_roots {
                let mut stmt = conn.prepare(
                    "SELECT path FROM source_seen WHERE source=?1 AND epoch!=?2 LIMIT 1000",
                )?;
                loop {
                    let paths: Vec<String> = stmt
                        .query_map(params![root, epoch], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?;
                    if paths.is_empty() {
                        break;
                    }
                    conn.execute_batch("BEGIN IMMEDIATE")?;
                    for path in paths {
                        Database::delete(&conn, &path)?;
                    }
                    conn.execute_batch("COMMIT")?;
                }
            }
        }
        self.db.generation.fetch_add(1, Ordering::Relaxed);
        progress.phase = if outcome.is_ok() {
            "Complete"
        } else {
            "Cancelled"
        }
        .into();
        let _ = self.tx.send(Event::Progress(progress.clone()));
        self.log(
            "Info",
            format!(
                "Index scan/update {:.2}s; {} files; {} changed; {} reused",
                start.elapsed().as_secs_f64(),
                progress.found,
                progress.changed,
                progress.reused
            ),
        );
        outcome?;
        Ok(format!(
            "{} files · {} changed · {} unchanged · {} skipped · {:.1}s",
            progress.found,
            progress.changed,
            progress.reused,
            progress.skipped,
            start.elapsed().as_secs_f64()
        ))
    }
    pub fn search(
        &self,
        sequence: u64,
        q: String,
        s: Settings,
        search_mode: String,
        category: String,
        scope: String,
        excluded: String,
    ) {
        let owner = self.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            let result = owner
                .search_inner(&q, &s, &search_mode, &category, &scope, &excluded)
                .map_err(|e| e.to_string());
            let _ = owner.tx.send(Event::Search(
                sequence,
                result,
                start.elapsed().as_secs_f64() * 1000.0,
            ));
        });
    }
    pub fn search_inner(
        &self,
        q: &str,
        s: &Settings,
        mode: &str,
        category: &str,
        scope: &str,
        excluded: &str,
    ) -> Result<Vec<Hit>> {
        if q.trim().is_empty() {
            return Ok(vec![]);
        }
        let expression = q.replace("% of", "/100*");
        if let Ok(value) = meval::eval_str(&expression) {
            if q.chars().any(|c| "+-*/^()%".contains(c)) {
                return Ok(vec![Hit {
                    path: format!("{value}"),
                    kind: "calculator".into(),
                    text: q.into(),
                    score: 3.0,
                    reason: "Calculated locally".into(),
                    page: 0,
                    seconds: -1.0,
                    line: 0,
                }]);
            }
        }
        let mut hits = crate::platform::actions(q, s);
        if category == "Programs" || q.to_lowercase().starts_with("app:") {
            hits.retain(|h| h.kind == "application");
            return Ok(hits);
        }
        if q.to_lowercase().starts_with("settings:") {
            hits.retain(|h| h.kind == "settings");
            return Ok(hits);
        }
        if category != "All files" {
            hits.clear();
        }

        if mode != "Semantic" || category == "Folders" {
            hits.extend(crate::db::names(&self.db, q, s, category, scope, excluded)?);
        }
        if category != "Folders"
            && mode != "Names"
            && self
                .db
                .connect(true)?
                .query_row("SELECT 1 FROM segment_vectors LIMIT 1", [], |r| {
                    r.get::<_, i64>(0)
                })
                .optional()?
                .is_some()
        {
            hits.extend(self.semantic(q, s, category, scope, excluded)?);
        }
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        let mut paths = HashSet::new();
        hits.retain(|h| paths.insert(h.path.clone()));
        hits.truncate(100);
        Ok(hits)
    }
    fn semantic(
        &self,
        q: &str,
        s: &Settings,
        category: &str,
        scope: &str,
        excluded: &str,
    ) -> Result<Vec<Hit>> {
        let vector = self.models.query(s, q)?;
        let conn = self.db.connect(true)?;
        let cat = if category == "All files"
            && ["images ", "image ", "photos ", "pictures "]
                .iter()
                .any(|x| q.to_lowercase().starts_with(x))
        {
            "Images"
        } else {
            category
        };
        conn.execute_batch("BEGIN")?;
        let text_intent = q.chars().any(|c| c.is_ascii_digit())
            || [
                "error",
                "text",
                "screenshot",
                "invoice",
                "number",
                "declined",
            ]
            .iter()
            .any(|word| q.to_lowercase().contains(word));
        let mut file_maxima: HashMap<(String, String), f32> = HashMap::new();
        let mut pool: Vec<(f32, i64)> = vec![];
        let mut channel_totals: HashMap<String, (f32, u64)> = HashMap::new();
        if let Some(matrix) = self.vectors.get(&conn, vector.len(), &s.performance)? {
            let permitted = matrix
                .groups
                .iter()
                .map(|g| {
                    (s.code_search || g.namespace != "code")
                        && !(cat == "Images" && g.channel == "text" && !text_intent)
                        && crate::db::allowed(&g.path, &g.kind, s, cat, scope, excluded)
                })
                .collect::<Vec<_>>();
            for (i, values) in matrix.values.chunks_exact(matrix.dimension).enumerate() {
                let group = matrix.group_ids[i];
                if !permitted[group] {
                    continue;
                }
                let score = crate::vector_cache::dot(values, &vector);
                let group = &matrix.groups[group];
                let maximum = file_maxima
                    .entry((group.path.clone(), group.channel.clone()))
                    .or_insert(score);
                *maximum = maximum.max(score);
                pool.push((score, matrix.ids[i]));
                if pool.len() > 800 {
                    pool.sort_by(|a, b| b.0.total_cmp(&a.0));
                    pool.truncate(400);
                }
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id,path,kind,vector,namespace,channel FROM segment_vectors WHERE dimension=?1",
            )?;
            let mut rows = stmt.query([vector.len()])?;
            while let Some(row) = rows.next()? {
                let id: i64 = row.get(0)?;
                let path: String = row.get(1)?;
                let kind: String = row.get(2)?;
                let namespace: String = row.get(4)?;
                let channel: String = row.get(5)?;
                if cat == "Images" && channel == "text" && !text_intent {
                    continue;
                }
                if !s.code_search && namespace == "code"
                    || !crate::db::allowed(&path, &kind, s, cat, scope, excluded)
                {
                    continue;
                }
                let bytes: Vec<u8> = row.get(3)?;
                let values = bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect::<Vec<_>>();
                let score = crate::vector_cache::dot(&values, &vector);
                let maximum = file_maxima.entry((path, channel)).or_insert(score);
                *maximum = maximum.max(score);
                pool.push((score, id));
                if pool.len() > 800 {
                    pool.sort_by(|a, b| b.0.total_cmp(&a.0));
                    pool.truncate(400);
                }
            }
        }
        for ((_, channel), score) in file_maxima {
            let total = channel_totals.entry(channel).or_default();
            total.0 += score;
            total.1 += 1;
        }
        let mut hits = HashMap::<String, Hit>::new();
        pool.sort_by(|a, b| b.0.total_cmp(&a.0));
        pool.truncate(400);
        let tokens = q
            .split_whitespace()
            .map(str::to_lowercase)
            .filter(|t| !["a", "the", "of", "images", "photos", "in", "on"].contains(&t.as_str()))
            .collect::<Vec<_>>();
        for (score, id) in pool {
            let (path, kind, text, loc): (String, String, String, String) = conn.query_row(
                "SELECT path,kind,text,location FROM segments WHERE rowid=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
            if s.excluded_sensitive && sensitive(&text) {
                continue;
            }
            let location: serde_json::Value = serde_json::from_str(&loc)?;
            let exact = tokens
                .iter()
                .filter(|t| text.to_lowercase().contains(t.as_str()))
                .count() as f32
                / tokens.len().max(1) as f32;
            let mean = channel_totals
                .get(if location["embedding_source"] == "media" {
                    kind.as_str()
                } else {
                    "text"
                })
                .map(|(sum, count)| sum / (*count).max(1) as f32)
                .unwrap_or(0.0);
            let ranking = score + (score - mean).clamp(-0.15, 0.15) + exact * 0.25;
            let media = location["embedding_source"] == "media";
            let h = Hit {
                path: path.clone(),
                kind: kind.clone(),
                text: text.chars().take(8000).collect(),
                score: ranking,
                reason: if exact > 0.5 {
                    "Matches exact words and meaning"
                } else if media && kind == "image" {
                    "Matches the picture"
                } else if media {
                    "Matches this media moment"
                } else {
                    "Matches the meaning of this passage"
                }
                .into(),
                page: location["page"].as_u64().unwrap_or(0) as u32,
                seconds: location["seconds"].as_f64().unwrap_or(-1.0) as f32,
                line: location["line"].as_u64().unwrap_or(0) as u32,
            };
            if hits.get(&path).is_none_or(|p| p.score < h.score) {
                hits.insert(path, h);
            }
        }
        // Exact words independently retrieve passages that were not in the vector pool.
        let terms = tokens
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        if !terms.is_empty() && !(cat == "Images" && !text_intent) {
            let mut stmt=conn.prepare("SELECT s.path,s.kind,s.text,s.location FROM words JOIN segments s ON s.rowid=words.rowid WHERE words MATCH ?1 ORDER BY rank LIMIT 100")?;
            for row in stmt.query_map([format!("text: ({terms})")], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })? {
                let (path, kind, text, loc) = row?;
                if !crate::db::allowed(&path, &kind, s, cat, scope, excluded) {
                    continue;
                }
                if s.excluded_sensitive && sensitive(&text) {
                    continue;
                }
                let lower = text.to_lowercase();
                let exact = tokens.iter().filter(|t| lower.contains(t.as_str())).count() as f32
                    / tokens.len().max(1) as f32;
                let v: serde_json::Value = serde_json::from_str(&loc)?;
                let h = Hit {
                    path: path.clone(),
                    kind,
                    text: text.chars().take(8000).collect(),
                    score: exact * 0.9,
                    reason: "Matches words in the content".into(),
                    page: v["page"].as_u64().unwrap_or(0) as u32,
                    seconds: v["seconds"].as_f64().unwrap_or(-1.0) as f32,
                    line: v["line"].as_u64().unwrap_or(0) as u32,
                };
                if hits.get(&path).is_none_or(|old| old.score < h.score) {
                    hits.insert(path, h);
                }
            }
        }
        Ok(hits.into_values().collect())
    }
}
fn walker<'a>(
    root: &Path,
    s: &'a Settings,
    home: &'a Path,
) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> + 'a {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_open(16)
        .into_iter()
        .filter_entry(move |e| {
            s.allowed(e.path()) && !e.path().starts_with(home) && !e.file_type().is_symlink()
        })
}
pub fn sensitive(text: &str) -> bool {
    text.contains("PRIVATE KEY-----")
        || text.split_whitespace().any(|t| {
            t.starts_with("ghp_")
                || t.starts_with("github_pat_")
                || t.starts_with("sk-")
                || t.starts_with("AKIA") && t.len() >= 20
        })
        || [
            "password=",
            "password:",
            "api_key=",
            "api_key:",
            "client_secret=",
            "authorization: bearer ",
        ]
        .iter()
        .any(|t| text.to_lowercase().contains(t))
}

#[cfg(test)]
mod notification_tests {
    use super::*;
    #[test]
    fn hidden_progress_never_blocks() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let notify = NotifySender::from(tx);
        notify.send(Event::Progress(Progress::default())).unwrap();
        for _ in 0..1000 {
            notify.send(Event::Progress(Progress::default())).unwrap();
        }
        assert_eq!(rx.len(), 1);
    }
}

#[cfg(test)]
mod scheduler_worker_tests {
    use super::*;
    #[test]
    fn scheduling_runs_without_a_visible_ui() {
        let home = tempfile::tempdir().unwrap();
        let source = tempfile::Builder::new()
            .prefix("VectorDash-schedule-test-")
            .tempdir()
            .unwrap();
        std::fs::write(source.path().join("note.txt"), "file").unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.schedule = "When idle".into();
        s.check_cpu = false;
        s.check_gpu = false;
        s.check_user_idle = false;
        s.ac_only = false;
        s.roots = vec![crate::config::Source {
            path: source.path().to_string_lossy().into(),
            mode: "quick".into(),
        }];
        let db = Database::new(home.path(), &s).unwrap();
        let (tx, _rx) = crossbeam_channel::bounded(512);
        let engine = Engine {
            db: db.clone(),
            models: Models::default(),
            control: Control::default(),
            tx: tx.into(),
            searching: Arc::new(AtomicBool::new(false)),
            vectors: crate::vector_cache::Cache::default(),
        };
        *engine.control.schedule.lock().unwrap() = s;
        let signal = Arc::new(AtomicBool::new(false));
        engine.start_scheduler(signal.clone());
        let start = Instant::now();
        while db.stats().unwrap().files == 0 && start.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(30));
        }
        signal.store(true, Ordering::Relaxed);
        assert_eq!(db.stats().unwrap().files, 1);
    }
}
