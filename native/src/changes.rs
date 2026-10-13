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
}
impl Monitor {
    pub fn configure(&mut self, s: &Settings, home: &Path, tx: &NotifySender) {
        let signature = format!("{}|{:?}", s.watch_changes, s.roots);
        if signature == self.signature {
            return;
        }
        self.signature = signature;
        self.watcher = None;
        self.rx = None;
        self.pending.clear();
        self.recovery = false;
        self.last = None;
        if !s.watch_changes {
            return;
        }
        let (sender, rx) = std::sync::mpsc::sync_channel(1024);
        let overflow = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = overflow.clone();
        let mut watcher = match notify::recommended_watcher(move |event| {
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
    }
    pub fn ready(&self) -> bool {
        (self.recovery || !self.pending.is_empty())
            && self
                .last
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(10))
    }
    pub fn take(&mut self) -> Option<Vec<PathBuf>> {
        self.last = None;
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
        let source = tempfile::Builder::new()
            .prefix("VectorDash-watch-")
            .tempdir()
            .unwrap();
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
