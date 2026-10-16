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
struct PendingDocument {
    path: String,
    key: String,
    token: String,
    resume: usize,
    records: Vec<crate::content::Segment>,
    size: u64,
    modified: Option<std::time::SystemTime>,
    prepared_seconds: f64,
}
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
    pub content_chunks: u64,
    pub session_completed: u64,
    pub session_pending: u64,
    pub requests: u64,
    pub started: i64,
    pub estimated_seconds: Option<f64>,
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
        let wake = matches!(
            &event,
            Event::Summon(..)
                | Event::ScopedSummon(..)
                | Event::VoiceSummon(..)
                | Event::Full
                | Event::Quit
        );
        // A full reliable queue must wake its receiver before waiting for space.
        if wake {
            crate::platform::wake_main_window();
        }
        let result = if matches!(
            &event,
            Event::Progress(..)
                | Event::Download(..)
                | Event::Log(..)
                | Event::WatchStatus(..)
                | Event::ClipboardChanged
        ) {
            match self.inner.try_send(event) {
                Ok(()) | Err(crossbeam_channel::TrySendError::Full(_)) => Ok(()),
                Err(crossbeam_channel::TrySendError::Disconnected(event)) => {
                    Err(crossbeam_channel::SendError(event))
                }
            }
        } else {
            self.inner.send(event)
        };
        // Notify after enqueue: a waking frame may otherwise drain an empty queue
        // and go idle before Exit (or another action) has actually arrived.
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
        if wake && result.is_ok() {
            crate::platform::wake_main_window();
        }
        result
    }
    pub fn try_send(&self, event: Event) -> Result<(), crossbeam_channel::TrySendError<Event>> {
        self.record(&event);
        let wake = matches!(
            &event,
            Event::Summon(..)
                | Event::ScopedSummon(..)
                | Event::VoiceSummon(..)
                | Event::Full
                | Event::Quit
        );
        let result = self.inner.try_send(event);
        if result.is_ok() {
            if let Some(ctx) = &self.ctx {
                ctx.request_repaint();
            }
            if wake {
                crate::platform::wake_main_window();
            }
        }
        result
    }
}
impl From<crossbeam_channel::Sender<Event>> for NotifySender {
    fn from(inner: crossbeam_channel::Sender<Event>) -> Self {
        Self::new(inner, None)
    }
}
#[derive(Clone)]
pub enum Event {
    DictationState(u64, String),
    DictationDone(u64, Result<String, String>),
    DictationInstalled(Result<(), String>),
    ClipboardImage(i64, Result<Vec<u8>, String>),
    CaptureTargets(Result<Vec<crate::capture::Target>, String>),
    Screenshot(Result<Vec<u8>, String>),
    Update(u64, crate::updates::Update),
    AiChunk(u64, String),
    RemoteCatalog(
        usize,
        String,
        std::result::Result<crate::remote::Catalog, String>,
    ),
    ConnectionTest(usize, u64, Result<(), String>),
    AiMetrics(u64, crate::ai::Metrics),
    TtsCatalog(
        String,
        String,
        Result<(Vec<String>, Vec<String>, String), String>,
    ),
    TtsVoices(u64, String, String, Result<(Vec<String>, String), String>),
    TtsDone(u64, Result<(), String>),
    AiDone(u64, Result<(), String>),
    AiModels(String, Result<Vec<String>, String>),
    Launcher(u64, Result<Vec<crate::launcher::Item>, String>),
    ClipboardCleared(Result<(), String>),
    ClipboardChanged,
    ModelReady(std::result::Result<(), String>),
    IndexCleared(std::result::Result<String, String>),
    Devices(Vec<(String, String)>),
    Download(String, u64, u64),
    Thumbnail(String, Result<Vec<u8>, String>),
    Converted(Result<Database, String>),
    Full,
    AutomaticStarted,
    Quit,
    Progress(Progress),
    IndexDone(Result<String, String>),
    OpenDone(Result<(), String>),
    Search(u64, Result<Vec<Hit>, String>, f64),
    SearchPartial(u64, Vec<Hit>, f64),
    SearchWarning(u64, String),
    ScopeStatus(u64, bool),
    WatchStatus(String),
    Stats(Stats, HashMap<String, crate::db::SourceStats>),
    Log(String, String),
    Setup(Result<Vec<(String, String)>, String>),
    Summon(isize, String),
    ScopedSummon(isize, String),
    VoiceSummon(isize),
    Microphones(Result<Vec<String>, String>),
    ComponentStats(std::collections::HashMap<String, crate::resources::Usage>),
    ComponentRemoved(String, Result<(), String>),
    Preview(String, Result<Vec<u8>, String>),
    Imported(Result<Settings, String>),
}
#[derive(Clone, Default)]
pub struct Control {
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
    pub automatic: Arc<AtomicBool>,
    pub watching: Arc<AtomicBool>,
    stats_busy: Arc<AtomicBool>,
    stats_requested: Arc<AtomicBool>,
    stats_started: Arc<std::sync::Mutex<Option<Instant>>>,
    stats_roots: Arc<std::sync::Mutex<Vec<String>>>,
    pub last_run_success: Arc<AtomicBool>,
    pub successful_full_scan_started: Arc<std::sync::atomic::AtomicI64>,
    pub running: Arc<AtomicBool>,
    pub automatic_hold: Arc<AtomicBool>,
    pub index_maintenance: Arc<AtomicBool>,
    pub cancelled_by_user: Arc<AtomicBool>,
    pub sources_pending: Arc<AtomicBool>,
    pub restart_requested: Arc<AtomicBool>,
    pub restart_automatic: Arc<AtomicBool>,
    pub restart_watching: Arc<AtomicBool>,
    pub automatic_started: Arc<std::sync::atomic::AtomicI64>,
    pub schedule: Arc<std::sync::Mutex<Settings>>,
}
impl Control {
    pub fn cancel_by_user(&self) {
        self.cancelled_by_user.store(true, Ordering::SeqCst);
        self.automatic_hold.store(true, Ordering::SeqCst);
        self.sources_pending.store(false, Ordering::SeqCst);
        self.restart_requested.store(false, Ordering::SeqCst);
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn interrupt_sources(&self) {
        if self.cancelled_by_user.load(Ordering::Acquire) {
            return;
        }
        if !self.sources_pending.swap(true, Ordering::SeqCst) {
            self.restart_automatic
                .store(self.automatic.load(Ordering::Relaxed), Ordering::Relaxed);
            self.restart_watching
                .store(self.watching.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        self.automatic_hold.store(true, Ordering::SeqCst);
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn update_settings(&self, settings: Settings) {
        let mut current = self.schedule.lock().unwrap();
        let changed = source_signature(&current) != source_signature(&settings);
        if changed && self.running.load(Ordering::Acquire) {
            self.interrupt_sources();
        }
        *current = settings;
        if self.sources_pending.swap(false, Ordering::SeqCst) {
            self.restart_requested.store(true, Ordering::SeqCst);
        }
    }
    pub fn file_checkpoint(&self) -> Result<()> {
        self.check()?;
        while self.automatic.load(Ordering::Relaxed) {
            let settings = self.schedule.lock().unwrap().clone();
            anyhow::ensure!(
                self.watching.load(Ordering::Relaxed) || settings.schedule != "Manual",
                "Automatic schedule disabled"
            );
            if !self.watching.load(Ordering::Relaxed)
                && settings.schedule != "When idle"
                && settings.schedule != "Time + idle"
            {
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
        self.check_with(&|| false)
    }
    pub fn check_with(&self, stopped: &dyn Fn() -> bool) -> Result<()> {
        anyhow::ensure!(!stopped(), "Preparation stopped");
        if self.automatic.load(Ordering::Relaxed) {
            let settings = self.schedule.lock().unwrap();
            if self.watching.load(Ordering::Relaxed) && !settings.watch_changes {
                self.cancel.store(true, Ordering::Relaxed);
                anyhow::bail!("Folder monitoring disabled");
            }
            if settings.schedule == "Manual" && !self.watching.load(Ordering::Relaxed) {
                self.cancel.store(true, Ordering::Relaxed);
                anyhow::bail!("Automatic schedule disabled");
            }
            if chrono::Local::now().timestamp() - self.automatic_started.load(Ordering::Relaxed)
                >= settings.max_run_minutes as i64 * 60
            {
                self.cancel.store(true, Ordering::Relaxed);
                anyhow::bail!("Maximum automatic run duration reached");
            }
        }
        loop {
            anyhow::ensure!(!stopped(), "Preparation stopped");
            anyhow::ensure!(
                !self.cancel.load(Ordering::Relaxed),
                "Indexing cancelled; completed files retained"
            );
            let settings = self.schedule.lock().unwrap().clone();
            if crate::scheduling::resource_pause(&settings, crate::scheduling::metrics()).is_none()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        while self.paused.load(Ordering::Relaxed) {
            anyhow::ensure!(!stopped(), "Preparation stopped");
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
pub fn source_signature(s: &Settings) -> String {
    format!(
        "{:?}|{:?}|{}|{}|{}|{}",
        s.roots,
        s.exclusions,
        s.excluded_types,
        s.skip_windows_churn,
        s.code_search,
        crate::raw::stamp(s)
    )
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
pub(crate) fn dispatch_action(
    tx: NotifySender,
    action: impl FnOnce() -> Result<()> + Send + 'static,
) {
    std::thread::spawn(move || {
        let _ = tx.send(Event::OpenDone(action().map_err(|e| e.to_string())));
    });
}
impl Engine {
    pub fn log(&self, level: &str, message: impl Into<String>) {
        let text = message.into();
        let _ = self.tx.send(Event::Log(level.into(), text));
    }
    pub fn refresh_stats(&self, s: &Settings) {
        *self.control.stats_roots.lock().unwrap() =
            s.roots.iter().map(|r| r.path.clone()).collect();
        self.control.stats_requested.store(true, Ordering::Release);
        if self
            .control
            .stats_busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let owner = self.clone();
        std::thread::spawn(move || {
            loop {
                let delay = owner
                    .control
                    .stats_started
                    .lock()
                    .unwrap()
                    .map(|last| Duration::from_secs(5).saturating_sub(last.elapsed()))
                    .unwrap_or_default();
                if !delay.is_zero() {
                    std::thread::sleep(delay);
                }
                owner
                    .control
                    .stats_requested
                    .store(false, Ordering::Release);
                *owner.control.stats_started.lock().unwrap() = Some(Instant::now());
                let roots = owner.control.stats_roots.lock().unwrap().clone();
                if let Ok(stats) = owner.db.stats() {
                    let sources = roots
                        .iter()
                        .filter_map(|root| {
                            owner.db.source_stats(root).ok().map(|v| (root.clone(), v))
                        })
                        .collect();
                    let _ = owner.tx.send(Event::Stats(stats, sources));
                }
                owner.control.stats_busy.store(false, Ordering::Release);
                if !owner.control.stats_requested.load(Ordering::Acquire)
                    || owner
                        .control
                        .stats_busy
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                {
                    break;
                }
            }
        });
    }
    pub fn start_scheduler(&self, shutdown: Arc<AtomicBool>) {
        let unloader = self.clone();
        let stop = shutdown.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let settings = unloader.control.schedule.lock().unwrap().clone();
                if unloader.models.idle_unload(&settings) {
                    unloader.log(
                        "Info",
                        format!(
                            "Local models unloaded; {}",
                            crate::scheduling::model_hold(&settings, crate::scheduling::metrics())
                                .map(str::to_string)
                                .unwrap_or_else(|| format!(
                                    "idle for {} minute(s)",
                                    settings.unload_minutes
                                ))
                        ),
                    );
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        let engine = self.clone();
        std::thread::spawn(move || {
            let mut retry = Instant::now();
            let mut changes = crate::changes::Monitor::default();
            let mut last_watch_status = String::new();
            let mut acknowledged_full_scan = 0;
            let mut pruned_signature = engine
                .db
                .connect(true)
                .ok()
                .and_then(|c| {
                    c.query_row(
                        "SELECT value FROM metadata WHERE key='pruned_source_signature'",
                        [],
                        |r| r.get::<_, String>(0),
                    )
                    .ok()
                })
                .unwrap_or_default();
            while !shutdown.load(Ordering::Relaxed) {
                let settings = engine.control.schedule.lock().unwrap().clone();
                if !engine.control.running.load(Ordering::Relaxed) {
                    changes.complete(
                        engine.control.last_run_success.load(Ordering::Relaxed),
                        &engine.db.home,
                    );
                }
                if !engine.control.index_maintenance.load(Ordering::Acquire)
                    && engine.control.restart_requested.load(Ordering::Acquire)
                    && !engine.control.running.load(Ordering::Acquire)
                    && !engine.control.sources_pending.load(Ordering::Acquire)
                {
                    let automatic = engine.control.restart_automatic.load(Ordering::Relaxed);
                    if !automatic
                        || crate::scheduling::idle_allowed(&settings, crate::scheduling::metrics())
                            .is_ok()
                    {
                        if engine
                            .control
                            .running
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            engine
                                .control
                                .restart_requested
                                .store(false, Ordering::SeqCst);
                            engine.control.automatic_hold.store(false, Ordering::SeqCst);
                            engine.control.automatic.store(automatic, Ordering::Relaxed);
                            engine.control.watching.store(
                                engine.control.restart_watching.load(Ordering::Relaxed),
                                Ordering::Relaxed,
                            );
                            engine
                                .control
                                .automatic_started
                                .store(chrono::Local::now().timestamp(), Ordering::Relaxed);
                            engine.log(
                                "Info",
                                "Sources changed; restarting with current saved sources",
                            );
                            if automatic {
                                let _ = engine.tx.send(Event::AutomaticStarted);
                            }
                            engine.launch_index(settings.clone(), false);
                        }
                    }
                }
                changes.configure(&settings, &engine.db.home, &engine.tx);
                if let Some(held) = changes.restored_hold.take() {
                    if held {
                        engine.control.automatic_hold.store(true, Ordering::Release);
                    }
                }
                changes.persist_hold(engine.control.automatic_hold.load(Ordering::Acquire));
                changes.collect(&settings, &engine.db.home);
                let full_scan = engine
                    .control
                    .successful_full_scan_started
                    .load(Ordering::Acquire);
                if full_scan > acknowledged_full_scan
                    && changes.acknowledge_full_scan(full_scan, &settings).is_ok()
                {
                    acknowledged_full_scan = full_scan;
                    engine.log("Info","Completed full scan satisfied older queued monitoring work; later notifications remain targeted");
                }
                let watch_status = if engine.control.automatic_hold.load(Ordering::Acquire) {
                    if engine.control.sources_pending.load(Ordering::Acquire) {
                        "Sources changed; save to restart indexing with the updated sources".into()
                    } else if engine.control.restart_requested.load(Ordering::Acquire) {
                        "Source update queued; waiting for the previous scan and idle conditions"
                            .into()
                    } else {
                        "Automatic indexing paused after Cancel; Update index or Resume automatic indexing to continue".into()
                    }
                } else {
                    changes.status(&settings, engine.control.running.load(Ordering::Relaxed))
                };
                if watch_status != last_watch_status {
                    engine.log("Info", format!("File monitoring: {watch_status}"));
                    let _ = engine.tx.send(Event::WatchStatus(watch_status.clone()));
                    last_watch_status = watch_status;
                }
                if !engine.control.index_maintenance.load(Ordering::Acquire)
                    && !engine.control.automatic_hold.load(Ordering::Acquire)
                    && !engine.control.running.load(Ordering::Relaxed)
                    && changes.ready()
                    && crate::scheduling::idle_allowed(&settings, crate::scheduling::metrics())
                        .is_ok()
                {
                    if engine
                        .control
                        .running
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        let paths = changes.take();
                        engine.control.watching.store(true, Ordering::Relaxed);
                        engine.control.automatic.store(true, Ordering::Relaxed);
                        engine
                            .control
                            .automatic_started
                            .store(chrono::Local::now().timestamp(), Ordering::Relaxed);
                        let _ = engine.tx.send(Event::AutomaticStarted);
                        engine.log(
                            "Info",
                            format!(
                                "Folder changes queued: {}",
                                paths.as_ref().map_or_else(
                                    || "full recovery scan".into(),
                                    |p| format!("{} targeted paths", p.len())
                                )
                            ),
                        );
                        engine.launch_paths(settings.clone(), false, paths);
                    }
                }
                if !engine.control.index_maintenance.load(Ordering::Acquire)
                    && !engine.control.automatic_hold.load(Ordering::Acquire)
                    && !engine.control.running.load(Ordering::Relaxed)
                    && Instant::now() >= retry
                {
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
                let signature = format!("visibility-v1|{}", source_signature(&settings));
                if signature != pruned_signature
                    && !engine.control.running.load(Ordering::Acquire)
                    && !engine.control.index_maintenance.load(Ordering::Acquire)
                    && crate::scheduling::idle_allowed(&settings, crate::scheduling::metrics())
                        .is_ok()
                {
                    if let Ok(true) = engine.db.prune_step(&settings, &signature) {
                        pruned_signature = signature;
                        engine.vectors.clear();
                    }
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
    }
    pub fn clear_index(&self) {
        if self.control.index_maintenance.swap(true, Ordering::SeqCst) {
            return;
        }
        self.control.cancel_by_user();
        let engine = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<String> {
                let start = Instant::now();
                while engine.control.running.load(Ordering::Acquire) {
                    anyhow::ensure!(start.elapsed() < Duration::from_secs(120),
                        "Indexer is still stopping; index was not cleared. Try again after it finishes.");
                    std::thread::sleep(Duration::from_millis(50));
                }
                engine.db.clear_index()
            })().map_err(|e|e.to_string());
            engine.vectors.clear();
            engine
                .control
                .index_maintenance
                .store(false, Ordering::Release);
            engine.refresh_stats(&engine.control.schedule.lock().unwrap().clone());
            let _ = engine.tx.send(Event::IndexCleared(result));
        });
    }
    pub fn index(&self, s: Settings, force: bool) {
        if self.control.index_maintenance.load(Ordering::Acquire) {
            return;
        }
        if self
            .control
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        self.control
            .cancelled_by_user
            .store(false, Ordering::SeqCst);
        self.control.automatic_hold.store(false, Ordering::SeqCst);
        self.control
            .restart_requested
            .store(false, Ordering::SeqCst);
        self.control.sources_pending.store(false, Ordering::SeqCst);
        *self.control.schedule.lock().unwrap() = s.clone();
        self.control.automatic.store(false, Ordering::Relaxed);
        self.control.watching.store(false, Ordering::Relaxed);
        self.launch_index(s, force);
    }
    fn launch_index(&self, s: Settings, force: bool) {
        self.launch_paths(s, force, None);
    }
    fn launch_paths(&self, s: Settings, force: bool, paths: Option<Vec<PathBuf>>) {
        if self.control.index_maintenance.load(Ordering::Acquire) {
            self.control.running.store(false, Ordering::Release);
            return;
        }
        self.control.cancel.store(false, Ordering::Relaxed);
        if self.control.index_maintenance.load(Ordering::Acquire) {
            self.control.running.store(false, Ordering::Release);
            return;
        }
        self.control.paused.store(false, Ordering::Relaxed);
        let owner = self.clone();
        let full_scan = paths.is_none();
        let started = chrono::Utc::now().timestamp_millis();
        std::thread::spawn(move || {
            let result = owner.run_index(&s, force, paths).map_err(|e| e.to_string());
            if full_scan && result.is_ok() {
                owner
                    .control
                    .successful_full_scan_started
                    .store(started, Ordering::Release);
            }
            owner
                .control
                .last_run_success
                .store(result.is_ok(), Ordering::Relaxed);
            if let Err(error) = &result {
                owner.log("Error", format!("Index update failed: {error}"));
            }
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
            owner.control.watching.store(false, Ordering::Relaxed);
            owner.control.automatic.store(false, Ordering::Relaxed);
            owner.control.running.store(false, Ordering::SeqCst);
            owner.refresh_stats(&s);
            let _ = owner.tx.send(Event::IndexDone(result));
        });
    }
    pub(crate) fn index_inner(&self, s: &Settings, force: bool) -> Result<String> {
        self.run_index(s, force, None)
    }
    fn run_index(&self, s: &Settings, force: bool, paths: Option<Vec<PathBuf>>) -> Result<String> {
        let target_scope = paths.clone();
        let mut first = s.clone();
        let has_quick = s.roots.iter().any(|r| r.mode.eq_ignore_ascii_case("quick"));
        let has_full = s
            .roots
            .iter()
            .any(|r| !r.mode.eq_ignore_ascii_case("quick"));
        first.index_phase = if !has_full {
            "quick-final"
        } else if s.index_order == "Full first" || !has_quick {
            "full"
        } else {
            "quick"
        }
        .into();
        let summary = self.index_paths(&first, force, paths)?;
        let c = self.db.connect(false)?;
        crate::db::register_search(&c, s, "All files", "", "", None)?;
        c.create_scalar_function(
            "vd_run_scope",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8,
            move |ctx| {
                let path = ctx.get::<String>(0)?;
                Ok(target_scope.as_ref().is_none_or(|roots| {
                    roots
                        .iter()
                        .any(|root| crate::config::under(&path, &root.to_string_lossy()))
                }))
            },
        )?;
        c.create_scalar_function(
            "vd_available",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8,
            |ctx| {
                let path = ctx.get::<String>(0)?;
                Ok(std::path::Path::new(&path).is_file())
            },
        )?;
        loop {
            self.control.file_checkpoint()?;
            let mut stmt=c.prepare_cached("SELECT path FROM native_jobs j WHERE error='' AND vd_match(path,'text',0,0) AND CASE WHEN vd_run_scope(path) THEN vd_available(path) ELSE 0 END ORDER BY (SELECT coalesce(max(seq),-1) FROM native_staging WHERE token=j.token),rowid LIMIT 64")?;
            let paths = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
                .filter(|p| {
                    s.searched(Path::new(p)) && mode(p, s) == "full" && Path::new(p).is_file()
                })
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            if paths.is_empty() {
                break;
            }
            let mut later = s.clone();
            later.count_files = false;
            later.index_phase = "resume".into();
            self.index_paths(&later, false, Some(paths))?;
        }
        loop {
            self.control.file_checkpoint()?;
            let mut stmt = c.prepare_cached(
                "SELECT path FROM native_quick_queue WHERE vd_run_scope(path) LIMIT 256",
            )?;
            let queued = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if queued.is_empty() {
                break;
            }
            let paths = queued
                .iter()
                .filter(|p| s.searched(Path::new(p)))
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                let mut later = s.clone();
                later.count_files = false;
                later.index_phase = "quick-final".into();
                self.index_paths(&later, false, Some(paths))?;
            }
            for path in queued {
                c.execute("DELETE FROM native_quick_queue WHERE path=?1", [path])?;
            }
        }
        Ok(summary)
    }
    fn index_paths(
        &self,
        s: &Settings,
        force: bool,
        paths: Option<Vec<PathBuf>>,
    ) -> Result<String> {
        let partial = paths.is_some();
        use std::io::Write;
        let audit_path = self.db.home.join("files.log");
        if std::fs::metadata(&audit_path).is_ok_and(|m| m.len() > 8 * 1024 * 1024) {
            let _ = std::fs::rename(&audit_path, self.db.home.join("files.previous.log"));
        }
        let mut audit = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&audit_path)
            .ok()
            .map(std::io::BufWriter::new);
        let mut audit_bytes = std::fs::metadata(&audit_path).map(|m| m.len()).unwrap_or(0);
        let mut added = 0u64;
        let mut modified = 0u64;
        let run_kind = if partial {
            "Targeted changes"
        } else {
            "Recovery/scheduled scan"
        };
        self.log(
            "Info",
            format!("{run_kind} started; unchanged content is reused"),
        );
        anyhow::ensure!(!s.roots.is_empty(), "Add a source before indexing");
        let start = Instant::now();
        let epoch = chrono::Utc::now().timestamp_millis();
        let mut progress = Progress {
            phase: if s.count_files && !partial {
                "Counting"
            } else {
                "Indexing"
            }
            .into(),
            started: chrono::Utc::now().timestamp(),
            ..Default::default()
        };
        let conn = self.db.connect(false)?;
        Database::begin_session(&conn)?;
        let (complete, pending) = Database::session(&conn)?;
        progress.session_completed = complete;
        progress.session_pending = pending;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS native_content(path TEXT PRIMARY KEY,fingerprint TEXT,error TEXT); CREATE TABLE IF NOT EXISTS source_seen(path TEXT PRIMARY KEY,source TEXT,epoch INTEGER,added INTEGER);CREATE INDEX IF NOT EXISTS seen_source ON source_seen(source,epoch);")?;
        // Counting is optional and happens only when explicitly requested. Streaming
        // indexing processes the first eligible file without waiting for a full scan.
        let mut roots = paths.unwrap_or_else(|| {
            s.roots
                .iter()
                .map(|r| PathBuf::from(&r.path))
                .collect::<Vec<_>>()
        });
        roots.sort_by_key(|p| p.components().count());
        let mut selected = vec![];
        for root in roots {
            if !selected.iter().any(|p: &PathBuf| root.starts_with(p)) {
                selected.push(root);
            }
        }
        if partial {
            self.log(
                "Info",
                format!(
                    "Targeted scan scope: {} path(s); {}",
                    selected.len(),
                    selected
                        .iter()
                        .take(4)
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            );
        }
        let mut pending_stmt = conn.prepare_cached(
            "SELECT path FROM native_jobs WHERE error='' ORDER BY rowid LIMIT 64",
        )?;
        let pending = pending_stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let resume_paths: HashSet<String> = pending.iter().cloned().collect();
        let mut resumed_done = HashSet::new();
        for path in pending
            .into_iter()
            .rev()
            .filter(|_| s.index_phase.is_empty() && !partial)
        {
            let path = PathBuf::from(path);
            if s.searched(&path) && !selected.contains(&path) {
                selected.insert(0, path);
            }
        }
        drop(pending_stmt);
        if s.count_files && !partial {
            let mut total = 0;
            for root in &selected {
                self.control.file_checkpoint()?;
                for entry in walker(root, s, &self.db.home) {
                    match entry {
                        Ok(entry) => {
                            self.control.file_checkpoint()?;
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
                            if network_source_unavailable(&root, e.io_error()) {
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
        let mut last_estimate = Instant::now() - Duration::from_secs(3);
        let mut tx_entries = 0u32;
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let mut complete_roots = vec![];
        let fingerprint = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            s.model,
            s.projector,
            // Hosting location/model identifiers are not extraction settings.
            // Keep the field empty to retain a stable legacy stamp layout.
            String::new(),
            s.index_images,
            s.index_audio,
            s.index_video,
            s.ocr,
            s.code_search
        );
        let mut documents: Vec<PendingDocument> = vec![];
        let outcome = (|| -> Result<()> {
            for root in selected {
                self.control.file_checkpoint()?;
                let root_s = if partial {
                    s.roots
                        .iter()
                        .filter(|r| crate::config::under(&root.to_string_lossy(), &r.path))
                        .min_by_key(|r| r.path.len())
                        .map(|r| r.path.clone())
                        .unwrap_or_else(|| root.to_string_lossy().into_owned())
                } else {
                    root.to_string_lossy().into_owned()
                };
                if partial
                    && std::fs::symlink_metadata(&root)
                        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                {
                    let accessible = s.roots.iter().any(|r| {
                        crate::config::under(&root.to_string_lossy(), &r.path)
                            && Path::new(&r.path).exists()
                    });
                    if accessible {
                        let prefix =
                            format!("{}\\", root.to_string_lossy().trim_end_matches(['\\', '/']));
                        let upper = format!("{prefix}{}", char::MAX);
                        let mut statement = conn.prepare("SELECT path FROM names WHERE path=?1 OR (path>=?2 AND path<?3) LIMIT 500")?;
                        loop {
                            let rows = statement
                                .query_map(
                                    params![root.to_string_lossy().as_ref(), prefix, upper],
                                    |r| r.get::<_, String>(0),
                                )?
                                .collect::<rusqlite::Result<Vec<_>>>()?;
                            if rows.is_empty() {
                                break;
                            }
                            for path in rows {
                                self.control.file_checkpoint()?;
                                Database::delete(&conn, &path)?;
                            }
                        }
                    }
                    continue;
                }
                if !root.exists() {
                    self.log(
                        "Warning",
                        format!(
                            "Path unavailable; previous records retained: {} (source {root_s})",
                            root.display()
                        ),
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
                            if network_source_unavailable(&root, e.io_error()) {
                                self.log("Warning","Network source root is unavailable; skipping remaining paths and retaining existing records");
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
                            if network_source_unavailable(&root, e.io_error()) {
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
                    if is_file && indexing_mode == "quick" && s.index_phase == "full" {
                        progress.found += 1;
                        conn.execute(
                            "INSERT OR IGNORE INTO native_quick_queue VALUES(?1)",
                            [path.to_string_lossy().as_ref()],
                        )?;
                        conn.execute("INSERT INTO source_seen(path,source,epoch,added) SELECT ?1,?2,?3,0 WHERE EXISTS(SELECT 1 FROM names WHERE path=?1) ON CONFLICT(path) DO UPDATE SET source=excluded.source,epoch=excluded.epoch",params![path.to_string_lossy(),root_s,epoch])?;
                        continue;
                    }
                    let reason = Database::touch_name_detail(
                        &conn,
                        path,
                        if is_file { "file" } else { "folder" },
                        mt,
                        meta.len(),
                        indexing_mode,
                        &root_s,
                        epoch,
                    )?;
                    let changed = reason != "unchanged";
                    if changed {
                        let line = format!(
                            "{} · {} · {} · {}\n",
                            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                            reason,
                            indexing_mode,
                            path.display()
                        );
                        if audit_bytes + line.len() as u64 > 8 * 1024 * 1024 {
                            drop(audit.take());
                            if std::fs::rename(&audit_path, self.db.home.join("files.previous.log"))
                                .is_ok()
                            {
                                audit = std::fs::OpenOptions::new()
                                    .create(true)
                                    .append(true)
                                    .open(&audit_path)
                                    .ok()
                                    .map(std::io::BufWriter::new);
                            }
                            audit_bytes = 0;
                        }
                        if let Some(file) = &mut audit {
                            let _ = file.write_all(line.as_bytes());
                            audit_bytes += line.len() as u64;
                        }
                    }
                    tx_entries += 1;
                    if !is_file {
                        continue;
                    }
                    progress.found += 1;
                    progress.current = path.to_string_lossy().into_owned();
                    if changed {
                        progress.changed += 1;
                        if reason == "added" {
                            added += 1;
                        } else {
                            modified += 1;
                        }
                    } else {
                        progress.reused += 1;
                    }
                    progress.phase = if indexing_mode == "full" {
                        "Checking/reusing Full content"
                    } else {
                        "Quick names — no embeddings"
                    }
                    .into();
                    progress.content_chunks = 0;
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
                        if content::is_video(&extension) && s.skip_similar_frames {
                            detail.push_str("|dedupe:rgb-v1");
                        }
                        if content::is_video(&extension)
                            && (s.video_interval - 5.0).abs() > f32::EPSILON
                        {
                            detail.push_str(&format!("|video:{}", s.video_interval));
                        }
                        if ["html", "htm"].contains(&extension.as_str()) {
                            detail.push_str("|html-visible-v1");
                        }
                        if crate::raw::is_raw(&extension) {
                            detail.push_str(&crate::raw::stamp(s));
                        }
                        let raw_key = format!("{mt}|{}|{fingerprint}{detail}", meta.len());
                        let key = content_stamp(&raw_key, &extension).unwrap_or(raw_key);
                        let previous: Option<(String, String)> = conn
                            .query_row(
                                "SELECT fingerprint,error FROM native_content WHERE path=?1",
                                [progress.current.as_str()],
                                |r| Ok((r.get(0)?, r.get(1)?)),
                            )
                            .optional()?;
                        let legacy = !crate::raw::is_raw(&extension)
                            && previous.is_none()
                            && (!changed
                                || conn
                                    .query_row(
                                        "SELECT mtime,size FROM files WHERE path=?1",
                                        [progress.current.as_str()],
                                        |r| {
                                            Ok((
                                                r.get::<_, Option<i64>>(0)?,
                                                r.get::<_, Option<i64>>(1)?,
                                            ))
                                        },
                                    )
                                    .optional()?
                                    .is_some_and(|v| {
                                        v.0 == Some(mt)
                                            && v.1.and_then(|n| u64::try_from(n).ok())
                                                == Some(meta.len())
                                    }))
                            && !force
                            && conn
                                .query_row(
                                    "SELECT 1 FROM segments WHERE path=?1 LIMIT 1",
                                    [progress.current.as_str()],
                                    |r| r.get::<_, i64>(0),
                                )
                                .optional()?
                                .is_some();
                        if !content::supports_content(&extension, s) {
                            progress.phase = "Filename only — unsupported content type".into();
                            if previous.as_ref().is_none_or(|p| p.0 != key) {
                                let has_content = conn
                                    .query_row(
                                        "SELECT 1 FROM segments WHERE path=?1 LIMIT 1",
                                        [progress.current.as_str()],
                                        |r| r.get::<_, i64>(0),
                                    )
                                    .optional()?
                                    .is_some();
                                if has_content {
                                    Database::save_segments(
                                        &conn,
                                        &progress.current,
                                        &[],
                                        &[],
                                        &key,
                                    )?;
                                } else {
                                    conn.execute(
                                        "INSERT OR REPLACE INTO native_content VALUES(?1,?2,'')",
                                        params![progress.current, key],
                                    )?;
                                }
                            }
                        } else if !resumed_done.contains(&progress.current)
                            && (force
                                || s.index_phase == "resume"
                                || resume_paths.contains(&progress.current)
                                || !legacy
                                    && !previous.as_ref().is_some_and(|p| {
                                        content_equivalent(&p.0, &key, &extension)
                                    })
                                || s.retry_failed
                                    && previous.as_ref().is_some_and(|p| !p.1.is_empty()))
                        {
                            let reason = if force {
                                "requested full rebuild"
                            } else if previous.is_none() {
                                "missing or unverified completed content metadata"
                            } else if changed {
                                "file metadata changed"
                            } else if ["html", "htm"].contains(&extension.as_str())
                                && previous
                                    .as_ref()
                                    .is_some_and(|p| !p.0.contains("html-visible-v1"))
                            {
                                "readable HTML extraction upgrade"
                            } else if s.index_phase == "resume"
                                || resume_paths.contains(&progress.current)
                            {
                                "resuming unfinished content"
                            } else {
                                "content extraction settings changed"
                            };
                            self.log(
                                "Info",
                                format!(
                                    "{} {}: {reason}",
                                    if s.index_phase == "quick" {
                                        "Queued Full content"
                                    } else {
                                        "Full indexing"
                                    },
                                    progress.current
                                ),
                            );
                            if resume_paths.contains(&progress.current) {
                                resumed_done.insert(progress.current.clone());
                            }
                            if s.index_phase == "quick" {
                                Database::prepare_job(&conn, &progress.current, &key)?;
                                progress.completed += 1;
                                continue;
                            }
                            let small_text = meta.len() <= 16 * 1024
                                && (["txt", "md", "csv", "log", "json", "yaml", "yml", "toml"]
                                    .contains(&extension.as_str())
                                    || s.code_search && content::is_code(&extension));
                            let small_image =
                                content::is_image(&extension) && meta.len() <= 16 * 1024 * 1024;
                            if small_text || small_image {
                                let (token, resume) =
                                    Database::prepare_job(&conn, &progress.current, &key)?;
                                documents.push(PendingDocument {
                                    path: progress.current.clone(),
                                    key,
                                    token,
                                    resume,
                                    records: vec![],
                                    size: meta.len(),
                                    modified: meta.modified().ok(),
                                    prepared_seconds: 0.0,
                                });
                                progress.completed += 1;
                                if documents.len() >= 32 {
                                    self.flush_documents(s, &conn, &mut documents, &mut progress)?;
                                }
                                continue;
                            }
                            self.flush_documents(s, &conn, &mut documents, &mut progress)?;
                            progress.phase = "Extracting Full content".into();
                            // Commit names before potentially slow parsing/inference. No writer lock is
                            // held while a GPU or remote model is working, and searches stay available.
                            let _ = self.tx.send(Event::Progress(progress.clone()));
                            conn.execute_batch("COMMIT")?;
                            tx_entries = 0;
                            let file_start = Instant::now();
                            let mut file_progress = progress.clone();
                            let file_result = (|| -> Result<usize> {
                                let (token, resume) =
                                    Database::prepare_job(&conn, &progress.current, &key)?;
                                let (start_page, prefix): (u32, usize) = if extension == "pdf"
                                    && resume > 0
                                {
                                    let page:u32=conn.query_row("SELECT coalesce(max(cast(json_extract(location,'$.page') AS INTEGER)),0) FROM native_staging WHERE token=?1",[&token],|r|r.get(0))?;
                                    let prefix=conn.query_row("SELECT count(*) FROM native_staging WHERE token=?1 AND cast(json_extract(location,'$.page') AS INTEGER)<?2",params![token,page],|r|r.get(0))?;
                                    (page, prefix)
                                } else {
                                    (0, 0)
                                };
                                // PDF can seek to its saved page. Media decoding must remain a
                                // single stream until timestamp resume is available; restarting it
                                // every few frames would multiply decoding work on long videos.
                                let sliced = !content::is_video(&extension)
                                    && !content::is_audio(&extension);
                                let turn_budget = if extension == "pdf" {
                                    32
                                } else {
                                    (resume / 4).max(32)
                                };
                                let mut seen = prefix;
                                let mut batch = vec![];
                                let mut count = resume;
                                if resume > 0 {
                                    self.log(
                                        "Info",
                                        format!(
                                            "Resuming {} after {resume} committed chunks",
                                            progress.current
                                        ),
                                    );
                                }
                                let mut had_warning = false;
                                let mut process = |batch: &mut Vec<crate::content::Segment>,
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
                                    file_progress.phase = format!(
                                        "EmbeddingGemma 2 — {}",
                                        if s.remote_embeddings() {
                                            "remote server".into()
                                        } else if s.hardware == "CPU / system RAM" {
                                            "local CPU".into()
                                        } else {
                                            format!("local GPU: {}", s.device)
                                        }
                                    );
                                    let _ = self.tx.send(Event::Progress(file_progress.clone()));
                                    let vectors = self
                                        .models
                                        .segments(s, batch, &|| self.control.file_checkpoint())?;
                                    Database::stage(&conn, &token, *count, batch, &vectors)?;
                                    *count += batch.len();
                                    if last_estimate.elapsed() > Duration::from_secs(2) {
                                        file_progress.estimated_seconds =
                                            crate::index_costs::estimate(&conn, s).ok().flatten();
                                        last_estimate = Instant::now();
                                    }
                                    file_progress.content_chunks = *count as u64;
                                    let _ = self.tx.send(Event::Progress(file_progress.clone()));
                                    batch.clear();
                                    Ok(())
                                };
                                let parsed = crate::worker::visit_checked(
                                    path,
                                    s,
                                    &self.control,
                                    start_page,
                                    &|| false,
                                    &mut |record| {
                                        if record.kind == "warning" {
                                            had_warning = true;
                                            self.log("Warning", &record.text);
                                            return Ok(());
                                        }
                                        seen += 1;
                                        if seen <= resume {
                                            return Ok(());
                                        }
                                        if sliced && seen > resume.saturating_add(turn_budget) {
                                            return Err(YieldFile.into());
                                        }
                                        let media = !record.media.is_empty();
                                        if batch.first().is_some_and(
                                            |r: &crate::content::Segment| {
                                                r.media.is_empty() != record.media.is_empty()
                                                    || !r.media.is_empty() && r.kind != record.kind
                                            },
                                        ) {
                                            process(&mut batch, &mut count)?;
                                        }
                                        batch.push(record);
                                        let bytes: usize = batch
                                            .iter()
                                            .map(|r| r.media.len() + r.text.len())
                                            .sum();
                                        if batch.len() >= s.batch_size.clamp(1, 16)
                                            || bytes >= 2 * 1024 * 1024
                                        {
                                            process(&mut batch, &mut count)?;
                                        }
                                        Ok(())
                                    },
                                );
                                let yielded = match parsed {
                                    Ok(()) => false,
                                    Err(error) if error.is::<YieldFile>() => true,
                                    Err(error) => return Err(error),
                                };
                                process(&mut batch, &mut count)?;
                                anyhow::ensure!(
                                    seen >= resume,
                                    "File extraction changed since its saved checkpoint"
                                );
                                if count == 0 {
                                    Database::stage(&conn, &token, 0, &[], &[])?;
                                }
                                anyhow::ensure!(
                                    count > 0 || !had_warning,
                                    "No readable content was extracted; previous file content retained"
                                );
                                let current = std::fs::metadata(path)?;
                                anyhow::ensure!(
                                    current.len() == meta.len()
                                        && current.modified().ok() == meta.modified().ok(),
                                    "File changed while parsing; retry on next update"
                                );
                                // Yield durable staged work without exposing an incomplete file.
                                // Replace its searchable content atomically only after extraction finishes.
                                if !yielded {
                                    Database::finish_stage(&conn, &token, &progress.current, &key)?;
                                }
                                if yielded {
                                    crate::index_costs::add_time(
                                        &conn,
                                        &progress.current,
                                        file_start.elapsed().as_secs_f64(),
                                    )?;
                                } else {
                                    crate::index_costs::finish(
                                        &conn,
                                        s,
                                        &progress.current,
                                        meta.len(),
                                        file_start.elapsed().as_secs_f64(),
                                    )?;
                                }
                                progress.requests += count
                                    .saturating_sub(resume)
                                    .div_ceil(s.batch_size.clamp(1, 16))
                                    as u64;
                                Ok(count)
                            })();
                            progress.content_chunks = file_progress.content_chunks;
                            match file_result {
                                Ok(chunks) => {
                                    self.log(
                                        "Info",
                                        format!(
                                            "Indexed {}: {} content chunks in {:.2}s",
                                            progress.current,
                                            chunks,
                                            file_start.elapsed().as_secs_f64()
                                        ),
                                    );
                                }
                                Err(e) => {
                                    if self.control.cancel.load(Ordering::Relaxed) {
                                        return Err(e);
                                    }
                                    if self.control.file_checkpoint().is_err() {
                                        return Err(e);
                                    }
                                    if crate::models::retryable(&e) {
                                        return Err(e);
                                    }
                                    conn.execute(
                                        "UPDATE native_jobs SET error=?1 WHERE path=?2",
                                        params![e.to_string(), progress.current],
                                    )?;
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
                        } else if previous.as_ref().is_none_or(|p| p.0 != key) {
                            conn.execute(
                                "INSERT OR REPLACE INTO native_content VALUES(?1,?2,'')",
                                params![progress.current, key],
                            )?;
                        }
                    }
                    progress.completed += 1;
                    if tx_entries >= 1000 {
                        conn.execute_batch("COMMIT;BEGIN IMMEDIATE")?;
                        tx_entries = 0;
                        self.db.generation.fetch_add(1, Ordering::Relaxed);
                    }
                    if last_estimate.elapsed() > Duration::from_secs(2) {
                        progress.estimated_seconds =
                            crate::index_costs::estimate(&conn, s).ok().flatten();
                        last_estimate = Instant::now();
                    }
                    if last.elapsed() > Duration::from_millis(100) {
                        if let Ok((complete, pending)) = Database::session(&conn) {
                            progress.session_completed = complete;
                            progress.session_pending = pending;
                        }
                        let _ = self.tx.send(Event::Progress(progress.clone()));
                        last = Instant::now();
                    }
                }
                if !source_failed && !partial {
                    complete_roots.push(root_s);
                }
            }
            self.flush_documents(s, &conn, &mut documents, &mut progress)?;
            Ok(())
        })();
        if !conn.is_autocommit() {
            conn.execute_batch("COMMIT")?;
        }
        if outcome.is_ok() {
            let cleanup_started = Instant::now();
            let mut removed = 0usize;
            self.log("Info", format!("Deleted-record cleanup started; {} completed source(s); scan counters are separate", complete_roots.len()));
            for root in complete_roots {
                self.control.file_checkpoint()?;
                progress.phase = "Removing deleted index records".into();
                progress.current = format!(
                    "{removed} records removed · {:.1}s",
                    cleanup_started.elapsed().as_secs_f64()
                );
                let _ = self.tx.send(Event::Progress(progress.clone()));
                let mut stmt = conn.prepare(
                    "SELECT path FROM source_seen WHERE source=?1 AND epoch<?2 UNION ALL SELECT path FROM source_seen WHERE source=?1 AND epoch>?2 LIMIT 128",
                )?;
                loop {
                    // Pause/cancel outside the transaction: never sleep holding
                    // a write lock, and finish at most one 128-path batch.
                    self.control.file_checkpoint()?;
                    let paths: Vec<String> = stmt
                        .query_map(params![root, epoch], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?;
                    if paths.is_empty() {
                        break;
                    }
                    Database::delete_batch(&conn, &paths)?;
                    removed += paths.len();
                    progress.current = format!(
                        "{removed} records removed · {:.1}s",
                        cleanup_started.elapsed().as_secs_f64()
                    );
                    let _ = self.tx.send(Event::Progress(progress.clone()));
                }
            }
            self.log(
                "Info",
                format!(
                    "Deleted-record cleanup complete: {removed} records removed in {:.3}s",
                    cleanup_started.elapsed().as_secs_f64()
                ),
            );
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
            "{run_kind} · {} files · {added} new · {modified} metadata/mode changes · {} changed · {} unchanged · {} skipped · {:.1}s",
            progress.found,
            progress.changed,
            progress.reused,
            progress.skipped,
            start.elapsed().as_secs_f64()
        ))
    }
    fn prepare_document(
        &self,
        s: &Settings,
        document: &mut PendingDocument,
        stop: &AtomicBool,
    ) -> Result<()> {
        let start = Instant::now();
        let path = Path::new(&document.path);
        let mut records = vec![];
        let mut bytes = 0usize;
        let mut warning = false;
        let ext = content::extension(path);
        let rust_image =
            ["png", "jpg", "jpeg", "webp", "bmp", "gif"].contains(&ext.as_str()) && !s.ocr_enabled;
        if content::is_image(&ext) && !rust_image {
            crate::worker::visit_checked(
                path,
                s,
                &self.control,
                0,
                &|| stop.load(Ordering::Acquire),
                &mut |record| {
                    if record.kind == "warning" {
                        warning = true;
                        self.log("Warning", &record.text);
                        return Ok(());
                    }
                    bytes += record.media.len() + record.text.len();
                    anyhow::ensure!(
                        records.len() < 128 && bytes <= 8 * 1024 * 1024,
                        "Image extraction exceeds bounded batch size"
                    );
                    records.push(record);
                    Ok(())
                },
            )?;
            anyhow::ensure!(
                !records.is_empty() || !warning,
                "No readable image content; previous content retained"
            );
        } else {
            records = content::extract(path, s, &|| {
                anyhow::ensure!(!stop.load(Ordering::Acquire), "Preparation stopped");
                self.control.check_with(&|| stop.load(Ordering::Acquire))
            })?;
        }
        anyhow::ensure!(
            !s.excluded_sensitive || !records.iter().any(|r| sensitive(&r.text)),
            "Potential credentials detected; content excluded"
        );
        anyhow::ensure!(
            records.len() <= 128
                && records
                    .iter()
                    .map(|r| r.text.len() + r.media.len())
                    .sum::<usize>()
                    <= 8 * 1024 * 1024,
            "Prepared document exceeds bounded batch size"
        );
        document.records = records;
        document.prepared_seconds = start.elapsed().as_secs_f64();
        Ok(())
    }
    fn flush_documents(
        &self,
        s: &Settings,
        c: &rusqlite::Connection,
        documents: &mut Vec<PendingDocument>,
        progress: &mut Progress,
    ) -> Result<()> {
        if documents.is_empty() {
            return Ok(());
        }
        if !c.is_autocommit() {
            c.execute_batch("COMMIT")?;
        }
        let input = std::mem::take(documents);
        let stopped = AtomicBool::new(false);
        let (tx, rx) = crossbeam_channel::bounded(1);
        let (advance, requests) = crossbeam_channel::bounded(1);
        advance.send(()).unwrap();
        let result = std::thread::scope(|scope| {
            let stop = &stopped;
            scope.spawn(move || {
                let mut input = input.into_iter().peekable();
                while input.peek().is_some() && !stop.load(Ordering::Acquire) {
                    loop {
                        match requests.recv_timeout(Duration::from_millis(50)) {
                            Ok(()) => break,
                            Err(crossbeam_channel::RecvTimeoutError::Timeout)
                                if !stop.load(Ordering::Acquire) =>
                            {
                                continue;
                            }
                            _ => return,
                        }
                    }
                    let mut batch = vec![];
                    let mut bytes = 0;
                    while batch.len() < s.batch_size.clamp(1, 16) && bytes < 2 * 1024 * 1024 {
                        let Some(mut document) = input.next() else {
                            break;
                        };
                        let error = self
                            .prepare_document(s, &mut document, stop)
                            .err()
                            .map(|e| e.to_string());
                        bytes += document
                            .records
                            .iter()
                            .map(|r| r.text.len() + r.media.len())
                            .sum::<usize>();
                        batch.push((document, error));
                    }
                    let mut pending = batch;
                    loop {
                        match tx.send_timeout(pending, Duration::from_millis(50)) {
                            Ok(()) => break,
                            Err(crossbeam_channel::SendTimeoutError::Timeout(value))
                                if !stop.load(Ordering::Acquire) =>
                            {
                                pending = value
                            }
                            _ => return,
                        }
                    }
                }
            });
            let result = (|| -> Result<()> {
                loop {
                    self.control.file_checkpoint()?;
                    let batch = match rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(batch) => batch,
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    };
                    let _ = advance.try_send(());
                    let mut ready = vec![];
                    for (document, error) in batch {
                        if let Some(error) = error {
                            c.execute(
                                "UPDATE native_jobs SET error=?1 WHERE path=?2",
                                params![error, document.path],
                            )?;
                            c.execute(
                                "INSERT OR REPLACE INTO native_content VALUES(?1,?2,?3)",
                                params![document.path, document.key, error],
                            )?;
                            progress.skipped += 1;
                            self.log("Warning", format!("Skipped {}: {error}", document.path));
                        } else {
                            ready.push(document)
                        }
                    }
                    self.commit_documents(s, c, &mut ready, progress)?;
                }
                Ok(())
            })();
            stopped.store(true, Ordering::Release);
            drop(advance);
            drop(rx);
            result
        });
        if c.is_autocommit() {
            c.execute_batch("BEGIN IMMEDIATE")?;
        }
        result
    }
    fn commit_documents(
        &self,
        s: &Settings,
        c: &rusqlite::Connection,
        documents: &mut Vec<PendingDocument>,
        progress: &mut Progress,
    ) -> Result<()> {
        if documents.is_empty() {
            return Ok(());
        }
        self.control.file_checkpoint()?;
        if !c.is_autocommit() {
            c.execute_batch("COMMIT")?;
        }
        let records = documents
            .iter()
            .flat_map(|d| d.records.iter().skip(d.resume).cloned())
            .collect::<Vec<_>>();
        progress.phase = format!(
            "EmbeddingGemma 2 — {} files in bounded batches",
            documents.len()
        );
        let _ = self.tx.send(Event::Progress(progress.clone()));
        let started = Instant::now();
        let ready_before = self.models.loaded.load(Ordering::Acquire) || s.remote_embeddings();
        let result = self
            .models
            .segments(s, &records, &|| self.control.file_checkpoint());
        let (vectors, failed) = match result {
            Ok(v) => (v, false),
            Err(error) => {
                if crate::models::retryable(&error) {
                    return Err(error);
                }
                (vec![], true)
            }
        };
        progress.requests += records.len().div_ceil(s.batch_size.clamp(1, 16)) as u64;
        let per_file = started.elapsed().as_secs_f64() / documents.len().max(1) as f64;
        let mut offset = 0usize;
        for document in documents.drain(..) {
            self.control.file_checkpoint()?;
            let remaining = document.records.len().saturating_sub(document.resume);
            let result = (|| -> Result<()> {
                let meta = std::fs::metadata(&document.path)?;
                anyhow::ensure!(
                    meta.len() == document.size && meta.modified().ok() == document.modified,
                    "File changed while queued; retrying on its next update"
                );
                let own = if failed {
                    self.models.segments(
                        s,
                        &document.records[document.resume.min(document.records.len())..],
                        &|| self.control.file_checkpoint(),
                    )?
                } else {
                    vectors[offset..offset + remaining].to_vec()
                };
                Database::stage(
                    c,
                    &document.token,
                    document.resume,
                    &document.records[document.resume.min(document.records.len())..],
                    &own,
                )?;
                self.control.file_checkpoint()?;
                Database::finish_stage(c, &document.token, &document.path, &document.key)?;
                if ready_before {
                    crate::index_costs::finish(
                        c,
                        s,
                        &document.path,
                        document.size,
                        document.prepared_seconds + per_file,
                    )?;
                }
                Ok(())
            })();
            offset += remaining;
            if let Err(error) = result {
                self.control.file_checkpoint()?;
                if crate::models::retryable(&error) {
                    return Err(error);
                }
                c.execute(
                    "UPDATE native_jobs SET error=?1 WHERE path=?2",
                    params![error.to_string(), document.path],
                )?;
                c.execute(
                    "INSERT OR REPLACE INTO native_content VALUES(?1,?2,?3)",
                    params![document.path, document.key, error.to_string()],
                )?;
                progress.skipped += 1;
                self.log("Warning", format!("Skipped {}: {error}", document.path));
            }
        }
        if let Ok((complete, pending)) = Database::session(c) {
            progress.session_completed = complete;
            progress.session_pending = pending;
        }
        c.execute_batch("BEGIN IMMEDIATE")?;
        Ok(())
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
        let ticket = self.models.query_generation.load(Ordering::Acquire);
        std::thread::spawn(move || {
            let start = Instant::now();
            if !scope.is_empty() {
                if let Ok(c) = owner.db.connect(true) {
                    let prefix = format!("{}\\", scope.trim_end_matches(['\\', '/']));
                    let upper = format!("{prefix}{}", char::MAX);
                    if let Ok(indexed)=c.query_row("SELECT EXISTS(SELECT 1 FROM names WHERE path=?1 OR (path>=?2 AND path<?3))",params![scope,prefix,upper],|r|r.get::<_,bool>(0)) { let _=owner.tx.send(Event::ScopeStatus(sequence,indexed)); }
                }
            }
            let result = (|| -> Result<Vec<Hit>> {
                let hits = owner.lexical(&q, &s, &search_mode, &category, &scope, &excluded)?;
                if search_mode != "Semantic" {
                    let _ = owner.tx.send(Event::SearchPartial(
                        sequence,
                        hits.clone(),
                        start.elapsed().as_secs_f64() * 1000.0,
                    ));
                }
                owner.complete_search(
                    &q,
                    &s,
                    &search_mode,
                    &category,
                    &scope,
                    &excluded,
                    hits,
                    Some(sequence),
                    Some(ticket),
                )
            })()
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
        let hits = self.lexical(q, s, mode, category, scope, excluded)?;
        self.complete_search(q, s, mode, category, scope, excluded, hits, None, None)
    }
    fn lexical(
        &self,
        q: &str,
        s: &Settings,
        mode: &str,
        category: &str,
        scope: &str,
        excluded: &str,
    ) -> Result<Vec<Hit>> {
        if scope.starts_with('[') {
            let scopes: Vec<String> = serde_json::from_str(scope)?;
            let mut hits = vec![];
            for root in scopes.into_iter().take(32) {
                hits.extend(self.lexical(q, s, mode, category, &root, excluded)?);
            }
            finish_hits(
                &mut hits,
                if mode != "Names" && category == "All files" {
                    media_intent(q)
                } else {
                    None
                },
            );
            return Ok(hits);
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
        if q.trim().is_empty() {
            return Ok(vec![]);
        }
        let filters = crate::search_filters::Filters::parse(q)?;
        let mut hits = if filters.active() {
            vec![]
        } else {
            crate::platform::actions(q, s)
        };
        if !scope.is_empty() {
            hits.retain(|h| crate::config::under(&h.path, scope));
        }
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
        if crate::db::path_query(&filters.text).is_some()
            || filters.pattern.is_some() && filters.text.is_empty()
        {
            return crate::db::names(&self.db, q, s, category, scope, excluded);
        }
        if mode != "Semantic"
            || category == "Folders"
            || filters.active() && filters.text.is_empty()
        {
            hits.extend(crate::db::names(&self.db, q, s, category, scope, excluded)?);
        }
        if mode != "Names"
            && category != "Folders"
            && crate::config::full_scope(s, scope)
            && !((category == "Images" || media_intent(&filters.text) == Some("Images"))
                && !text_query_intent(&filters.text))
        {
            hits.extend(self.keywords(
                &filters.text,
                s,
                category,
                scope,
                excluded,
                Some(&filters),
            )?);
        }
        finish_hits(
            &mut hits,
            if mode != "Names" && category == "All files" {
                media_intent(q)
            } else {
                None
            },
        );
        Ok(hits)
    }
    fn complete_search(
        &self,
        q: &str,
        s: &Settings,
        mode: &str,
        category: &str,
        scope: &str,
        excluded: &str,
        mut hits: Vec<Hit>,
        sequence: Option<u64>,
        ticket: Option<u64>,
    ) -> Result<Vec<Hit>> {
        let filters = crate::search_filters::Filters::parse(q)?;
        if !filters.uses_embeddings()
            || mode == "Names"
            || category == "Folders"
            || category == "Programs"
            || q.trim().is_empty()
            || q.to_lowercase().starts_with("app:")
            || q.to_lowercase().starts_with("settings:")
            || hits.first().is_some_and(|h| h.kind == "calculator")
            || filters.text.is_empty()
        {
            return Ok(hits);
        }
        let scopes: Vec<String> = if scope.starts_with('[') {
            serde_json::from_str(scope)?
        } else {
            vec![scope.into()]
        };
        for scope in scopes.into_iter().take(32) {
            if !crate::config::full_scope(s, &scope) || s.roots.is_empty() {
                continue;
            }
            if !self.db.connect(true)?.query_row(
                "SELECT EXISTS(SELECT 1 FROM segment_vectors LIMIT 1)",
                [],
                |r| r.get::<_, bool>(0),
            )? {
                continue;
            }
            match self.semantic(
                &filters.text,
                s,
                category,
                &scope,
                excluded,
                Some(&filters),
                ticket,
            ) {
                Ok(found) => hits.extend(found),
                Err(error) if mode != "Semantic" => {
                    let message = format!(
                        "Semantic search unavailable; showing names and exact words: {error}"
                    );
                    if let Some(id) = sequence {
                        let _ = self.tx.send(Event::SearchWarning(id, message));
                    }
                }
                Err(error) => return Err(error),
            }
        }
        finish_hits(
            &mut hits,
            if category == "All files" && !filters.active() {
                media_intent(q)
            } else {
                None
            },
        );
        Ok(hits)
    }
    fn keywords(
        &self,
        q: &str,
        s: &Settings,
        category: &str,
        scope: &str,
        excluded: &str,
        filters: Option<&crate::search_filters::Filters>,
    ) -> Result<Vec<Hit>> {
        let tokens = search_tokens(q);
        if tokens.is_empty() {
            return Ok(vec![]);
        }
        let terms = tokens
            .iter()
            .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let conn = self.db.connect(true)?;
        crate::db::register_search(&conn, s, category, scope, excluded, filters)?;
        let mut stmt=conn.prepare_cached("SELECT s.path,s.kind,s.text,s.location FROM words JOIN segments s ON s.rowid=words.rowid JOIN names n ON n.path=s.path WHERE words MATCH ?1 AND n.index_mode='full' AND vd_match(s.path,s.kind,n.mtime,n.size) ORDER BY rank LIMIT 100")?;
        let rows = stmt.query_map([format!("text: ({terms})")], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut hits = vec![];
        for row in rows {
            let (path, kind, text, loc) = row?;
            if crate::config::mode(&path, s) != "full" || s.excluded_sensitive && sensitive(&text) {
                continue;
            }
            let lower = text.to_lowercase();
            let exact = tokens.iter().filter(|t| lower.contains(t.as_str())).count() as f32
                / tokens.len().max(1) as f32;
            let v: serde_json::Value = serde_json::from_str(&loc)?;
            hits.push(Hit {
                path,
                kind,
                text: text.chars().take(8000).collect(),
                score: exact * 0.9,
                reason: "Matches words in the content".into(),
                page: v["page"].as_u64().unwrap_or(0) as u32,
                seconds: v["seconds"].as_f64().unwrap_or(-1.0) as f32,
                line: v["line"].as_u64().unwrap_or(0) as u32,
            });
        }
        Ok(hits)
    }
    fn semantic(
        &self,
        q: &str,
        s: &Settings,
        category: &str,
        scope: &str,
        excluded: &str,
        filters: Option<&crate::search_filters::Filters>,
        ticket: Option<u64>,
    ) -> Result<Vec<Hit>> {
        let vector = self.models.query_checked(s, q, &|| {
            anyhow::ensure!(
                ticket.is_none_or(|id| self.models.query_generation.load(Ordering::Acquire) == id),
                "Search superseded"
            );
            Ok(())
        })?;
        let conn = self.db.connect(true)?;
        let cat = if category == "All files" {
            media_intent(q).unwrap_or(category)
        } else {
            category
        };
        conn.execute_batch("BEGIN")?;
        crate::db::register_search(&conn, s, cat, scope, excluded, filters)?;
        let text_intent = text_query_intent(q);
        let policy = crate::config::SearchPolicy::new(s, cat, scope, excluded);
        let mut file_maxima: HashMap<(String, String), f32> = HashMap::new();
        let mut pool: Vec<(f32, i64)> = vec![];
        let mut channel_totals: HashMap<String, (f32, u64)> = HashMap::new();
        if let Some(matrix) =
            self.vectors
                .get(&conn, vector.len(), &s.performance, &s.cache_precision)?
        {
            let mut metadata_allowed = vec![true; matrix.groups.len()];
            if filters.is_some_and(|f| f.active()) {
                let mut stmt=conn.prepare_cached("SELECT path FROM names WHERE path IN (SELECT value FROM json_each(?1)) AND vd_match(path,kind,mtime,size)")?;
                for (chunk_index, groups) in matrix.groups.chunks(256).enumerate() {
                    let paths =
                        serde_json::to_string(&groups.iter().map(|g| &g.path).collect::<Vec<_>>())?;
                    let found = stmt
                        .query_map([paths], |r| r.get::<_, String>(0))?
                        .collect::<rusqlite::Result<HashSet<_>>>()?;
                    for (i, g) in groups.iter().enumerate() {
                        metadata_allowed[chunk_index * 256 + i] = found.contains(&g.path);
                    }
                }
            }
            let permitted = matrix
                .groups
                .iter()
                .enumerate()
                .map(|(i, g)| {
                    (s.code_search || g.namespace != "code")
                        && !(cat == "Images" && g.channel == "text" && !text_intent)
                        && policy.allowed(&g.path, &g.kind)
                        && metadata_allowed[i]
                })
                .collect::<Vec<_>>();
            for i in 0..matrix.ids.len() {
                let group = matrix.group_ids[i];
                if !permitted[group] {
                    continue;
                }
                let score = matrix.score(i, &vector);
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
            let sql = if filters.is_some_and(|f| f.active()) {
                "SELECT v.id,v.path,v.kind,v.vector,v.namespace,v.channel FROM segment_vectors v JOIN names n ON n.path=v.path WHERE v.dimension=?1 AND vd_match(v.path,v.kind,n.mtime,n.size)"
            } else {
                "SELECT v.id,v.path,v.kind,v.vector,v.namespace,v.channel FROM segment_vectors v WHERE v.dimension=?1 AND vd_match(v.path,v.kind,0,0)"
            };
            let mut stmt = conn.prepare_cached(sql)?;
            let mut rows = stmt.query([vector.len()])?;
            let mut saw_vector = false;
            while let Some(row) = rows.next()? {
                saw_vector = true;
                let id: i64 = row.get(0)?;
                let path: String = row.get(1)?;
                let kind: String = row.get(2)?;
                let namespace: String = row.get(4)?;
                let channel: String = row.get(5)?;
                if cat == "Images" && channel == "text" && !text_intent {
                    continue;
                }
                if !s.code_search && namespace == "code" {
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
            if !saw_vector {
                let mismatch: Option<usize> = conn.query_row(
                    "SELECT dimension FROM segment_vectors WHERE NOT EXISTS(SELECT 1 FROM segment_vectors WHERE dimension=?1) LIMIT 1",
                    [vector.len()], |r| r.get(0)).optional()?;
                anyhow::ensure!(
                    mismatch.is_none(),
                    "Embedding dimensions differ from the existing index. Select a compatible model or explicitly use Rebuild full content."
                );
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
            .filter(|t| {
                ![
                    "a", "the", "of", "image", "images", "photo", "photos", "picture", "pictures",
                    "in", "on",
                ]
                .contains(&t.as_str())
            })
            .collect::<Vec<_>>();
        let scores: HashMap<i64, f32> = pool.iter().map(|(score, id)| (*id, *score)).collect();
        let ids = serde_json::to_string(&pool.iter().map(|(_, id)| *id).collect::<Vec<_>>())?;
        let mut details=conn.prepare_cached("SELECT rowid,path,kind,text,location FROM segments WHERE rowid IN (SELECT value FROM json_each(?1))")?;
        let mut rows = details.query([ids])?;
        while let Some(row) = rows.next()? {
            let score = scores[&row.get::<_, i64>(0)?];
            let (path, kind, text, loc): (String, String, String, String) =
                (row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?);
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
            let ranking = score
                + if s.rank_calibration {
                    (score - mean).clamp(-0.15, 0.15)
                } else {
                    0.0
                }
                + if cat == "Images" && !text_intent {
                    0.0
                } else {
                    exact * 0.25
                };
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
        Ok(hits.into_values().collect())
    }
}
fn search_tokens(q: &str) -> Vec<String> {
    q.split_whitespace()
        .map(str::to_lowercase)
        .filter(|t| {
            ![
                "a", "the", "of", "image", "images", "photo", "photos", "picture", "pictures",
                "in", "on",
            ]
            .contains(&t.as_str())
        })
        .collect()
}
fn finish_hits(hits: &mut Vec<Hit>, intent: Option<&str>) {
    rank_hits(hits, intent);
    let mut paths = HashSet::new();
    hits.retain(|h| paths.insert(h.path.clone()));
    hits.truncate(100);
}
// Normalize old global fingerprints to relevant per-file settings. This upgrades
// stamps without regenerating embeddings when an unrelated option changes.
fn content_stamp(raw: &str, ext: &str) -> Option<String> {
    if crate::raw::is_raw(ext) && !raw.contains("|raw:libraw-0.22.2-preview-v1:") {
        return None;
    }
    if raw.starts_with("v3|") {
        return Some(raw.into());
    }
    let fields: Vec<_> = raw.split('|').collect();
    if fields.len() < 10 {
        return None;
    }
    let extras = &fields[10..];
    if ["html", "htm"].contains(&ext) && !extras.contains(&"html-visible-v1") {
        return None;
    }
    let mut indices = vec![0, 1, 2, 4];
    if content::is_image(ext) {
        indices.extend([3, 5, 8]);
    } else if content::is_video(ext) {
        indices.extend([3, 6, 7]);
    } else if content::is_audio(ext) {
        indices.extend([3, 6]);
    } else if ext == "pdf" {
        indices.push(8);
        if extras.contains(&"pdf:visual") {
            indices.push(3);
        }
    } else if content::is_code(ext) {
        indices.push(9);
    }
    let mut stamp = String::from("v3");
    for i in indices {
        stamp.push('|');
        stamp.push_str(fields[i]);
    }
    for value in extras {
        stamp.push('|');
        stamp.push_str(value);
    }
    Some(stamp)
}
/// Embedding deployment is user-controlled: changing URLs, local paths,
/// model aliases or projector paths never implicitly rebuilds completed content.
/// Compare metadata and only the extraction settings encoded by old stamps.
pub(crate) fn content_equivalent(old: &str, new: &str, ext: &str) -> bool {
    let (Some(old), Some(new)) = (content_stamp(old, ext), content_stamp(new, ext)) else {
        return false;
    };
    if old == new {
        return true;
    }
    let a: Vec<_> = old.split('|').collect();
    let b: Vec<_> = new.split('|').collect();
    if a.len() < 5 || b.len() < 5 || a[1..3] != b[1..3] {
        return false;
    }
    let extras = |fields: &[&str]| {
        fields
            .iter()
            .filter(|v| {
                v.starts_with("ocr:")
                    || v.starts_with("ocr-lang:")
                    || v.starts_with("visual:")
                    || v.starts_with("pdf:")
                    || v.starts_with("dedupe:")
                    || v.starts_with("video:")
                    || v.starts_with("raw:")
                    || **v == "html-visible-v1"
            })
            .map(|v| (*v).to_owned())
            .collect::<Vec<_>>()
    };
    if extras(&a) != extras(&b) {
        return false;
    }
    // Older remote stamps embedded URL|model as two fields in an eight-field
    // layout, shifting/truncating extractor settings. Those missing values
    // cannot be reconstructed; adopt the current extraction stamp once without
    // regenerating unchanged content. Subsequent stamps use the stable layout.
    if a[4].contains("://") || b[4].contains("://") {
        return true;
    }
    let indices: &[usize] = if content::is_image(ext) {
        &[6, 7]
    } else if content::is_video(ext) {
        &[6, 7]
    } else if content::is_audio(ext) {
        &[6]
    } else if ext == "pdf" || content::is_code(ext) {
        &[5]
    } else {
        &[]
    };
    indices.iter().all(|i| a.get(*i) == b.get(*i))
}

/// A child may fail with a network error even though its SMB share is healthy
/// (for example a NAS directory that cannot be enumerated). Only circuit-break
/// the whole source after confirming the root itself is no longer readable.
fn network_source_unavailable(root: &Path, error: Option<&std::io::Error>) -> bool {
    let Some(error) = error else {
        return false;
    };
    #[cfg(windows)]
    let network_error = error
        .raw_os_error()
        .is_some_and(|n| [53, 59, 64, 67, 121, 1231, 1232].contains(&n));
    #[cfg(not(windows))]
    let network_error = matches!(
        error.kind(),
        std::io::ErrorKind::NetworkDown
            | std::io::ErrorKind::NetworkUnreachable
            | std::io::ErrorKind::HostUnreachable
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::NotConnected
    );
    if !network_error {
        return false;
    }
    // Opening and reading the first entry verifies enumeration, rather than
    // trusting cached metadata/exists() after a share disconnects.
    std::fs::read_dir(root)
        .and_then(|mut entries| entries.next().transpose().map(|_| ()))
        .is_err()
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
    let lower = text.to_ascii_lowercase();
    text.contains("PRIVATE KEY-----")
        || text.split_whitespace().any(|t| {
            t.starts_with("ghp_")
                || t.starts_with("github_pat_")
                || t.starts_with("sk-")
                || t.starts_with("AKIA") && t.len() >= 20
        })
        || [
            "\"password\":",
            "\"api_key\":",
            "\"client_secret\":",
            "password=",
            "password:",
            "api_key=",
            "api_key:",
            "client_secret=",
            "authorization: bearer ",
        ]
        .iter()
        .any(|t| lower.contains(t))
}

#[cfg(test)]
mod notification_tests {
    use super::*;
    #[test]
    fn repaint_notification_happens_after_the_action_is_enqueued() {
        for reliable in [false, true] {
            let (tx, rx) = crossbeam_channel::bounded(2);
            let ctx = eframe::egui::Context::default();
            let seen = Arc::new(AtomicBool::new(false));
            let callback_seen = seen.clone();
            let pending = rx.clone();
            ctx.set_request_repaint_callback(move |_| {
                assert!(!pending.is_empty(), "UI woke before its event arrived");
                callback_seen.store(true, Ordering::Relaxed);
            });
            let notify = NotifySender::new(tx, Some(ctx));
            if reliable {
                notify.send(Event::Quit).unwrap();
            } else {
                notify.try_send(Event::Quit).unwrap();
            }
            assert!(seen.load(Ordering::Relaxed));
            assert!(matches!(rx.recv().unwrap(), Event::Quit));
        }
    }
    #[test]
    fn quit_is_retained_when_the_notification_queue_is_full() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let notify = NotifySender::from(tx);
        notify.send(Event::Progress(Progress::default())).unwrap();
        let worker = std::thread::spawn(move || notify.send(Event::Quit).unwrap());
        assert!(matches!(rx.recv().unwrap(), Event::Progress(_)));
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            Event::Quit
        ));
        worker.join().unwrap();
    }
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

#[cfg(test)]
mod fingerprint_tests {
    #[test]
    fn raw_upgrade_does_not_reembed_unrelated_files() {
        let old = "v3|1|42|model|||true|old-ocr";
        assert!(super::content_stamp(old, "arw").is_none());
        assert_eq!(super::content_stamp(old, "png"), Some(old.into()));
        let s = crate::config::Settings {
            index_raw: true,
            ..Default::default()
        };
        let new = format!(
            "1|42|model|projector||true|true|true|old-ocr|false{}",
            crate::raw::stamp(&s)
        );
        let key = super::content_stamp(&new, "arw").unwrap();
        assert!(key.contains("raw:libraw-0.22.2-preview-v1:true"));
        assert_eq!(super::content_stamp(&key, "arw"), Some(key));
    }
    #[test]
    fn unrelated_options_do_not_reembed_documents() {
        let a = "1|42|model|projector||true|true|true|old-ocr|false";
        let b = "1|42|model|new-projector||false|false|false|new-ocr|true";
        assert_eq!(
            super::content_stamp(a, "csv"),
            super::content_stamp(b, "csv")
        );
        assert_ne!(
            super::content_stamp(a, "png"),
            super::content_stamp(b, "png")
        );
        assert!(super::content_stamp(a, "html").is_none());
        assert!(super::content_stamp(&format!("{a}|html-visible-v1"), "html").is_some());
        assert_ne!(
            super::content_stamp(a, "csv"),
            super::content_stamp(&a.replacen("1|42", "2|42", 1), "csv")
        );
    }
}

#[cfg(test)]
mod interruption_tests {
    #[test]
    fn automatic_limits_cancel_without_marking_the_file_failed() {
        use std::sync::atomic::Ordering;
        let control = super::Control::default();
        control.automatic.store(true, Ordering::Relaxed);
        control
            .automatic_started
            .store(chrono::Local::now().timestamp() - 600, Ordering::Relaxed);
        {
            let mut s = control.schedule.lock().unwrap();
            s.schedule = "When idle".into();
            s.max_run_minutes = 5;
        }
        assert!(control.check().is_err());
        assert!(control.cancel.load(Ordering::Relaxed));
    }
}
#[cfg(test)]
mod incremental_tests {
    use super::*;
    #[test]
    fn targeted_update_retains_other_files_and_deletes_only_removed_subtree() {
        let home = tempfile::tempdir().unwrap();
        let source = tempfile::Builder::new()
            .prefix("VectorDash-delta-")
            .tempdir()
            .unwrap();
        let folder = source.path().join("sub");
        std::fs::create_dir(&folder).unwrap();
        let a = folder.join("a.txt");
        let b = source.path().join("b.txt");
        std::fs::write(&a, "one").unwrap();
        std::fs::write(&b, "two").unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
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
        engine.index_inner(&s, false).unwrap();
        assert_eq!(db.stats().unwrap().files, 2);
        std::fs::write(&a, "changed").unwrap();
        let result = engine
            .index_paths(&s, false, Some(vec![a.clone()]))
            .unwrap();
        assert!(result.starts_with("Targeted changes · 1 files"));
        assert_eq!(db.stats().unwrap().files, 2);
        std::fs::remove_file(&a).unwrap();
        std::fs::remove_dir(&folder).unwrap();
        engine.index_paths(&s, false, Some(vec![folder])).unwrap();
        assert_eq!(db.stats().unwrap().files, 1);
        assert!(!engine.models.loaded.load(Ordering::Relaxed));
        assert!(b.exists());
    }
    #[test]
    fn monitoring_waits_for_idle_even_with_manual_schedule() {
        let control = Control::default();
        let mut s = Settings::default();
        s.watch_changes = true;
        s.schedule = "Manual".into();
        s.check_cpu = false;
        s.check_gpu = false;
        s.ac_only = false;
        s.check_user_idle = true;
        s.idle_seconds = u32::MAX;
        *control.schedule.lock().unwrap() = s;
        control.automatic.store(true, Ordering::Relaxed);
        control.watching.store(true, Ordering::Relaxed);
        control
            .automatic_started
            .store(chrono::Local::now().timestamp(), Ordering::Relaxed);
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        let c = control.clone();
        let thread = std::thread::spawn(move || {
            c.file_checkpoint().unwrap();
            flag.store(true, Ordering::Relaxed);
        });
        std::thread::sleep(Duration::from_millis(200));
        assert!(!done.load(Ordering::Relaxed));
        control.schedule.lock().unwrap().check_user_idle = false;
        thread.join().unwrap();
        assert!(done.load(Ordering::Relaxed));
    }
}

fn text_query_intent(query: &str) -> bool {
    query.chars().any(|c| c.is_ascii_digit())
        || [
            "error",
            "text",
            "screenshot",
            "invoice",
            "number",
            "declined",
        ]
        .iter()
        .any(|word| query.to_lowercase().contains(word))
}

fn media_intent(query: &str) -> Option<&'static str> {
    let query = query.trim().to_lowercase();
    let query = query
        .strip_prefix("show me ")
        .or_else(|| query.strip_prefix("find "))
        .unwrap_or(&query);
    let query = query
        .strip_prefix("a ")
        .or_else(|| query.strip_prefix("an "))
        .unwrap_or(query);
    for (kind, words) in [
        (
            "Images",
            &["picture", "pictures", "photo", "photos", "image", "images"][..],
        ),
        (
            "Videos",
            &["video", "videos", "clip", "clips", "movie", "movies"][..],
        ),
    ] {
        for word in words {
            if let Some(rest) = query.strip_prefix(word) {
                if [
                    " of ",
                    " with ",
                    " showing ",
                    " containing ",
                    " from ",
                    " about ",
                ]
                .iter()
                .any(|p| rest.starts_with(p))
                {
                    return Some(kind);
                }
            }
        }
    }
    if query.contains(" in a video") || query.contains(" in the video") {
        return Some("Videos");
    }
    None
}
fn rank_hits(hits: &mut [Hit], intent: Option<&str>) {
    let matches = |h: &Hit| match intent {
        Some("Images") => content::is_image(&content::extension(Path::new(&h.path))),
        Some("Videos") => content::is_video(&content::extension(Path::new(&h.path))),
        _ => false,
    };
    hits.sort_by(|a, b| {
        matches(b)
            .cmp(&matches(a))
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| a.path.cmp(&b.path))
    });
}
#[cfg(test)]
mod media_ranking_tests {
    use super::*;
    fn hit(path: &str, score: f32) -> Hit {
        Hit {
            path: path.into(),
            kind: "file".into(),
            text: String::new(),
            score,
            reason: String::new(),
            page: 0,
            seconds: -1.0,
            line: 0,
        }
    }
    #[test]
    fn recognizes_explicit_media_requests() {
        for q in ["picture of dogs", "photos of dogs", "an image of dogs"] {
            assert_eq!(media_intent(q), Some("Images"));
        }
        for q in ["video of dogs", "clips showing dogs", "zebra in a video"] {
            assert_eq!(media_intent(q), Some("Videos"));
        }
        assert!(media_intent("video driver error").is_none());
        assert!(media_intent("invoice number").is_none());
    }
    #[test]
    fn requested_type_precedes_higher_scores_in_other_types() {
        let mut hits = vec![
            hit("vet.pdf", 0.95),
            hit("dog.jpg", 0.8),
            hit("a.mp4", 0.7),
            hit("b.mp4", 0.6),
        ];
        rank_hits(&mut hits, Some("Images"));
        assert_eq!(hits[0].path, "dog.jpg");
        rank_hits(&mut hits, Some("Videos"));
        assert_eq!(hits[0].path, "a.mp4");
        assert_eq!(hits[1].path, "b.mp4");
        rank_hits(&mut hits, None);
        assert_eq!(hits[0].path, "vet.pdf");
    }
}
#[cfg(test)]
mod automatic_watch_tests {
    use super::*;
    #[test]
    fn manual_scan_covers_startup_recovery_and_next_notification_visits_one_file() {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::Builder::new()
            .prefix("VD-manual-ack-")
            .tempdir()
            .unwrap();
        let existing = root.path().join("existing.txt");
        std::fs::write(&existing, "existing").unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.watch_changes = true;
        s.schedule = "Manual".into();
        s.check_cpu = false;
        s.check_gpu = false;
        s.check_user_idle = false;
        s.ac_only = false;
        s.pause_for_game = false;
        s.pause_for_fullscreen = false;
        s.roots = vec![crate::config::Source {
            path: root.path().to_string_lossy().into(),
            mode: "quick".into(),
        }];
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine = Engine {
            db: Database::new(home.path(), &s).unwrap(),
            models: Models::default(),
            control: Control::default(),
            tx: tx.into(),
            searching: Arc::new(AtomicBool::new(false)),
            vectors: crate::vector_cache::Cache::default(),
        };
        *engine.control.schedule.lock().unwrap() = s.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        struct Stop(Arc<AtomicBool>);
        impl Drop for Stop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let _stop = Stop(shutdown.clone());
        engine.start_scheduler(shutdown);
        let start = Instant::now();
        loop {
            assert!(start.elapsed() < Duration::from_secs(5));
            if let Ok(Event::WatchStatus(status)) = rx.recv_timeout(Duration::from_millis(100)) {
                if status.contains("recovery scan queued") {
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
        engine.index(s, false);
        loop {
            assert!(start.elapsed() < Duration::from_secs(7));
            if let Ok(Event::IndexDone(result)) = rx.recv_timeout(Duration::from_millis(100)) {
                result.unwrap();
                break;
            }
        }
        let c = engine.db.connect(true).unwrap();
        let epoch = c
            .query_row(
                "SELECT epoch FROM source_seen WHERE path=?1",
                [existing.to_string_lossy().as_ref()],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        loop {
            assert!(start.elapsed() < Duration::from_secs(9));
            if let Ok(Event::Log(_, line)) = rx.recv_timeout(Duration::from_millis(100)) {
                if line.starts_with("Completed full scan satisfied") {
                    break;
                }
            }
        }
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM native_changes WHERE path=''",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            0
        );
        let changed = root.path().join("only-new.txt");
        std::fs::write(&changed, "new").unwrap();
        let deadline = Instant::now() + Duration::from_secs(16);
        loop {
            assert!(Instant::now() < deadline);
            if let Ok(Event::IndexDone(result)) = rx.recv_timeout(Duration::from_millis(100)) {
                let summary = result.unwrap();
                assert!(
                    summary.starts_with("Targeted changes · 1 files"),
                    "{summary}"
                );
                break;
            }
        }
        assert_eq!(
            c.query_row(
                "SELECT epoch FROM source_seen WHERE path=?1",
                [existing.to_string_lossy().as_ref()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            epoch,
            "unrelated existing file must not be revisited"
        );
    }
    #[test]
    fn new_and_modified_files_are_automatically_indexed_in_quick_and_full_sources() {
        use std::sync::atomic::AtomicUsize;
        let home = tempfile::tempdir().unwrap();
        let source = tempfile::Builder::new()
            .prefix("VectorDash-auto-watch-")
            .tempdir()
            .unwrap();
        let full = source.path().join("full");
        std::fs::create_dir(&full).unwrap();
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let url = format!("http://{}", server.server_addr());
        let calls = Arc::new(AtomicUsize::new(0));
        let served = calls.clone();
        let http = server.clone();
        let stop_http = Arc::new(AtomicBool::new(false));
        let signal_http = stop_http.clone();
        let responder = std::thread::spawn(move || {
            while !signal_http.load(Ordering::Relaxed) {
                if let Ok(Some(request)) = http.recv_timeout(Duration::from_millis(100)) {
                    served.fetch_add(1, Ordering::Relaxed);
                    let _ = request.respond(tiny_http::Response::from_string(
                        "{\"data\":[{\"index\":0,\"embedding\":[1,0]}]}",
                    ));
                }
            }
        });
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.watch_changes = true;
        s.schedule = "Manual".into();
        s.remote_url = url;
        s.check_cpu = false;
        s.check_gpu = false;
        s.ac_only = false;
        s.check_user_idle = true;
        s.idle_seconds = u32::MAX;
        s.roots = vec![
            crate::config::Source {
                path: source.path().to_string_lossy().into(),
                mode: "quick".into(),
            },
            crate::config::Source {
                path: full.to_string_lossy().into(),
                mode: "full".into(),
            },
        ];
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
        let shutdown = Arc::new(AtomicBool::new(false));
        engine.start_scheduler(shutdown.clone());
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(source.path().join("quick.jfm"), "name only").unwrap();
        let note = full.join("note.txt");
        std::fs::write(&note, "A note about sleeping well.").unwrap();
        std::thread::sleep(Duration::from_millis(1200));
        assert_eq!(
            db.stats().unwrap().files,
            0,
            "Idle rule must block automatic indexing"
        );
        engine.control.schedule.lock().unwrap().check_user_idle = false;
        let deadline = Instant::now() + Duration::from_secs(25);
        while db.stats().unwrap().full != 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(db.stats().unwrap().files, 2);
        assert_eq!(db.stats().unwrap().full, 1);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        while engine.control.running.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
        std::fs::write(
            &note,
            "The changed note now discusses dogs sleeping outdoors.",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(25);
        while calls.load(Ordering::Relaxed) < 2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        while engine.control.running.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let c = db.connect(true).unwrap();
        let text: String = c
            .query_row(
                "SELECT text FROM segments WHERE path=?1",
                [note.to_string_lossy().as_ref()],
                |r| r.get(0),
            )
            .unwrap();
        assert!(text.contains("changed note"));
        shutdown.store(true, Ordering::Relaxed);
        stop_http.store(true, Ordering::Relaxed);
        responder.join().unwrap();
        std::thread::sleep(Duration::from_millis(1100));
    }
}

#[cfg(test)]
mod multi_scope_tests {
    use super::*;
    #[test]
    fn searches_union_selected_roots_without_unrelated_matches() {
        let home = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.roots = [r"C:\", r"D:\", r"E:\"]
            .iter()
            .map(|path| crate::config::Source {
                path: (*path).into(),
                mode: "quick".into(),
            })
            .collect();
        let db = Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        for root in ["C:\\a", "D:\\b", "E:\\other"] {
            Database::touch_name(
                &c,
                std::path::Path::new(&format!("{root}\\dog.txt")),
                "file",
                1,
                20,
                "quick",
                root,
                1,
            )
            .unwrap();
        }
        let (tx, _) = crossbeam_channel::unbounded();
        let engine = Engine {
            db,
            models: Models::default(),
            control: Control::default(),
            tx: NotifySender::new(tx, None),
            searching: Arc::new(AtomicBool::new(false)),
            vectors: crate::vector_cache::Cache::default(),
        };
        let scope = serde_json::to_string(&vec!["C:\\a", "D:\\b"]).unwrap();
        let hits = engine
            .search_inner("dog", &s, "Names", "All files", &scope, "")
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert!(!hits.iter().any(|h| h.path.starts_with("E:")));
    }
}

fn metadata_matches(
    conn: &rusqlite::Connection,
    filters: Option<&crate::search_filters::Filters>,
    path: &str,
) -> bool {
    filters.is_none_or(|f| {
        conn.query_row(
            "SELECT name,size,mtime FROM names WHERE path=?1",
            [path],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )
        .is_ok_and(|(name, size, mtime)| f.matches(&name, size, mtime))
    })
}

#[cfg(test)]
mod semantic_metadata_tests {
    use super::*;
    #[test]
    fn metadata_filters_apply_before_semantic_candidate_limits() {
        let home = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.remote_url = String::new();
        s.roots = vec![crate::config::Source {
            path: "C:\\photos".into(),
            mode: "full".into(),
        }];
        let db = Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        c.execute_batch("BEGIN").unwrap();
        for i in 0..501 {
            let target = i == 500;
            let path = format!("C:\\photos\\dog_{i}.jpg");
            Database::touch_name(
                &c,
                std::path::Path::new(&path),
                "file",
                1700000000000000000,
                if target { 4096 } else { 10 },
                "full",
                "C:\\photos",
                1,
            )
            .unwrap();
            c.execute(
                "INSERT INTO segments VALUES(?1,'files','image','','[]',?2)",
                params![path, "{\"embedding_source\":\"media\",\"seconds\":-1}"],
            )
            .unwrap();
            let id = c.last_insert_rowid();
            let x = if target { 0.3f32 } else { 0.99f32 };
            let vector = [x, (1. - x * x).sqrt()]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>();
            c.execute(
                "INSERT INTO segment_vectors VALUES(?1,'files','image','image',2,?2,?3)",
                params![id, vector, path],
            )
            .unwrap();
        }
        c.execute_batch("COMMIT").unwrap();
        drop(c);
        let http = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        s.remote_url = format!("http://{}", http.server_addr());
        let responder = http.clone();
        let signal = Arc::new(AtomicBool::new(false));
        let stop = signal.clone();
        let worker = std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Ok(Some(req)) = responder.recv_timeout(Duration::from_millis(100)) {
                    let _ = req.respond(tiny_http::Response::from_string(
                        "{\"data\":[{\"embedding\":[1,0],\"index\":0}]}",
                    ));
                }
            }
        });
        let (tx, _) = crossbeam_channel::unbounded();
        let engine = Engine {
            db,
            models: Models::default(),
            control: Control::default(),
            tx: NotifySender::new(tx, None),
            searching: Arc::new(AtomicBool::new(false)),
            vectors: crate::vector_cache::Cache::default(),
        };
        for profile in ["Balanced", "Low memory"] {
            s.performance = profile.into();
            let hits = engine
                .search_inner(
                    "dogs name:dog*.jpg size:>1KB",
                    &s,
                    "Semantic",
                    "All files",
                    "",
                    "",
                )
                .unwrap();
            assert_eq!(hits.len(), 1);
            assert!(hits[0].path.ends_with("dog_500.jpg"));
        }
        signal.store(true, Ordering::Relaxed);
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod calculator_filter_tests {
    use super::*;
    #[test]
    fn arithmetic_is_not_mistaken_for_filename_wildcards() {
        let home = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
        let db = Database::new(home.path(), &s).unwrap();
        let (tx, _) = crossbeam_channel::unbounded();
        let engine = Engine {
            db,
            models: Models::default(),
            control: Control::default(),
            tx: NotifySender::new(tx, None),
            searching: Arc::new(AtomicBool::new(false)),
            vectors: crate::vector_cache::Cache::default(),
        };
        for query in ["2*3", "2 * 3"] {
            let hits = engine
                .search_inner(query, &s, "Both", "All files", "", "")
                .unwrap();
            assert_eq!(hits[0].kind, "calculator");
            assert_eq!(hits[0].path, "6");
        }
        assert!(!engine.models.loaded.load(Ordering::Relaxed));
    }
}

#[cfg(test)]
mod scan_lifecycle_tests {
    use super::*;
    fn fixture(home: &Path, settings: &Settings) -> Engine {
        let db = Database::new(home, settings).unwrap();
        let (tx, _rx) = crossbeam_channel::bounded(512);
        let engine = Engine {
            db,
            models: Models::default(),
            control: Control::default(),
            tx: NotifySender::new(tx, None).with_log(home.join("test.log")),
            searching: Arc::new(AtomicBool::new(false)),
            vectors: crate::vector_cache::Cache::default(),
        };
        *engine.control.schedule.lock().unwrap() = settings.clone();
        engine
    }
    #[test]
    fn inaccessible_network_child_does_not_abort_readable_source() {
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("healthy-sibling.txt"), "sibling").unwrap();
        #[cfg(windows)]
        let errors = [53, 59, 64, 67, 121, 1231, 1232]
            .into_iter()
            .map(std::io::Error::from_raw_os_error)
            .collect::<Vec<_>>();
        #[cfg(not(windows))]
        let errors = vec![std::io::Error::from(std::io::ErrorKind::NetworkUnreachable)];
        for error in errors {
            assert!(!network_source_unavailable(source.path(), Some(&error)));
            assert!(network_source_unavailable(
                &source.path().join("offline-source"),
                Some(&error)
            ));
        }
        assert!(!network_source_unavailable(source.path(), None));
        assert!(!network_source_unavailable(
            source.path(),
            Some(&std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        ));
    }
    #[test]
    fn unavailable_source_retains_previously_indexed_names() {
        let home = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let root = source.path().join("share");
        std::fs::create_dir(&root).unwrap();
        let file = root.join("keep-indexed.txt");
        std::fs::write(&file, "keep").unwrap();
        let s = settings(&root, false);
        let engine = fixture(home.path(), &s);
        engine.index_inner(&s, false).unwrap();
        let before = engine.db.stats().unwrap().files;
        assert!(before > 0);
        std::fs::rename(&root, source.path().join("offline")).unwrap();
        engine.index_inner(&s, false).unwrap();
        assert_eq!(engine.db.stats().unwrap().files, before);
    }
    fn settings(path: &Path, watch: bool) -> Settings {
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.watch_changes = watch;
        s.schedule = "Manual".into();
        s.check_cpu = false;
        s.check_gpu = false;
        s.check_user_idle = false;
        s.ac_only = false;
        s.roots = vec![crate::config::Source {
            path: path.to_string_lossy().into_owned(),
            mode: "quick".into(),
        }];
        s
    }
    #[test]
    fn targeted_run_does_not_resume_unrelated_full_or_quick_jobs() {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let full = tempfile::tempdir().unwrap();
        let changed = root.path().join("changed.txt");
        std::fs::write(&changed, "changed").unwrap();
        let deferred = root.path().join("deferred.txt");
        std::fs::write(&deferred, "deferred").unwrap();
        let old_job = full.path().join("old-job.txt");
        std::fs::write(&old_job, "content").unwrap();
        let mut s = settings(root.path(), false);
        s.roots.push(crate::config::Source {
            path: full.path().to_string_lossy().into(),
            mode: "full".into(),
        });
        let engine = fixture(home.path(), &s);
        let c = engine.db.connect(false).unwrap();
        let (token, _) =
            Database::prepare_job(&c, &old_job.to_string_lossy(), "unfinished").unwrap();
        c.execute(
            "INSERT INTO native_quick_queue VALUES(?1)",
            [deferred.to_string_lossy().as_ref()],
        )
        .unwrap();
        engine
            .run_index(&s, false, Some(vec![changed.clone()]))
            .unwrap();
        assert_eq!(
            c.query_row(
                "SELECT error FROM native_jobs WHERE token=?1",
                [token],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            ""
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM native_quick_queue WHERE path=?1",
                [deferred.to_string_lossy().as_ref()],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM names WHERE path=?1",
                [changed.to_string_lossy().as_ref()],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM names WHERE path=?1",
                [old_job.to_string_lossy().as_ref()],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn changed_file_and_folder_never_walk_unrelated_sources() {
        let home = tempfile::tempdir().unwrap();
        let source = tempfile::Builder::new()
            .prefix("VD-targeted-")
            .tempdir()
            .unwrap();
        let branch = source.path().join("changed-folder");
        let unrelated = source.path().join("unrelated-folder");
        std::fs::create_dir(&branch).unwrap();
        std::fs::create_dir(&unrelated).unwrap();
        let changed = branch.join("one.txt");
        let sibling = unrelated.join("untouched.txt");
        std::fs::write(&changed, "one").unwrap();
        std::fs::write(&sibling, "untouched").unwrap();
        let s = settings(source.path(), false);
        let engine = fixture(home.path(), &s);
        engine.index_inner(&s, false).unwrap();
        let c = engine.db.connect(false).unwrap();
        c.execute("UPDATE source_seen SET epoch=123", []).unwrap();
        drop(c);
        std::fs::write(&changed, "changed").unwrap();
        let report = engine
            .index_paths(&s, false, Some(vec![changed.clone()]))
            .unwrap();
        assert!(report.contains("Targeted changes"));
        assert!(report.contains("1 files"), "{report}");
        let c = engine.db.connect(true).unwrap();
        let epoch: i64 = c
            .query_row(
                "SELECT epoch FROM source_seen WHERE path=?1",
                [sibling.to_string_lossy().as_ref()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(epoch, 123, "Unrelated file must not even be visited");
        drop(c);
        std::fs::write(branch.join("two.txt"), "two").unwrap();
        let report = engine.index_paths(&s, false, Some(vec![branch])).unwrap();
        assert!(report.contains("2 files"), "{report}");
        let c = engine.db.connect(true).unwrap();
        let epoch: i64 = c
            .query_row(
                "SELECT epoch FROM source_seen WHERE path=?1",
                [sibling.to_string_lossy().as_ref()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(epoch, 123);
    }
    #[test]
    fn deleted_record_cleanup_uses_bounded_epoch_ranges() {
        let home = tempfile::tempdir().unwrap();
        let s = settings(home.path(), false);
        let engine = fixture(home.path(), &s);
        let c = engine.db.connect(true).unwrap();
        let mut stmt = c.prepare("EXPLAIN QUERY PLAN SELECT path FROM source_seen WHERE source=?1 AND epoch<?2 UNION ALL SELECT path FROM source_seen WHERE source=?1 AND epoch>?2 LIMIT 1000").unwrap();
        let plan = stmt
            .query_map(params!["fixture", 123], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            plan.iter()
                .filter(|line| line.contains("seen_source") && line.contains("epoch"))
                .count(),
            2,
            "{plan:?}"
        );
        assert!(
            !plan.iter().any(|line| line.starts_with("SCAN source_seen")),
            "{plan:?}"
        );
    }
    #[test]
    fn cancelled_scan_does_not_launch_pending_recovery() {
        let home = tempfile::tempdir().unwrap();
        let source = tempfile::Builder::new()
            .prefix("VD-source-")
            .tempdir()
            .unwrap();
        std::fs::write(source.path().join("note.txt"), "name").unwrap();
        let s = settings(source.path(), true);
        let engine = fixture(home.path(), &s);
        engine.control.cancel_by_user();
        let stop = Arc::new(AtomicBool::new(false));
        engine.start_scheduler(stop.clone());
        std::thread::sleep(Duration::from_secs(12));
        assert!(engine.control.automatic_hold.load(Ordering::Relaxed));
        assert!(!engine.control.running.load(Ordering::Relaxed));
        assert_eq!(engine.db.stats().unwrap().files, 0);
        engine
            .control
            .cancelled_by_user
            .store(false, Ordering::SeqCst);
        engine.control.automatic_hold.store(false, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(5);
        while engine.db.stats().unwrap().files == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            engine.db.stats().unwrap().files,
            1,
            "Explicit resume should allow the queued recovery"
        );
        stop.store(true, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(1100));
    }
    #[test]
    fn source_restart_uses_updated_roots_and_removes_old_drive() {
        let home = tempfile::tempdir().unwrap();
        let old = tempfile::Builder::new()
            .prefix("VD-old-")
            .tempdir()
            .unwrap();
        let new = tempfile::Builder::new()
            .prefix("VD-new-")
            .tempdir()
            .unwrap();
        std::fs::write(old.path().join("removed-drive.txt"), "old").unwrap();
        std::fs::write(new.path().join("kept-drive.txt"), "new").unwrap();
        let s = settings(old.path(), false);
        let engine = fixture(home.path(), &s);
        engine.control.running.store(true, Ordering::Relaxed);
        engine.control.interrupt_sources();
        assert!(engine.control.cancel.load(Ordering::Relaxed));
        engine.control.update_settings(settings(new.path(), false));
        assert!(engine.control.restart_requested.load(Ordering::Relaxed));
        engine.control.running.store(false, Ordering::Relaxed);
        let stop = Arc::new(AtomicBool::new(false));
        engine.start_scheduler(stop.clone());
        let deadline = Instant::now() + Duration::from_secs(5);
        while engine.db.stats().unwrap().files == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        stop.store(true, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(1100));
        let c = engine.db.connect(true).unwrap();
        let paths = c
            .prepare("SELECT path FROM names")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(
            paths.contains(
                &new.path()
                    .join("kept-drive.txt")
                    .to_string_lossy()
                    .into_owned()
            ),
            "{}",
            std::fs::read_to_string(home.path().join("test.log")).unwrap_or_default()
        );
        assert!(paths.iter().all(|p| Path::new(p).starts_with(new.path())));
    }
    #[test]
    fn removing_sources_after_explicit_cancel_does_not_request_restart() {
        let c = Control::default();
        c.running.store(true, Ordering::Relaxed);
        *c.schedule.lock().unwrap() = settings(Path::new("Y:\\"), false);
        c.cancel_by_user();
        c.update_settings(settings(Path::new("C:\\"), false));
        assert!(!c.restart_requested.load(Ordering::Relaxed));
        assert!(c.automatic_hold.load(Ordering::Relaxed));
    }
}

#[derive(Debug)]
struct YieldFile;
impl std::fmt::Display for YieldFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Continuing saved long-file work after other files")
    }
}
impl std::error::Error for YieldFile {}

#[cfg(test)]
mod deployment_stamp_tests {
    use super::*;
    #[test]
    fn local_remote_aliases_and_paths_preserve_embeddings() {
        for ext in ["txt", "csv", "png", "mp4", "mp3", "pdf", "rs"] {
            let a = "1|42|old-model|old-encoder||true|true|true|ocr|false";
            let b = "1|42|new-model-path|new-encoder-path||true|true|true|ocr|false";
            let a = content_stamp(a, ext).unwrap();
            let b = content_stamp(b, ext).unwrap();
            assert!(content_equivalent(&a, &b, ext), "{ext}");
            assert!(!content_equivalent(
                &a,
                &b.replacen("v3|1|42", "v3|2|42", 1),
                ext
            ));
        }
        let remote = "v3|1|42|old-model|http://old-server/v1|true|false|ocr:png-v1";
        let local = "v3|1|42|new-local-model||ocr|ocr:png-v1";
        assert!(content_equivalent(remote, local, "pdf"));
        assert!(!content_equivalent(
            remote,
            &local.replace("ocr:png-v1", "ocr:off"),
            "pdf"
        ));
        assert!(!content_equivalent(
            remote,
            &format!("{local}|pdf:visual"),
            "pdf"
        ));
        let image = "v3|1|42|model||projector|true|ocr|ocr:png-v1";
        assert!(!content_equivalent(
            image,
            &image.replace("|true|ocr", "|false|ocr"),
            "png"
        ));
    }
}
