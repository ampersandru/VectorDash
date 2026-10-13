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
#[derive(Default)]
pub struct Monitor {
    watcher: Option<notify::RecommendedWatcher>,
    rx: Option<std::sync::mpsc::Receiver<notify::Result<notify::Event>>>,
    signature: String,
    overflow: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pending: HashSet<PathBuf>,
    recovery: bool,
    last: Option<Instant>,
    first: Option<Instant>,
    watched: usize,
    journal: crate::journal::Tracker,
    journal_inflight: bool,
}
impl Monitor {
    pub fn configure(&mut self, s: &Settings, home: &Path, tx: &NotifySender) {
        let signature = format!(
            "{}|{}|{:?}|{:?}|{}|{}",
            s.watch_changes,
            s.ntfs_journal,
            s.roots,
            s.exclusions,
            s.excluded_types,
            s.skip_windows_churn
        );
        if signature == self.signature {
            return;
        }
        self.signature = signature;
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
        let (sender, rx) = std::sync::mpsc::sync_channel(1024);
        let overflow = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = overflow.clone();
        let mut watcher =
            match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let event = event.map(|mut e| {
                    if !e.need_rescan() {
                        e.paths.retain(|p| !directory_metadata_only(&e.kind, p));
                    }
                    e
                });
                if event
                    .as_ref()
                    .is_ok_and(|e| e.paths.is_empty() && !e.need_rescan())
                {
                    return;
                }
                if sender.try_send(event).is_err() {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
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
            self.recovery = true;
            self.last = Some(Instant::now());
            self.first = self.last;
        }
        self.overflow = Some(overflow);
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
    pub fn collect(&mut self, s: &Settings, home: &Path) {
        let (paths, recovery) = self.journal.poll();
        if recovery {
            self.recovery = true;
            self.last = Some(Instant::now());
        }
        for path in paths {
            if !under(&path.to_string_lossy(), &home.to_string_lossy())
                && s.roots
                    .iter()
                    .any(|r| under(&path.to_string_lossy(), &r.path))
                && s.allowed(&path)
            {
                if self.pending.len() < 4096 {
                    self.pending.insert(path);
                } else {
                    self.recovery = true;
                    self.pending.clear();
                }
                self.last = Some(Instant::now());
            }
        }

        if self
            .overflow
            .as_ref()
            .is_some_and(|f| f.swap(false, std::sync::atomic::Ordering::Relaxed))
        {
            self.recovery = true;
            self.last = Some(Instant::now());
        }
        if let Some(rx) = &self.rx {
            for result in rx.try_iter().take(1024) {
                match result {
                    Ok(event) => {
                        if event.need_rescan() {
                            self.recovery = true;
                            self.last = Some(Instant::now());
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
                                    .any(|r| under(&path.to_string_lossy(), &r.path))
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
                                self.last = Some(Instant::now());
                                if self.pending.len() < 4096 {
                                    self.pending.insert(path);
                                } else {
                                    self.recovery = true;
                                    self.pending.clear();
                                }
                            }
                        }
                    }
                    Err(_) => {
                        self.recovery = true;
                        self.last = Some(Instant::now());
                    }
                }
            }
        }
        if (self.recovery || !self.pending.is_empty()) && self.first.is_none() {
            self.first = Some(Instant::now());
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
            "catch-up/recovery scan queued".into()
        } else {
            format!("{} changed paths queued", self.pending.len())
        };
        let waiting = if busy {
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
            "Watching {} sources · {} NTFS journals · {pending} · {waiting}",
            self.watched,
            self.journal.active()
        )
    }
    pub fn ready(&self) -> bool {
        (self.recovery || !self.pending.is_empty())
            && (self
                .last
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(10))
                || self
                    .first
                    .is_some_and(|t| t.elapsed() >= Duration::from_secs(60)))
    }
    pub fn complete(&mut self, success: bool, home: &Path) {
        if self.journal_inflight {
            self.journal.finish(success, home);
            self.journal_inflight = false;
            if !success {
                self.recovery = true;
                self.last = Some(Instant::now());
                self.first = self.last;
            }
        }
    }
    pub fn take(&mut self) -> Option<Vec<PathBuf>> {
        self.journal.begin();
        self.journal_inflight = true;
        self.last = None;
        self.first = None;
        if self.recovery {
            self.recovery = false;
            self.pending.clear();
            None
        } else {
            Some(self.pending.drain().collect())
        }
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
        let root = tempfile::tempdir().unwrap();
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
