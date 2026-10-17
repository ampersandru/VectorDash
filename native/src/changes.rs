use crate::{
    config::{Settings, under},
    engine::{Event, NotifySender},
};
use notify::{RecursiveMode, Watcher};
use std::{
    collections::{HashSet,BTreeMap},
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
    failed: HashSet<PathBuf>,
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
fn deliver_notification(
    event: notify::Result<notify::Event>, origin: &Path,
    filter: &Settings, data_home: &Path,
    sender: &std::sync::mpsc::SyncSender<notify::Result<notify::Event>>,
    spill: &std::sync::Mutex<NotificationSpill>,
) {
    let event = event.map(|mut e| {
        if e.need_rescan() {
            if e.paths.is_empty() { e.paths.push(origin.to_path_buf()); }
        } else {
            e.paths.retain(|p| {
                !directory_metadata_only(&e.kind, p)
                    && !under(&p.to_string_lossy(), &data_home.to_string_lossy())
                    && filter.roots.iter()
                        .filter(|r| under(&p.to_string_lossy(), &r.path))
                        .max_by_key(|r| r.path.len())
                        .is_some_and(|root| p.ancestors()
                            .take_while(|a| under(&a.to_string_lossy(), &root.path))
                            .all(|a| filter.allowed(a)))
            });
        }
        e
    }).map_err(|mut e| {
        if e.paths.is_empty() { e.paths.push(origin.to_path_buf()); }
        e
    });
    if event.as_ref().is_ok_and(|e| e.paths.is_empty() && !e.need_rescan()) { return; }
    if let Err(std::sync::mpsc::TrySendError::Full(event)) = sender.try_send(event) {
        let mut spill = spill.lock().unwrap_or_else(|e| e.into_inner());
        match event {
            Ok(e) if !e.need_rescan() => {
                for path in e.paths { spill.insert(path, filter); }
            }
            Ok(e) => { spill.roots.extend(e.paths); }
            Err(e) => { spill.failed.extend(e.paths); }
        }
    }

}
/// One callback per watched root retains the origin even when Windows supplies
/// an empty overflow/error event. A shared watcher cannot identify that source.
fn source_watcher(
    target: &Path, settings: &Settings, home: &Path,
    sender: std::sync::mpsc::SyncSender<notify::Result<notify::Event>>,
    spill: std::sync::Arc<std::sync::Mutex<NotificationSpill>>,
) -> notify::Result<notify::RecommendedWatcher> {
    let origin = target.to_path_buf();
    let filter = settings.clone();
    let data_home = home.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |event| {
        deliver_notification(event, &origin, &filter, &data_home, &sender, &spill);
    })?;
    watcher.watch(target, RecursiveMode::Recursive)?;
    Ok(watcher)
}
#[derive(Default)]
pub struct Monitor {
    watchers: BTreeMap<PathBuf, notify::RecommendedWatcher>,
    sender: Option<std::sync::mpsc::SyncSender<notify::Result<notify::Event>>>,
    rx: Option<std::sync::mpsc::Receiver<notify::Result<notify::Event>>>,
    signature: String,
    settings: Option<Settings>,
    logger: Option<NotifySender>,
    spill: Option<std::sync::Arc<std::sync::Mutex<NotificationSpill>>>,
    recovery_reason: String,
    journal_notification_log: BTreeMap<PathBuf, Instant>,
    pending: HashSet<PathBuf>,
    recovery: bool,
    recovery_saved: bool,
    startup_recovery: bool,
    retry_after: Option<Instant>,
    failures: u32,
    last: Option<Instant>,
    first: Option<Instant>,
    watched: usize,
    watched_roots: HashSet<PathBuf>,
    watcher_retries:BTreeMap<PathBuf,(u32,Instant)>,
    watcher_failures:BTreeMap<PathBuf,u32>,
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
        self.journal_notification_log.clear();
        self.watchers.clear();
        self.sender = None;
        self.rx = None;
        self.pending.clear();
        self.recovery = false;
        self.last = None;
        self.first = None;
        self.watched = 0;
        self.watched_roots.clear();
        self.watcher_retries.clear();
        self.watcher_failures.clear();
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
        let spill = std::sync::Arc::new(std::sync::Mutex::new(NotificationSpill::default()));
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
            match source_watcher(&target, s, home, sender.clone(), spill.clone()) {
                Ok(watcher) => { self.watchers.insert(target.clone(), watcher); watched.push(target); },
                Err(e) => {
                    self.watcher_retries.insert(target.clone(),(1,Instant::now()+Duration::from_secs(5)));
                    self.watcher_failures.insert(target.clone(),1);
                    let _ = tx.send(Event::Log(
                        "Warning".into(),
                        format!("Cannot monitor {}; retry in 5s: {e}", target.display()),
                    ));
                }
            }
        }
        self.watched = watched.len();
        self.watched_roots=watched.iter().cloned().collect();
        let uncovered = self.journal.configure(s, home, tx);
        if self.watched > 0 && !uncovered.is_empty() {
            self.request_scoped_recovery(uncovered, "startup catch-up without continuous coverage");
        }
        self.spill = Some(spill);
        self.rx = Some(rx);
        self.sender = Some(sender);
        let _ = tx.send(Event::Log(
            "Info".into(),
            format!(
                "Monitoring {} recursive source(s); changes wait for idle",
                watched.len()
            ),
        ));
    }
    fn schedule_watch_retry(&mut self, paths: &[PathBuf]) {
        for path in paths {
            let target = self
                .watched_roots
                .iter()
                .filter(|r| under(&path.to_string_lossy(), &r.to_string_lossy()))
                .min_by_key(|r| r.components().count())
                .cloned()
                .unwrap_or_else(|| path.clone());
            self.watched_roots.remove(&target);
            if !self.watcher_retries.contains_key(&target) {
                let attempts = self
                    .watcher_failures
                    .get(&target)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(1);
                let delay = (5u64 << attempts.saturating_sub(1).min(6)).min(300);
                self.watcher_failures.insert(target.clone(), attempts);
                self.watcher_retries.insert(
                    target,
                    (attempts, Instant::now() + Duration::from_secs(delay)),
                );
            }
        }
        self.watched = self.watched_roots.len();
    }
    fn retry_watches(&mut self) {
        let now = Instant::now();
        let due = self
            .watcher_retries
            .iter()
            .filter(|(_, (_, at))| now >= *at)
            .map(|(p, (attempts, _))| (p.clone(), *attempts))
            .collect::<Vec<_>>();
        for (path, attempts) in due {
            let (Some(settings), Some(store), Some(sender), Some(spill)) =
                (&self.settings, &self.store, &self.sender, &self.spill) else { return; };
            self.watchers.remove(&path);
            match source_watcher(&path, settings, &store.home, sender.clone(), spill.clone()) {
                Ok(watcher) => {
                    self.watchers.insert(path.clone(), watcher);
                    self.watcher_retries.remove(&path);
                    self.watched_roots.insert(path.clone());
                    self.watched = self.watched_roots.len();
                    self.watcher_recovery(vec![path.clone()], "folder monitor reconnected");
                    if let Some(tx) = &self.logger {
                        let _ = tx.send(Event::Log(
                            "Info".into(),
                            format!(
                                "Folder monitor {} reconnected; journal replay or affected-source catch-up will handle missed changes",
                                path.display()
                            ),
                        ));
                    }
                }
                Err(error) => {
                    let attempts = attempts.saturating_add(1);
                    let delay = (5u64 << attempts.saturating_sub(1).min(6)).min(300);
                    self.watcher_failures.insert(path.clone(), attempts);
                    self.watcher_retries
                        .insert(path.clone(), (attempts, now + Duration::from_secs(delay)));
                    if let Some(tx) = &self.logger {
                        let _ = tx.send(Event::Log(
                            "Warning".into(),
                            format!(
                                "Folder monitor {} unavailable; retry in {delay}s: {error}",
                                path.display()
                            ),
                        ));
                    }
                }
            }
        }
    }
    fn request_scoped_recovery(&mut self, paths: Vec<PathBuf>, reason: &str) {
        let mut roots = Vec::new();
        if let Some(settings) = &self.settings {
            for path in paths {
                let contained = settings.roots.iter().filter(|r| under(&r.path, &path.to_string_lossy())).collect::<Vec<_>>();
                if !contained.is_empty() {
                    roots.extend(contained.into_iter().map(|r| PathBuf::from(&r.path)));
                    continue;
                }
                if let Some(root) = settings
                    .roots
                    .iter()
                    .filter(|r| under(&path.to_string_lossy(), &r.path))
                    .max_by_key(|r| r.path.len())
                {
                    roots.push(PathBuf::from(&root.path));
                }
            }
        }
        roots.sort();
        roots.dedup();
        if let Some(tx) = &self.logger {
            let _ = tx.send(Event::Log(
                "Info".into(),
                format!(
                    "Scoped recovery ({reason}): {}",
                    roots
                        .iter()
                        .map(|r| r.display().to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            ));
        }
        for root in roots {
            self.queue_path(root);
        }
    }
    fn watcher_recovery(&mut self, paths: Vec<PathBuf>, reason: &str) {
        // Healthy journal replay already covers notification gaps. Do not create
        // a second source scan while those exact journal paths are being drained.
        let now = Instant::now();
        let mut uncovered = vec![];
        for path in paths {
            if !self.journal.covers(&path) { uncovered.push(path); continue; }
            if self.journal_notification_log.get(&path).is_none_or(|last| now.saturating_duration_since(*last) >= Duration::from_secs(60)) {
                if let Some(tx) = &self.logger { let _ = tx.send(Event::Log("Info".into(), format!("Folder notification gap for {} ({reason}): covered by NTFS replay; no source rescan queued", path.display()))); }
                self.journal_notification_log.insert(path, now);
            }
        }
        if !uncovered.is_empty() { self.request_scoped_recovery(uncovered, reason); }
    }
    fn request_recovery(&mut self, reason: &str) {
        self.recovery = true;
        self.recovery_reason = reason.into();
        self.dirty.insert(PathBuf::new()); // refresh the durable recovery generation/time too
        self.last = Some(Instant::now());
    }
    fn queue_path(&mut self, path: PathBuf) {
        if crate::config::ntfs_internal_path(&path.to_string_lossy()) {return;}
        let path=path.ancestors().find_map(|p|self.pending.get(p).cloned()).unwrap_or(path);
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
        self.retry_watches();
        let (paths, recovery) = self.journal.poll();
        if recovery {
            self.request_recovery("NTFS journal gap or inaccessible journal");
        }
        if let Some(db) = &self.store {
            match self.journal.resolve_missing(db) {
                Ok(candidates) => for path in candidates {
                    if s.searched(&path) && !under(&path.to_string_lossy(), &home.to_string_lossy()) {
                        self.queue_path(path);
                    }
                },
                Err(error) => if let Some(tx) = &self.logger {
                    let _ = tx.send(Event::Log("Warning".into(), format!("Targeted journal filename lookup pending; retrying: {error:#}")));
                }
            }
        }
        for path in paths {
            if !under(&path.to_string_lossy(), &home.to_string_lossy()) && s.searched(&path) {
                self.queue_path(path);
            }
        }
        let spilled = self.spill.as_ref().and_then(|buffer| {
            buffer.lock().ok().map(|mut buffer| {
                let paths = buffer.paths.drain().collect::<Vec<_>>();
                let roots=buffer.roots.drain().collect::<Vec<_>>();
                let failed=buffer.failed.drain().collect::<Vec<_>>();
                if !roots.is_empty() {
                    if let Some(tx)=&self.logger {let _=tx.send(Event::Log("Warning".into(),format!("Notification overflow retained source identity; journal replay or source-only recovery: {}",roots.iter().map(|r|r.display().to_string()).collect::<Vec<_>>().join("; "))));}
                }
                (paths, roots, failed)
            })
        });
        if let Some((paths, roots, failed)) = spilled {
            self.watcher_recovery(roots, "notification buffer overflow on affected source");
            self.schedule_watch_retry(&failed);
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
                    if !event.need_rescan(){
                        for path in &event.paths {
                            for root in &self.watched_roots {if under(&path.to_string_lossy(),&root.to_string_lossy()){self.watcher_failures.remove(root);}}
                        }
                    }
                    if event.need_rescan() {
                        self.watcher_recovery(event.paths.clone(), "Windows/provider requested a rescan after missed events");
                        continue;
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
                    self.schedule_watch_retry(&error.paths);
                    if let Some(tx)=&self.logger{let _=tx.send(Event::Log("Warning".into(),format!("Folder watcher failed; reattachment queued: {error}")));}
                    if error.paths.is_empty(){self.watcher_recovery(vec![],"folder watcher failed without a source path");}
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
        let checkpoint_home=db.home.clone();
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
        self.journal.begin();
        self.journal.finish(true,&checkpoint_home);
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
    pub fn pending_work(&self)->bool {self.recovery||!self.pending.is_empty()}
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
        // Startup catch-up is represented by durable source paths, never a
        // second all-source scan after finishing targeted work.
        if success { self.startup_recovery = false; }
    }
    pub fn try_take(&mut self) -> anyhow::Result<Option<Vec<PathBuf>>> {
        if let Err(error) = self.persist() {
            self.retry_after = Some(Instant::now() + Duration::from_secs(30));
            return Err(error);
        }
        Ok(self.take())
    }
    pub fn take(&mut self) -> Option<Vec<PathBuf>> {
        let _ = self.persist();
        self.journal.begin();
        self.journal_inflight = true;
        self.last = None;
        self.first = None;
        let queued = if self.recovery {
            self.recovery = false;
            self.recovery_saved = false;
            self.pending.clear();
            None
        } else {
            Some(self.pending.drain().collect::<Vec<_>>())
        };
        let paths=queued.as_ref().map(|paths| {
            let mut ordered=paths.clone();ordered.sort_by_key(|p|p.components().count());
            let mut selected:Vec<PathBuf>=Vec::new();let mut prefixes=HashSet::new();
            for path in ordered {if !path.ancestors().any(|p|prefixes.contains(&crate::config::fold(p))) {prefixes.insert(crate::config::fold(&path));selected.push(path);}}
            selected
        });
        let mut inflight = vec![];
        if let Some(db) = &self.store {
            if let Ok(c) = db.connect(true) {
                if let Some(paths) = &queued {
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
        assert_eq!(resumed.take().unwrap(), vec![root.path().to_path_buf()]);
        resumed.complete(false, home.path());
        assert!(!resumed.recovery);
        assert_eq!(resumed.take().unwrap(), vec![root.path().to_path_buf()]);
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
        assert_eq!(resumed.take().unwrap(), vec![root.path().to_path_buf()]);
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

#[cfg(test)]
mod scoped_recovery_tests {
    use super::*;
    #[test]
    fn scoped_recovery_is_coalesced_and_never_schedules_second_global_scan() {
        let mut m = Monitor::default();
        let mut s = Settings::default();
        s.roots = ["C:\\", "T:\\"]
            .iter()
            .map(|p| crate::config::Source {
                path: (*p).into(),
                mode: "quick".into(),
            })
            .collect();
        m.settings = Some(s);
        m.request_scoped_recovery(vec![PathBuf::from("T:\\folder")], "connection lost");
        m.queue_path(PathBuf::from("T:\\folder\\file.txt"));
        m.queue_path(PathBuf::from("C:\\Users\\changed.txt"));
        let paths = m.take().unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&PathBuf::from("T:\\")));
        assert!(!paths.contains(&PathBuf::from("C:\\")));
        m.complete(true, Path::new("unused"));
        assert!(!m.recovery && m.pending.is_empty());
    }
    #[test]
    fn internal_changes_do_not_enter_queue_even_with_exclusions_disabled() {
        let mut m = Monitor::default();
        m.queue_path(PathBuf::from("C:\\$Extend\\$Deleted\\abc"));
        assert!(m.pending.is_empty() && m.dirty.is_empty());
    }
    #[test]
    fn scoped_completion_preserves_change_received_during_source_scan() {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.roots = vec![crate::config::Source {
            path: root.path().to_string_lossy().into(),
            mode: "quick".into(),
        }];
        let mut m = Monitor::default();
        m.settings = Some(s.clone());
        let db = crate::db::Database::new(home.path(), &s).unwrap();
        db.connect(false).unwrap().execute_batch("CREATE TABLE IF NOT EXISTS native_changes(path TEXT PRIMARY KEY,generation INTEGER NOT NULL,queued INTEGER NOT NULL)").unwrap();
        m.store = Some(db);
        m.queue_path(root.path().join("before.txt"));
        m.queue_path(root.path().to_path_buf());
        assert_eq!(m.take().unwrap(), vec![root.path().to_path_buf()]);
        let after = root.path().join("after.txt");
        m.queue_path(after.clone());
        m.complete(true, home.path());
        assert_eq!(m.take().unwrap(), vec![after]);
    }
    #[test]
    fn real_watcher_reconnects_after_root_recreated_and_catches_up_only_that_root() {
        let home = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("watched");
        std::fs::create_dir(&root).unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.watch_changes = true;
        s.roots = vec![crate::config::Source {
            path: root.to_string_lossy().into(),
            mode: "quick".into(),
        }];
        let (tx, _) = crossbeam_channel::unbounded();
        let mut m = Monitor::default();
        m.configure(&s, home.path(), &tx.into());
        m.take();
        m.complete(true, home.path());
        std::fs::remove_dir(&root).unwrap();
        let start = Instant::now();
        while m.watcher_retries.is_empty() && start.elapsed() < Duration::from_secs(4) {
            m.collect(&s, home.path());
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(
            !m.watcher_retries.is_empty(),
            "failed Windows watch must be reported"
        );
        std::fs::create_dir(&root).unwrap();
        for (_, at) in m.watcher_retries.values_mut() {
            *at = Instant::now();
        }
        m.collect(&s, home.path());
        assert!(m.watcher_retries.is_empty());
        assert_eq!(m.take().unwrap(), vec![root.clone()]);
        m.complete(true, home.path());
        let file = root.join("after-reconnect.txt");
        std::fs::write(&file, "reconnected").unwrap();
        let start = Instant::now();
        while !m.pending.contains(&file) && start.elapsed() < Duration::from_secs(4) {
            m.collect(&s, home.path());
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(m.pending.contains(&file));
        assert!(!m.recovery);
    }
}

#[cfg(test)]
mod recovery_persistence_tests {
    use super::*;
    #[test]
    fn persistence_failure_keeps_pending_work_and_does_not_dispatch() {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.roots = vec![crate::config::Source {
            path: root.path().to_string_lossy().into(),
            mode: "quick".into(),
        }];
        let db = crate::db::Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        c.execute_batch("CREATE TRIGGER refuse_changes BEFORE INSERT ON native_changes BEGIN SELECT RAISE(FAIL,'injected queue failure'); END;").unwrap();
        let mut m = Monitor::default();
        m.store = Some(db);
        let path = root.path().join("pending.txt");
        m.queue_path(path.clone());
        assert!(m.try_take().is_err());
        assert!(m.pending.contains(&path) && m.dirty.contains(&path));
        assert!(!m.journal_inflight);
        assert!(!m.ready());
        c.execute_batch("DROP TRIGGER refuse_changes").unwrap();
        assert_eq!(m.try_take().unwrap().unwrap(), vec![path]);
    }
    #[test]
    fn asynchronous_watch_errors_keep_exponential_backoff_until_events_resume() {
        let mut m = Monitor::default();
        let path = PathBuf::from("T:\\");
        for n in 1..=4 {
            m.schedule_watch_retry(&[path.clone()]);
            assert_eq!(m.watcher_retries[&path].0, n);
            assert_eq!(m.watcher_failures[&path], n);
            m.watcher_retries.clear();
        }
    }
}

#[cfg(test)]
mod notification_origin_tests {
    use super::*;
    fn settings() -> Settings {
        let mut s = Settings::default();
        s.roots = [r"C:\", r"D:\", r"T:\"].iter().map(|p| crate::config::Source {
            path: (*p).into(), mode: "quick".into(),
        }).collect();
        s
    }
    #[test]
    fn empty_windows_overflow_keeps_its_source_even_when_channel_is_full() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let spill = std::sync::Mutex::new(NotificationSpill::default());
        for _ in 0..1000 {
            deliver_notification(Ok(notify::Event::new(notify::EventKind::Other)
                .set_flag(notify::event::Flag::Rescan)), Path::new(r"D:\"),
                &settings(), Path::new(r"C:\AppData\VectorDash"), &tx, &spill);
        }
        let event = rx.try_recv().unwrap().unwrap();
        assert_eq!(event.paths, vec![PathBuf::from(r"D:\")]);
        assert_eq!(spill.lock().unwrap().roots, HashSet::from([PathBuf::from(r"D:\")]));
        let mut m = Monitor::default();
        m.settings = Some(settings());
        m.watcher_recovery(event.paths, "fixture");
        assert_eq!(m.take().unwrap(), vec![PathBuf::from(r"D:\")]);
        assert!(!m.recovery);
    }
    #[test]
    fn saturated_error_retains_origin_for_reattachment() {
        let (tx, _rx) = std::sync::mpsc::sync_channel(0);
        let spill = std::sync::Mutex::new(NotificationSpill::default());
        deliver_notification(Err(notify::Error::generic("lost connection")), Path::new(r"T:\"),
            &settings(), Path::new(r"C:\AppData\VectorDash"), &tx, &spill);
        assert_eq!(spill.lock().unwrap().failed, HashSet::from([PathBuf::from(r"T:\")]));
        assert!(spill.lock().unwrap().roots.is_empty());
    }
}

#[cfg(all(test, windows))]
mod live_journal_overflow_tests {
    use super::*;
    #[test]
    #[ignore = "Requires accessible NTFS journal and explicit test directory"]
    fn real_changes_survive_repeated_overflows_without_source_rescans() {
        let base = PathBuf::from(std::env::var("VECTORDASH_JOURNAL_TEST_DIR").expect("explicit fixture directory"));
        let home = tempfile::tempdir_in(&base).unwrap();
        let root = tempfile::Builder::new().prefix("recovery-live-").tempdir_in(&base).unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.watch_changes = true;
        s.ntfs_journal = true;
        s.include_hidden = true;
        s.roots = vec![crate::config::Source { path: root.path().to_string_lossy().into(), mode: "quick".into() }];
        let (tx, _rx) = crossbeam_channel::unbounded();
        let mut m = Monitor::default();
        m.configure(&s, home.path(), &tx.into());
        assert!(m.journal.covers(root.path()), "fixture needs an accessible journal");
        m.take();
        m.complete(true, home.path());
        let file = root.path().join("live-target.txt");
        assert!(s.searched(&file), "fixture must pass the production source filters");
        for step in 0..3 {
            if step == 2 { std::fs::remove_file(&file).unwrap(); }
            else { std::fs::write(&file, format!("change {step}")).unwrap(); }
            for _ in 0..1500 {
                deliver_notification(Ok(notify::Event::new(notify::EventKind::Other)
                    .set_flag(notify::event::Flag::Rescan)), root.path(), &s, home.path(),
                    m.sender.as_ref().unwrap(), m.spill.as_ref().unwrap());
            }
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(5) {
                m.collect(&s, home.path());
                assert!(!m.pending.contains(root.path()), "overflow must not queue the source root");
                assert!(!m.recovery);
                if m.pending.contains(&file) { break; }
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(m.pending.contains(&file), "create/modify/delete must stay targeted: step {step}");
            assert_eq!(m.take().unwrap(), vec![file.clone()]);
            m.complete(true, home.path());
        }
    }
}
