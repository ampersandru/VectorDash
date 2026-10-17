use crate::{
    config::{Settings, under},
    engine::{Event, NotifySender},
};
use notify::{RecursiveMode, Watcher};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
/// Bounded overflow buffer retains exact paths if the event channel is busy.
/// Only a genuinely saturated spill falls back to the affected source, never
/// all sources. The durable queue is drained in bounded batches separately.
#[derive(Default)]
struct NotificationSpill {
    paths: HashSet<PathBuf>,
    roots: HashSet<PathBuf>,
}
impl NotificationSpill {
    fn insert(&mut self, path: PathBuf, settings: &Settings) {
        if self.roots.iter().any(|r| path.starts_with(r)) {
            return;
        }
        if self.paths.len() < 32768 {
            self.paths.insert(path);
            return;
        }
        if let Some(root) = settings
            .roots
            .iter()
            .filter(|r| under(&path.to_string_lossy(), &r.path))
            .max_by_key(|r| r.path.len())
        {
            let root = PathBuf::from(&root.path);
            self.paths.retain(|p| !p.starts_with(&root));
            self.roots.insert(root);
        }
    }
}
#[derive(Default)]
pub struct Monitor {
    watcher: Option<notify::RecommendedWatcher>,
    rx: Option<std::sync::mpsc::Receiver<notify::Result<notify::Event>>>,
    signature: String,
    settings: Option<Settings>,
    logger: Option<NotifySender>,
    overflow: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    spill: Option<std::sync::Arc<std::sync::Mutex<NotificationSpill>>>,
    recovery_reason: String,
    pending: HashSet<PathBuf>,
    recovery: bool,
    recovery_saved: bool,
    startup_recovery: bool,
    retry_after: Option<Instant>,
    failures: u32,
    last: Option<Instant>,
    first: Option<Instant>,
    watched: usize,
    journal: crate::journal::Tracker,
    journal_inflight: bool,
    store: Option<crate::db::Database>,
    dirty: HashSet<PathBuf>,
    generation: i64,
    inflight: Option<Vec<(PathBuf, i64)>>,
    saved_hold: Option<bool>,
    loaded_store: bool,
    pub restored_hold: Option<bool>,
}
impl Monitor {
    pub fn configure(&mut self, s: &Settings, home: &Path, tx: &NotifySender) {
        let signature = format!(
            "{}|{}|{:?}|{:?}|{}|{:?}",
            s.watch_changes,
            s.ntfs_journal,
            s.roots,
            s.exclusions,
            s.excluded_types,
            (s.skip_windows_churn, s.include_hidden)
        );
        if signature == self.signature {
            return;
        }
        self.signature = signature;
        self.settings = Some(s.clone());
        self.logger = Some(tx.clone());
        self.recovery_reason.clear();
        self.watcher = None;
        self.rx = None;
        self.pending.clear();
        self.recovery = false;
        self.last = None;
        self.first = None;
        self.watched = 0;
        self.journal = crate::journal::Tracker::default();
        self.journal_inflight = false;
        if !s.watch_changes {
            return;
        }
        if let Ok(db) = crate::db::Database::new(home, s) {
            if let Ok(c) = db.connect(false) {
                let result=c.execute_batch("CREATE TABLE IF NOT EXISTS native_changes(path TEXT PRIMARY KEY,generation INTEGER NOT NULL,queued INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS native_changes_queued ON native_changes(queued,path);");
                if result.is_ok() {
                    let _ = crate::db::register_search(&c, s, "All files", "", "", None);
                    let _=c.execute("DELETE FROM native_changes WHERE path!='' AND NOT vd_match(path,'file',0,0)",[]);
                    self.generation = c
                        .query_row(
                            "SELECT coalesce(max(generation),0) FROM native_changes",
                            [],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    if !self.loaded_store {
                        use rusqlite::OptionalExtension;
                        let held = c
                            .query_row(
                                "SELECT value FROM metadata WHERE key='index_auto_hold'",
                                [],
                                |r| r.get::<_, String>(0),
                            )
                            .optional()
                            .ok()
                            .flatten()
                            .is_some_and(|v| v == "1");
                        self.restored_hold = Some(held);
                        self.saved_hold = Some(held);
                        self.loaded_store = true;
                    }
                    self.store = Some(db.clone());
                    self.restore(s);
                }
            }
        }
        let (sender, rx) = std::sync::mpsc::sync_channel(1024);
        let overflow = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = overflow.clone();
        let spill = std::sync::Arc::new(std::sync::Mutex::new(NotificationSpill::default()));
        let callback_spill = spill.clone();
        let filter = s.clone();
        let data_home = home.to_path_buf();
        let mut watcher =
            match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let event = event.map(|mut e| {
                    if !e.need_rescan() {
                        e.paths.retain(|p| {
                            !directory_metadata_only(&e.kind, p)
                                && !under(&p.to_string_lossy(), &data_home.to_string_lossy())
                                && filter
                                    .roots
                                    .iter()
                                    .filter(|r| under(&p.to_string_lossy(), &r.path))
                                    .max_by_key(|r| r.path.len())
                                    .is_some_and(|root| {
                                        p.ancestors()
                                            .take_while(|a| under(&a.to_string_lossy(), &root.path))
                                            .all(|a| filter.allowed(a))
                                    })
                        });
                    }
                    e
                });
                if event
                    .as_ref()
                    .is_ok_and(|e| e.paths.is_empty() && !e.need_rescan())
                {
                    return;
                }
                if let Err(std::sync::mpsc::TrySendError::Full(event)) = sender.try_send(event) {
                    match event {
                        Ok(e) if !e.need_rescan() => {
                            if let Ok(mut spill) = callback_spill.lock() {
                                for path in e.paths {
                                    spill.insert(path, &filter);
                                }
                            } else {
                                flag.store(true, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        _ => {
                            flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    let _ = tx.send(Event::Log(
                        "Warning".into(),
                        format!("Folder monitoring unavailable: {e}"),
                    ));
                    return;
                }
            };
        let mut roots = s
            .roots
            .iter()
            .map(|r| PathBuf::from(&r.path))
            .collect::<Vec<_>>();
        roots.sort_by_key(|p| p.components().count());
        let mut watched = Vec::<PathBuf>::new();
        for root in roots {
            let target = if root.is_file() {
                root.parent().unwrap_or(&root).to_path_buf()
            } else {
                root
            };
            if watched.iter().any(|p| target.starts_with(p)) || target.starts_with(home) {
                continue;
            }
            match watcher.watch(&target, RecursiveMode::Recursive) {
                Ok(()) => watched.push(target),
                Err(e) => {
                    let _ = tx.send(Event::Log(
                        "Warning".into(),
                        format!("Cannot monitor {}: {e}", target.display()),
                    ));
                }
            }
        }
        self.watched = watched.len();
        let journal_covered = self.journal.configure(s, home, tx);
        if self.watched > 0 && !journal_covered {
            // Replay durable interrupted work before a startup catch-up crawl.
            if !self.pending.is_empty() && !self.recovery {
                self.startup_recovery = true;
            } else {
                self.request_recovery("startup catch-up without continuous journal coverage");
                self.first = self.last;
            }
        }
        self.overflow = Some(overflow);
        self.spill = Some(spill);
        self.rx = Some(rx);
        self.watcher = Some(watcher);
        let _ = tx.send(Event::Log(
            "Info".into(),
            format!(
                "Monitoring {} recursive source(s); changes wait for idle",
                watched.len()
            ),
        ));
    }
    fn request_recovery(&mut self, reason: &str) {
        self.recovery = true;
        self.recovery_reason = reason.into();
        self.dirty.insert(PathBuf::new()); // refresh the durable recovery generation/time too
        self.last = Some(Instant::now());
    }
    fn queue_path(&mut self, path: PathBuf) {
        if self.dirty.len() >= 8192 && !self.dirty.contains(&path) {
            let saved = self.persist().is_ok() && self.dirty.is_empty();
            if !saved {
                self.pending.clear();
                self.dirty.clear();
                self.request_recovery("durable change queue unavailable; bounded memory fallback");
            }
        }
        self.last = Some(Instant::now());
        self.dirty.insert(path.clone());
        if self.pending.len() < 4096 || self.pending.contains(&path) {
            self.pending.insert(path);
        } else if self.store.is_none() {
            self.request_recovery("durable change queue unavailable");
        }
        // Additional known paths are persisted, then loaded after this batch.
    }
    pub fn collect(&mut self, s: &Settings, home: &Path) {
        let (paths, recovery) = self.journal.poll();
        if recovery {
            self.request_recovery("NTFS journal gap or inaccessible journal");
        }
        for path in paths {
            if !under(&path.to_string_lossy(), &home.to_string_lossy()) && s.searched(&path) {
                self.queue_path(path);
            }
        }
        if self
            .overflow
            .as_ref()
            .is_some_and(|f| f.swap(false, std::sync::atomic::Ordering::Relaxed))
        {
            self.request_recovery("notification information was lost");
        }
        let spilled = self.spill.as_ref().and_then(|buffer| {
            buffer.lock().ok().map(|mut buffer| {
                let mut paths = buffer.paths.drain().collect::<Vec<_>>();
                let roots=buffer.roots.drain().collect::<Vec<_>>();
                if !roots.is_empty() {
                    if let Some(tx)=&self.logger {let _=tx.send(Event::Log("Warning".into(),format!("Notification spill reached its memory limit; rechecking only affected sources: {}",roots.iter().map(|r|r.display().to_string()).collect::<Vec<_>>().join("; "))));}
                }
                paths.extend(roots);
                paths
            })
        });
        if let Some(paths) = spilled {
            for path in paths {
                self.queue_path(path);
            }
        }
        let events = self
            .rx
            .as_ref()
            .map(|rx| rx.try_iter().take(4096).collect::<Vec<_>>())
            .unwrap_or_default();
        for result in events {
            match result {
                Ok(event) => {
                    if event.need_rescan() {
                        self.request_recovery(
                            "Windows/provider requested a rescan after missed events",
                        );
                    }
                    if matches!(event.kind, notify::EventKind::Access(_)) {
                        continue;
                    }
                    for path in event.paths {
                        if directory_metadata_only(&event.kind, &path) {
                            continue;
                        }
                        if !under(&path.to_string_lossy(), &home.to_string_lossy())
                            && s.roots
                                .iter()
                                .filter(|r| under(&path.to_string_lossy(), &r.path))
                                .max_by_key(|r| r.path.len())
                                .is_some_and(|root| {
                                    path.ancestors()
                                        .take_while(|p| under(&p.to_string_lossy(), &root.path))
                                        .all(|p| s.allowed(p))
                                })
                        {
                            self.queue_path(path);
                        }
                    }
                }
                Err(error) => {
                    self.request_recovery(&format!("folder watcher reported an error: {error}"))
                }
            }
        }
        if (self.recovery || !self.pending.is_empty()) && self.first.is_none() {
            self.first = Some(Instant::now());
        }
        let _ = self.persist();
    }
    /// A completed whole-source scan covers only notifications queued before
    /// it started. Later generations (including repeated writes to one path)
    /// must survive and remain targeted.
    pub fn acknowledge_full_scan(&mut self, started: i64, s: &Settings) -> anyhow::Result<()> {
        self.persist()?;
        let Some(db) = &self.store else {
            return Ok(());
        };
        let c = db.connect(false)?;
        c.execute("DELETE FROM native_changes WHERE queued<?1", [started])?;
        drop(c);
        self.pending.clear();
        self.recovery = false;
        self.recovery_saved = false;
        self.startup_recovery = false;
        self.last = None;
        self.first = None;
        self.retry_after = None;
        self.failures = 0;
        self.restore(s);
        Ok(())
    }
    fn persist(&mut self) -> anyhow::Result<()> {
        if self.recovery && !self.recovery_saved {
            self.dirty.insert(PathBuf::new());
        }
        if self.dirty.is_empty() {
            return Ok(());
        }
        let Some(db) = &self.store else {
            return Ok(());
        };
        let mut c = db.connect(false)?;
        let tx = c.transaction()?;
        self.generation += 1;
        {
            let queued = chrono::Utc::now().timestamp_millis();
            let mut statement=tx.prepare_cached("INSERT INTO native_changes VALUES(?1,?2,?3) ON CONFLICT(path) DO UPDATE SET generation=excluded.generation,queued=excluded.queued")?;
            for path in &self.dirty {
                statement.execute(rusqlite::params![
                    path.to_string_lossy(),
                    self.generation,
                    queued
                ])?;
            }
        }
        tx.commit()?;
        if self.dirty.contains(&PathBuf::new()) {
            self.recovery_saved = true;
        }
        self.dirty.clear();
        Ok(())
    }
    fn restore(&mut self, s: &Settings) {
        let Some(db) = &self.store else {
            return;
        };
        let Ok(c) = db.connect(true) else {
            return;
        };
        let Ok(mut stmt) = c.prepare("SELECT path FROM native_changes ORDER BY queued LIMIT 4096")
        else {
            return;
        };
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for path in rows.flatten() {
                if path.is_empty() {
                    self.recovery = true;
                    self.recovery_saved = true;
                } else if s.searched(Path::new(&path)) && self.pending.len() < 4096 {
                    self.pending.insert(path.into());
                }
            }
        }
        if self.recovery || !self.pending.is_empty() {
            self.last = Some(Instant::now());
            self.first = Some(Instant::now() - Duration::from_secs(61));
        }
    }
    pub fn persist_hold(&mut self, held: bool) {
        if self.saved_hold == Some(held) {
            return;
        }
        let Some(db) = &self.store else {
            return;
        };
        if let Ok(c) = db.connect(false) {
            if c.execute(
                "INSERT OR REPLACE INTO metadata VALUES('index_auto_hold',?1)",
                [if held { "1" } else { "0" }],
            )
            .is_ok()
            {
                self.saved_hold = Some(held);
            }
        }
    }
    pub fn status(&self, s: &Settings, busy: bool) -> String {
        if !s.watch_changes {
            return "File monitoring is off".into();
        }
        if self.watched == 0 {
            return "No sources could be monitored; check warnings in Logs".into();
        }
        let pending = if self.recovery {
            format!(
                "catch-up/recovery scan queued ({})",
                if self.recovery_reason.is_empty() {
                    "startup/interrupted work"
                } else {
                    &self.recovery_reason
                }
            )
        } else {
            format!("{} changed paths queued", self.pending.len())
        };
        let waiting = if self
            .retry_after
            .is_some_and(|deadline| Instant::now() < deadline)
        {
            "waiting to retry a failed update"
        } else if busy {
            "indexing is running"
        } else if self.ready() {
            crate::scheduling::idle_allowed(s, crate::scheduling::metrics())
                .err()
                .unwrap_or("ready to update")
        } else if self.last.is_some() {
            "combining recent changes (10s debounce; 60s maximum)"
        } else {
            "waiting for file changes"
        };
        format!(
            "Watching {} sources · {} NTFS journals ({} retrying) · {pending} · {waiting}",
            self.watched,
            self.journal.active(),
            self.journal.retrying()
        )
    }
    pub fn ready(&self) -> bool {
        self.retry_after
            .is_none_or(|deadline| Instant::now() >= deadline)
            && (self.recovery || !self.pending.is_empty())
            && (self
                .last
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(10))
                || self
                    .first
                    .is_some_and(|t| t.elapsed() >= Duration::from_secs(60)))
    }
    pub fn complete(&mut self, success: bool, home: &Path) {
        if !self.journal_inflight {
            return;
        }
        if success {
            self.failures = 0;
            self.retry_after = None;
        } else {
            self.failures = self.failures.saturating_add(1);
            self.retry_after = Some(
                Instant::now()
                    + Duration::from_secs(
                        (10u64.saturating_mul(1u64 << self.failures.min(5))).min(300),
                    ),
            );
        }
        self.journal.finish(success, home);
        self.journal_inflight = false;
        if let Some(inflight) = self.inflight.take() {
            if success {
                if let Some(db) = &self.store {
                    if let Ok(mut c) = db.connect(false) {
                        if let Ok(tx) = c.transaction() {
                            let result = (|| -> anyhow::Result<()> {
                                let mut statement = tx.prepare_cached(
                                    "DELETE FROM native_changes WHERE path=?1 AND generation=?2",
                                )?;
                                for (path, generation) in &inflight {
                                    statement.execute(rusqlite::params![
                                        path.to_string_lossy(),
                                        generation
                                    ])?;
                                }
                                Ok(())
                            })();
                            if result.is_ok() {
                                let _ = tx.commit();
                            }
                        }
                    }
                }
            } else {
                for (path, _) in inflight {
                    if path.as_os_str().is_empty() {
                        self.recovery = true;
                    } else {
                        self.pending.insert(path);
                    }
                }
                self.last = Some(Instant::now());
                self.first = self.last;
            }
        }
        if success {
            if let Some(settings) = self.settings.clone() {
                self.restore(&settings);
            }
        }
        if success && self.startup_recovery {
            self.startup_recovery = false;
            self.recovery = true;
            self.last = Some(Instant::now());
            self.first = self.last;
        }
    }
    pub fn take(&mut self) -> Option<Vec<PathBuf>> {
        let _ = self.persist();
        self.journal.begin();
        self.journal_inflight = true;
        self.last = None;
        self.first = None;
        let paths = if self.recovery {
            self.recovery = false;
            self.recovery_saved = false;
            self.pending.clear();
            None
        } else {
            Some(self.pending.drain().collect::<Vec<_>>())
        };
        let mut inflight = vec![];
        if let Some(db) = &self.store {
            if let Ok(c) = db.connect(true) {
                if let Some(paths) = &paths {
                    if let Ok(mut statement) =
                        c.prepare_cached("SELECT generation FROM native_changes WHERE path=?1")
                    {
                        for path in paths {
                            if let Ok(generation) = statement
                                .query_row([path.to_string_lossy().as_ref()], |r| {
                                    r.get::<_, i64>(0)
                                })
                            {
                                inflight.push((path.clone(), generation));
                            }
                        }
                    }
                } else if let Ok(mut statement) = c.prepare(
                    "SELECT path,generation FROM native_changes ORDER BY queued LIMIT 4096",
                ) {
                    if let Ok(rows) = statement
                        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                    {
                        inflight.extend(rows.flatten().map(|(p, g)| (PathBuf::from(p), g)));
                    }
                }
            }
        }
        if inflight.is_empty() {
            if let Some(paths) = &paths {
                inflight.extend(paths.iter().cloned().map(|p| (p, 0)));
            } else {
                inflight.push((PathBuf::new(), 0));
            }
        }
        self.inflight = Some(inflight);
        paths
    }
}
#[cfg(test)]
mod notification_tests {
    use super::*;
    #[test]
    fn recursive_notifications_queue_changes_and_exclude_patterns() {
        let source = if let Ok(root) = std::env::var("VECTORDASH_TEST_WATCH_ROOT") {
            tempfile::Builder::new()
                .prefix("VectorDash-watch-")
                .tempdir_in(root)
                .unwrap()
        } else {
            tempfile::Builder::new()
                .prefix("VectorDash-watch-")
                .tempdir()
                .unwrap()
        };
        let home = tempfile::tempdir().unwrap();
        let child = source.path().join("nested");
        std::fs::create_dir(&child).unwrap();
        let mut s = Settings::default();
        s.watch_changes = true;
        s.roots = vec![crate::config::Source {
            path: source.path().to_string_lossy().into(),
            mode: "quick".into(),
        }];
        s.exclusions = vec!["*.log".into()];
        let (tx, _rx) = crossbeam_channel::bounded(64);
        let mut monitor = Monitor::default();
        monitor.configure(&s, home.path(), &tx.into());
        monitor.take();
        std::thread::sleep(Duration::from_millis(150));
        let note = child.join("new.txt");
        std::fs::write(&note, "one").unwrap();
        std::fs::write(child.join("ignored.log"), "ignored").unwrap();
        let started = Instant::now();
        while !monitor.pending.contains(&note) && started.elapsed() < Duration::from_secs(5) {
            monitor.collect(&s, home.path());
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(monitor.pending.contains(&note));
        assert!(
            !monitor
                .pending
                .iter()
                .any(|p| p.extension().is_some_and(|e| e == "log"))
        );
        assert!(!monitor.ready());
        monitor.last = Some(Instant::now() - Duration::from_secs(11));
        assert!(monitor.ready());
        assert!(monitor.take().unwrap().contains(&note));
        assert!(!monitor.ready());
    }
    #[test]
    fn overflow_becomes_recovery_scan() {
        let mut monitor = Monitor::default();
        monitor.recovery = true;
        monitor.last = Some(Instant::now() - Duration::from_secs(11));
        assert!(monitor.ready());
        assert!(monitor.take().is_none());
        assert!(!monitor.ready());
    }
}
#[cfg(test)]
mod debounce_tests {
    use super::*;
    #[test]
    fn continual_changes_cannot_starve_ready_queue() {
        let mut monitor = Monitor::default();
        monitor.pending.insert(PathBuf::from("note.txt"));
        monitor.last = Some(Instant::now());
        monitor.first = Some(Instant::now() - Duration::from_secs(61));
        assert!(monitor.ready());
    }
}

// Updating a child changes its parent's mtime too. Windows sends the child's own
// notification; rescanning that parent for metadata-only events defeats targeting.
fn directory_metadata_only(kind: &notify::EventKind, path: &Path) -> bool {
    matches!(
        kind,
        notify::EventKind::Access(_)
            | notify::EventKind::Modify(
                notify::event::ModifyKind::Any
                    | notify::event::ModifyKind::Metadata(_)
                    | notify::event::ModifyKind::Data(_)
            )
    ) && path.is_dir()
}
#[cfg(test)]
mod directory_event_tests {
    use super::*;
    #[test]
    fn child_changes_do_not_rescan_parent_but_new_and_renamed_folders_do() {
        let root = tempfile::Builder::new()
            .prefix("VectorDash-stability-")
            .tempdir()
            .unwrap();
        assert!(directory_metadata_only(
            &notify::EventKind::Modify(notify::event::ModifyKind::Any),
            root.path()
        ));
        assert!(!directory_metadata_only(
            &notify::EventKind::Create(notify::event::CreateKind::Folder),
            root.path()
        ));
        assert!(!directory_metadata_only(
            &notify::EventKind::Modify(notify::event::ModifyKind::Name(
                notify::event::RenameMode::Both
            )),
            root.path()
        ));
    }
}

#[cfg(test)]
mod durable_tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Settings, Monitor) {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::Builder::new()
            .prefix("VectorDash-stability-")
            .tempdir()
            .unwrap();
        let s = Settings {
            encrypt_index: false,
            watch_changes: true,
            roots: vec![crate::config::Source {
                path: root.path().to_string_lossy().into(),
                mode: "quick".into(),
            }],
            ..Default::default()
        };
        let (tx, _) = crossbeam_channel::unbounded();
        let mut monitor = Monitor::default();
        monitor.configure(&s, home.path(), &tx.into());
        monitor.take();
        monitor.complete(true, home.path());
        (home, root, s, monitor)
    }
    #[test]
    fn unavailable_durable_queue_keeps_memory_bounded_and_requests_recovery() {
        let mut monitor = Monitor::default();
        for i in 0..20000 {
            monitor.queue_path(PathBuf::from(format!("file-{i}.txt")));
        }
        assert!(monitor.recovery);
        assert!(monitor.pending.len() <= 4096);
        assert!(monitor.dirty.len() <= 8192);
    }
    #[test]
    fn successful_full_scan_clears_old_recovery_but_preserves_new_notifications() {
        let (_home, root, s, mut monitor) = fixture();
        let old = root.path().join("old.txt");
        let later = root.path().join("later.txt");
        monitor.queue_path(old.clone());
        monitor.request_recovery("startup catch-up");
        monitor.persist().unwrap();
        monitor.startup_recovery = true;
        let db = monitor.store.as_ref().unwrap().clone();
        let c = db.connect(false).unwrap();
        c.execute("UPDATE native_changes SET queued=100", [])
            .unwrap();
        monitor.queue_path(later.clone());
        monitor.persist().unwrap();
        c.execute(
            "UPDATE native_changes SET queued=300 WHERE path=?1",
            [later.to_string_lossy().as_ref()],
        )
        .unwrap();
        drop(c);
        monitor.acknowledge_full_scan(200, &s).unwrap();
        assert!(!monitor.recovery && !monitor.startup_recovery);
        assert_eq!(monitor.take().unwrap(), vec![later]);
    }
    #[test]
    fn new_overflow_during_full_scan_is_not_acknowledged_as_old_work() {
        let (_home, _root, s, mut monitor) = fixture();
        monitor.request_recovery("new notification gap");
        monitor.persist().unwrap();
        let c = monitor.store.as_ref().unwrap().connect(false).unwrap();
        c.execute("UPDATE native_changes SET queued=300 WHERE path=''", [])
            .unwrap();
        drop(c);
        monitor.acknowledge_full_scan(200, &s).unwrap();
        assert!(monitor.recovery);
    }
    #[test]
    fn large_known_burst_is_durable_targeted_batches_not_a_global_recovery() {
        let (home, root, _s, mut monitor) = fixture();
        for i in 0..10000 {
            monitor.queue_path(root.path().join(format!("changed-{i}.txt")));
        }
        monitor.persist().unwrap();
        assert!(!monitor.recovery);
        assert!(monitor.pending.len() <= 4096);
        let c = monitor.store.as_ref().unwrap().connect(true).unwrap();
        let mut plan = c
            .prepare(
                "EXPLAIN QUERY PLAN SELECT path FROM native_changes ORDER BY queued LIMIT 4096",
            )
            .unwrap();
        let details = plan
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(
            details.iter().any(|p| p.contains("native_changes_queued")),
            "{details:?}"
        );
        drop(plan);
        drop(c);
        let mut seen = HashSet::new();
        let mut batches = 0;
        while !monitor.pending.is_empty() {
            let paths = monitor.take().expect("known paths must stay targeted");
            assert!(paths.len() <= 4096);
            for path in paths {
                assert!(
                    seen.insert(path),
                    "path visited twice without a new generation"
                );
            }
            monitor.complete(true, home.path());
            batches += 1;
        }
        assert_eq!(seen.len(), 10000);
        assert_eq!(batches, 3);
        assert!(!monitor.recovery);
        let c = monitor.store.as_ref().unwrap().connect(true).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM native_changes", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn notification_spill_keeps_exact_paths_and_limits_extreme_fallback_to_affected_root() {
        let (_home, root, mut s, _monitor) = fixture();
        let other = tempfile::tempdir().unwrap();
        s.roots.push(crate::config::Source {
            path: other.path().to_string_lossy().into(),
            mode: "quick".into(),
        });
        let mut spill = NotificationSpill::default();
        for i in 0..10000 {
            spill.insert(root.path().join(format!("changed-{i}")), &s);
        }
        assert_eq!(spill.paths.len(), 10000);
        assert!(spill.roots.is_empty());
        for i in 10000..33000 {
            spill.insert(root.path().join(format!("changed-{i}")), &s);
        }
        assert!(spill.paths.len() <= 32768);
        assert_eq!(spill.roots, HashSet::from([root.path().to_path_buf()]));
    }
    #[test]
    fn pending_paths_survive_restart_and_failed_updates_stay_targeted() {
        let (home, root, s, mut monitor) = fixture();
        let path = root.path().join("changed.txt");
        monitor.pending.insert(path.clone());
        monitor.dirty.insert(path.clone());
        monitor.persist().unwrap();
        drop(monitor);
        let (tx, _) = crossbeam_channel::unbounded();
        let mut resumed = Monitor::default();
        resumed.configure(&s, home.path(), &tx.into());
        assert!(resumed.ready());
        assert_eq!(resumed.take().unwrap(), vec![path.clone()]);
        resumed.complete(false, home.path());
        assert!(!resumed.recovery);
        assert_eq!(resumed.take().unwrap(), vec![path]);
        resumed.complete(true, home.path());
        let c = resumed.store.as_ref().unwrap().connect(true).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM native_changes", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn change_during_indexing_is_not_deleted_with_older_generation() {
        let (home, root, s, mut monitor) = fixture();
        let path = root.path().join("busy.txt");
        monitor.pending.insert(path.clone());
        monitor.dirty.insert(path.clone());
        monitor.persist().unwrap();
        monitor.take();
        monitor.pending.insert(path.clone());
        monitor.dirty.insert(path.clone());
        monitor.persist().unwrap();
        monitor.complete(true, home.path());
        let c = monitor.store.as_ref().unwrap().connect(true).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM native_changes", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            1
        );
        drop(c);
        drop(monitor);
        let (tx, _) = crossbeam_channel::unbounded();
        let mut resumed = Monitor::default();
        resumed.configure(&s, home.path(), &tx.into());
        assert_eq!(resumed.take().unwrap(), vec![path]);
    }
    #[test]
    fn explicit_cancel_hold_survives_restart_and_resume_clears_it() {
        let (home, _root, s, mut monitor) = fixture();
        monitor.persist_hold(true);
        drop(monitor);
        let (tx, _) = crossbeam_channel::unbounded();
        let mut resumed = Monitor::default();
        resumed.configure(&s, home.path(), &tx.into());
        assert_eq!(resumed.restored_hold, Some(true));
        resumed.persist_hold(false);
        drop(resumed);
        let (tx, _) = crossbeam_channel::unbounded();
        let mut again = Monitor::default();
        again.configure(&s, home.path(), &tx.into());
        assert_eq!(again.restored_hold, Some(false));
    }
    #[test]
    fn idle_recovery_marker_does_not_rewrite_database_every_poll() {
        let (_home, _root, _s, mut monitor) = fixture();
        monitor.recovery = true;
        monitor.persist().unwrap();
        let generation = monitor.generation;
        for _ in 0..5 {
            monitor.persist().unwrap();
        }
        assert_eq!(monitor.generation, generation);
    }
}
