//! Incremental SMB safety reconciliation. No recursive filesystem walk is queued.
//! Directory progress and exact changed paths are committed together; incomplete
//! listings never establish deletion. Blocking SMB calls stay off the scheduler/UI.
use crate::{config::Settings, db::Database};
use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
const ENTRIES: usize = 256;
struct Cursor {
    root: PathBuf,
    path: PathBuf,
    reader: Option<fs::ReadDir>,
}
pub struct Report {
    pub root: PathBuf,
    pub checked: usize,
    pub changed: usize,
    pub folders_changed: bool,
    pub done: bool,
    pub warning: Option<String>,
    pub run_checked: usize,
    pub run_changed: usize,
}
struct Output {
    cursor: Option<Cursor>,
    report: Report,
}
#[derive(Default)]
pub struct Reconciler {
    root: Option<PathBuf>,
    cursor: Option<Cursor>,
    queued: VecDeque<PathBuf>,
    reply: Option<mpsc::Receiver<(u64, Result<Output>)>>,
    generation: u64,
    cancel: Arc<AtomicBool>,
    requests: HashMap<PathBuf, (bool, i64)>,
    totals: HashMap<PathBuf, (usize, usize)>,
    fresh: HashSet<PathBuf>,
}
impl Drop for Reconciler {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Reconciler {
    pub fn reset(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation = self.generation.wrapping_add(1);
        self.root = None;
        self.cursor = None;
        self.queued.clear();
        self.requests.clear();
        self.totals.clear();
        self.fresh.clear();
    }
    pub fn request(&mut self, root: PathBuf) {
        self.requests
            .entry(root.clone())
            .or_insert((false, chrono::Utc::now().timestamp_millis()));
        if self.root.as_ref() != Some(&root) && !self.queued.contains(&root) {
            self.queued.push_back(root);
        }
    }
    pub fn request_recovery(&mut self, root: PathBuf) {
        self.fresh.insert(root.clone());
        self.requests
            .entry(root.clone())
            .and_modify(|r| {
                *r = (true, chrono::Utc::now().timestamp_millis());
            })
            .or_insert((true, chrono::Utc::now().timestamp_millis()));
        self.request(root);
    }
    fn retire(&mut self, roots: &[PathBuf]) {
        for root in roots {
            self.requests.remove(root);
            self.totals.remove(root);
            self.fresh.remove(root);
        }
        self.queued.retain(|p| !roots.contains(p));
        if self.root.as_ref().is_some_and(|p| roots.contains(p)) {
            self.cancel.store(true, Ordering::Release);
            self.cancel = Arc::new(AtomicBool::new(false));
            self.generation = self.generation.wrapping_add(1);
            self.root = None;
            self.cursor = None;
        }
    }
    pub fn disable_periodic(&mut self) {
        let roots = self
            .requests
            .iter()
            .filter(|(_, r)| !r.0)
            .map(|(p, _)| p.clone())
            .collect::<Vec<_>>();
        self.retire(&roots);
    }
    pub fn covered_before(&mut self, started: i64) -> Vec<PathBuf> {
        let roots = self
            .requests
            .iter()
            .filter(|(_, r)| !r.0 || r.1 < started)
            .map(|(p, _)| p.clone())
            .collect::<Vec<_>>();
        self.retire(&roots);
        roots
    }
    pub fn has_root(&self, root: &Path) -> bool {
        self.requests.contains_key(root)
    }
    pub fn pending(&self) -> bool {
        self.root.is_some() || !self.queued.is_empty() || self.reply.is_some()
    }
    pub fn poll(&mut self, db: &Database, s: &Settings, allow: bool) -> Option<Result<Report>> {
        let mut report = None;
        if let Some(rx) = &self.reply {
            match rx.try_recv() {
                Ok((generation, result)) => {
                    self.reply = None;
                    if generation == self.generation {
                        match result {
                            Ok(mut out) => {
                                let total = self.totals.entry(out.report.root.clone()).or_default();
                                total.0 += out.report.checked;
                                total.1 += out.report.changed;
                                out.report.run_checked = total.0;
                                out.report.run_changed = total.1;
                                if self.fresh.contains(&out.report.root) {
                                    self.cursor = None;
                                    out.report.done = false;
                                } else {
                                    self.cursor = out.cursor;
                                }
                                if out.report.done {
                                    self.requests.remove(&out.report.root);
                                    self.totals.remove(&out.report.root);
                                    self.root = None;
                                }
                                report = Some(Ok(out.report));
                            }
                            Err(e)
                                if self
                                    .root
                                    .as_ref()
                                    .is_some_and(|root| self.fresh.contains(root)) =>
                            {
                                self.cursor = None;
                                report = Some(Err(e));
                            }
                            Err(e) => {
                                if let Some(root) = &self.root {
                                    self.requests.remove(root);
                                    self.totals.remove(root);
                                }
                                self.cursor = None;
                                self.root = None;
                                report = Some(Err(e));
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.reply = None;
                    if let Some(root) = &self.root {
                        self.requests.remove(root);
                        self.totals.remove(root);
                    }
                    self.cursor = None;
                    self.root = None;
                    report = Some(Err(anyhow::anyhow!(
                        "Network reconciliation worker stopped"
                    )));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if allow && self.reply.is_none() {
            if self.root.is_none() {
                self.root = self.queued.pop_front();
            }
            if let Some(root) = self.root.clone() {
                let cursor = self.cursor.take();
                let db = db.clone();
                let s = s.clone();
                let cancel = self.cancel.clone();
                let generation = self.generation;
                let fresh = self.fresh.remove(&root);
                let (tx, rx) = mpsc::channel();
                self.reply = Some(rx);
                std::thread::spawn(move || {
                    let result = (|| {
                        if fresh {
                            reset_progress(&db, &root, &cancel)?;
                        }
                        run_budget(&db, &s, root, if fresh { None } else { cursor }, &cancel)
                    })();
                    let _ = tx.send((generation, result));
                });
            }
        }
        report
    }
}
fn reset_progress(db: &Database, root: &Path, cancel: &AtomicBool) -> Result<()> {
    let mut c = db.connect(false)?;
    schema(&c)?;
    let tx = c.transaction()?;
    anyhow::ensure!(
        !cancel.load(Ordering::Acquire),
        "Network reconciliation cancelled"
    );
    tx.execute(
        "DELETE FROM native_network_dirs WHERE root=?1",
        [root.to_string_lossy().as_ref()],
    )?;
    tx.execute(
        "DELETE FROM native_network_seen WHERE root=?1",
        [root.to_string_lossy().as_ref()],
    )?;
    anyhow::ensure!(
        !cancel.load(Ordering::Acquire),
        "Network reconciliation cancelled"
    );
    tx.commit()?;
    Ok(())
}
fn run_budget(
    db: &Database,
    s: &Settings,
    root: PathBuf,
    cursor: Option<Cursor>,
    cancel: &AtomicBool,
) -> Result<Output> {
    let began = Instant::now();
    let mut out = step(db, s, root.clone(), cursor, cancel)?;
    for _ in 1..16 {
        if out.report.done
            || out.cursor.is_some()
            || out.report.warning.is_some()
            || out.report.checked >= ENTRIES
            || began.elapsed() >= Duration::from_millis(100)
        {
            break;
        }
        let mut next = step_limited(
            db,
            s,
            root.clone(),
            None,
            cancel,
            ENTRIES - out.report.checked,
        )?;
        next.report.checked += out.report.checked;
        next.report.changed += out.report.changed;
        next.report.folders_changed |= out.report.folders_changed;
        out = next;
    }
    Ok(out)
}
fn schema(c: &rusqlite::Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS native_changes(path TEXT PRIMARY KEY,generation INTEGER NOT NULL,queued INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS native_network_dirs(root TEXT NOT NULL,path TEXT NOT NULL,PRIMARY KEY(root,path));
    CREATE TABLE IF NOT EXISTS native_network_seen(root TEXT NOT NULL,directory TEXT NOT NULL,path TEXT NOT NULL COLLATE NOCASE,PRIMARY KEY(root,directory,path));
    CREATE INDEX IF NOT EXISTS names_parent_network ON names(parent COLLATE NOCASE);")?;
    Ok(())
}
fn enqueue(c: &rusqlite::Connection, path: &Path, generation: i64) -> Result<()> {
    c.prepare_cached("INSERT INTO native_changes VALUES(?1,?2,?3) ON CONFLICT(path) DO UPDATE SET generation=excluded.generation,queued=excluded.queued")?.execute(params![path.to_string_lossy(),generation,chrono::Utc::now().timestamp_millis()])?;
    Ok(())
}
fn mark(c: &rusqlite::Connection, root: &Path, dir: &Path, path: &Path) -> Result<()> {
    c.prepare_cached("INSERT OR IGNORE INTO native_network_seen VALUES(?1,?2,?3)")?
        .execute(params![
            root.to_string_lossy(),
            dir.to_string_lossy(),
            path.to_string_lossy()
        ])?;
    Ok(())
}
fn step(
    db: &Database,
    s: &Settings,
    root: PathBuf,
    cursor: Option<Cursor>,
    cancel: &AtomicBool,
) -> Result<Output> {
    step_limited(db, s, root, cursor, cancel, ENTRIES)
}
fn step_limited(
    db: &Database,
    s: &Settings,
    root: PathBuf,
    cursor: Option<Cursor>,
    cancel: &AtomicBool,
    limit: usize,
) -> Result<Output> {
    anyhow::ensure!(
        !cancel.load(Ordering::Acquire),
        "Network reconciliation cancelled"
    );
    let mut c = db.connect(false)?;
    schema(&c)?;
    let mut cursor = if let Some(cursor) = cursor {
        cursor
    } else {
        let outstanding: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM native_network_dirs WHERE root=?1)",
            [root.to_string_lossy().as_ref()],
            |r| r.get(0),
        )?;
        if !outstanding {
            c.execute(
                "INSERT OR IGNORE INTO native_network_dirs VALUES(?1,?1)",
                [root.to_string_lossy().as_ref()],
            )?;
        }
        let path: String = c.query_row(
            "SELECT path FROM native_network_dirs WHERE root=?1 ORDER BY length(path),path LIMIT 1",
            [root.to_string_lossy().as_ref()],
            |r| r.get(0),
        )?;
        let path = PathBuf::from(path);
        if !s.searched(&path) {
            c.execute(
                "DELETE FROM native_network_dirs WHERE root=?1 AND path=?2",
                params![root.to_string_lossy(), path.to_string_lossy()],
            )?;
            return finish_directory(c, &root, &path, false, 0, 0, false);
        }
        let physical = crate::platform::preview_path(&path.to_string_lossy());
        // A disconnected/unreadable root never proves that files were deleted.
        let reader = match fs::read_dir(&physical) {
            Ok(reader) => reader,
            Err(error) if path != root => {
                return skip_directory(c, &root, &path, &error.to_string(), cancel);
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Cannot list network root {}", root.display()));
            }
        };
        c.execute(
            "DELETE FROM native_network_seen WHERE root=?1 AND directory=?2",
            params![root.to_string_lossy(), path.to_string_lossy()],
        )?;
        Cursor {
            root: root.clone(),
            path,
            reader: Some(reader),
        }
    };
    let began = Instant::now();
    let mut entries = vec![];
    let mut finished = false;
    if let Some(reader) = &mut cursor.reader {
        for _ in 0..limit {
            anyhow::ensure!(
                !cancel.load(Ordering::Acquire),
                "Network reconciliation cancelled"
            );
            match reader.next() {
                Some(entry) => {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(error) => {
                            return skip_directory(
                                c,
                                &root,
                                &cursor.path,
                                &error.to_string(),
                                cancel,
                            );
                        }
                    };
                    let logical = cursor.path.join(entry.file_name());
                    let meta = match entry.metadata() {
                        Ok(meta) => meta,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(error) => {
                            return skip_directory(
                                c,
                                &root,
                                &cursor.path,
                                &error.to_string(),
                                cancel,
                            );
                        }
                    };
                    entries.push((logical, meta));
                }
                None => {
                    finished = true;
                    break;
                }
            }
            if began.elapsed() >= Duration::from_millis(100) {
                break;
            }
        }
        if finished {
            cursor.reader = None;
        }
    }
    let mut checked = entries.len();
    let mut changed = 0;
    let mut folders_changed = false;
    let tx = c.transaction()?;
    let generation: i64 = tx.query_row(
        "SELECT coalesce(max(generation),0)+1 FROM native_changes",
        [],
        |r| r.get(0),
    )?;
    for (path, meta) in entries {
        mark(&tx, &root, &cursor.path, &path)?;
        if meta.file_type().is_symlink() || !s.searched(&path) {
            continue;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
            .unwrap_or(-1);
        let old:Option<(i64,i64,String,String)>=tx.query_row("SELECT coalesce(mtime,-1),coalesce(size,-1),coalesce(index_mode,''),coalesce(kind,'') FROM names WHERE path=?1",[path.to_string_lossy().as_ref()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let mode = crate::config::mode(&path.to_string_lossy(), s);
        if meta.is_dir() {
            if old.as_ref().is_none_or(|(mt, size, old_mode, kind)| {
                *mt != mtime
                    || i64::try_from(meta.len()).ok() != Some(*size)
                    || old_mode != mode
                    || kind != "folder"
            }) {
                Database::touch_name(
                    &tx,
                    &path,
                    "folder",
                    mtime,
                    meta.len(),
                    mode,
                    &root.to_string_lossy(),
                    0,
                )?;
                folders_changed = true;
            }
            tx.execute(
                "INSERT OR IGNORE INTO native_network_dirs VALUES(?1,?2)",
                params![root.to_string_lossy(), path.to_string_lossy()],
            )?;
        } else if meta.is_file()
            && old.is_none_or(|(mt, size, old_mode, kind)| {
                mt != mtime
                    || i64::try_from(meta.len()).ok() != Some(size)
                    || old_mode != mode
                    || kind != "file"
            })
        {
            enqueue(&tx, &path, generation)?;
            changed += 1;
        }
    }
    // Only a complete, error-free listing can establish a missing immediate child.
    if cursor.reader.is_none() && checked < limit {
        let remaining = limit - checked;
        let missing = {
            let mut stmt=tx.prepare("SELECT path FROM names WHERE parent=?1 COLLATE NOCASE AND NOT EXISTS(SELECT 1 FROM native_network_seen s WHERE s.root=?2 AND s.directory=?1 AND s.path=names.path COLLATE NOCASE) LIMIT ?3")?;
            stmt.query_map(
                params![
                    cursor.path.to_string_lossy(),
                    root.to_string_lossy(),
                    remaining
                ],
                |r| r.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        checked += missing.len();
        for path in &missing {
            let path = Path::new(path);
            mark(&tx, &root, &cursor.path, path)?;
            if s.searched(path) {
                enqueue(&tx, path, generation)?;
                changed += 1;
            }
        }
        if missing.len() < remaining {
            tx.execute(
                "DELETE FROM native_network_dirs WHERE root=?1 AND path=?2",
                params![root.to_string_lossy(), cursor.path.to_string_lossy()],
            )?;
            tx.execute(
                "DELETE FROM native_network_seen WHERE root=?1 AND directory=?2",
                params![root.to_string_lossy(), cursor.path.to_string_lossy()],
            )?;
            anyhow::ensure!(
                !cancel.load(Ordering::Acquire),
                "Network reconciliation cancelled"
            );
            tx.commit()?;
            return finish_directory(
                c,
                &root,
                &cursor.path,
                true,
                checked,
                changed,
                folders_changed,
            );
        }
    }
    anyhow::ensure!(
        !cancel.load(Ordering::Acquire),
        "Network reconciliation cancelled"
    );
    tx.commit()?;
    Ok(Output {
        cursor: Some(cursor),
        report: Report {
            root,
            checked,
            changed,
            folders_changed,
            done: false,
            warning: None,
            run_checked: 0,
            run_changed: 0,
        },
    })
}
fn skip_directory(
    c: rusqlite::Connection,
    root: &Path,
    path: &Path,
    error: &str,
    cancel: &AtomicBool,
) -> Result<Output> {
    anyhow::ensure!(
        !cancel.load(Ordering::Acquire),
        "Network reconciliation cancelled"
    );
    // Keep all indexed children. The next pass can retry the branch; no partial
    // enumeration is allowed to manufacture deletion or block every other branch.
    c.execute(
        "DELETE FROM native_network_dirs WHERE root=?1 AND path=?2",
        params![root.to_string_lossy(), path.to_string_lossy()],
    )?;
    c.execute(
        "DELETE FROM native_network_seen WHERE root=?1 AND directory=?2",
        params![root.to_string_lossy(), path.to_string_lossy()],
    )?;
    let mut out = finish_directory(c, root, path, false, 0, 0, false)?;
    out.report.warning = Some(format!(
        "{}: {error}; incomplete listing retained indexed files",
        path.display()
    ));
    Ok(out)
}
fn finish_directory(
    c: rusqlite::Connection,
    root: &Path,
    _path: &Path,
    _complete: bool,
    checked: usize,
    changed: usize,
    folders_changed: bool,
) -> Result<Output> {
    let remaining: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM native_network_dirs WHERE root=?1)",
        [root.to_string_lossy().as_ref()],
        |r| r.get(0),
    )?;
    Ok(Output {
        cursor: None,
        report: Report {
            root: root.into(),
            checked,
            changed,
            folders_changed,
            done: !remaining,
            warning: None,
            run_checked: 0,
            run_changed: 0,
        },
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Settings, Database) {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::Builder::new()
            .prefix("VectorDash-network-")
            .tempdir()
            .unwrap();
        let s = Settings {
            encrypt_index: false,
            include_hidden: true,
            skip_windows_churn: false,
            roots: vec![crate::config::Source {
                path: root.path().to_string_lossy().into(),
                mode: "quick".into(),
            }],
            ..Default::default()
        };
        let db = Database::new(home.path(), &s).unwrap();
        (home, root, s, db)
    }
    fn indexed(db: &Database, s: &Settings, path: &Path) {
        let m = fs::metadata(path).unwrap();
        let mt = m
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .min(i64::MAX as u128) as i64;
        Database::touch_name(
            &db.connect(false).unwrap(),
            path,
            "file",
            mt,
            m.len(),
            "quick",
            &s.roots[0].path,
            1,
        )
        .unwrap();
    }
    #[test]
    fn large_directory_is_incremental_and_only_changed_files_are_queued() {
        let (_h, r, s, db) = fixture();
        for i in 0..600 {
            let p = r.path().join(format!("file-{i}.txt"));
            fs::write(&p, "same").unwrap();
            indexed(&db, &s, &p);
        }
        fs::write(r.path().join("file-1.txt"), "changed longer").unwrap();
        fs::remove_file(r.path().join("file-2.txt")).unwrap();
        fs::write(r.path().join("new.txt"), "new").unwrap();
        let cancel = AtomicBool::new(false);
        let mut cursor = None;
        let mut steps = 0;
        loop {
            let out = step(&db, &s, r.path().into(), cursor, &cancel).unwrap();
            assert!(out.report.checked <= ENTRIES);
            steps += 1;
            if out.report.done {
                break;
            }
            cursor = out.cursor;
            assert!(steps < 20);
        }
        assert!(steps >= 3);
        let c = db.connect(true).unwrap();
        let mut stmt = c.prepare("SELECT path FROM native_changes").unwrap();
        let paths = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(paths.len(), 3);
        assert!(paths.iter().all(|p| Path::new(p).extension().is_some()));
    }

    #[test]
    fn small_directory_batches_are_budgeted_and_only_files_enter_index_queue() {
        let (_h, r, s, db) = fixture();
        for n in 0..20 {
            let dir = r.path().join(format!("branch-{n}"));
            fs::create_dir(&dir).unwrap();
            for i in 0..20 {
                fs::write(dir.join(format!("new-{i}.txt")), "new").unwrap();
            }
        }
        let cancel = AtomicBool::new(false);
        let mut cursor = None;
        for _ in 0..30 {
            let out = run_budget(&db, &s, r.path().into(), cursor, &cancel).unwrap();
            assert!(out.report.checked <= ENTRIES);
            if out.report.done {
                break;
            }
            cursor = out.cursor;
        }
        let c = db.connect(true).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM native_changes", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            400
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM native_network_dirs", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM names WHERE kind='folder'", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            20
        );
    }
    #[test]
    fn failed_branch_retains_index_and_does_not_starve_other_branches() {
        let (_h, r, s, db) = fixture();
        let inaccessible = r.path().join("not-a-directory.txt");
        fs::write(&inaccessible, "file").unwrap();
        indexed(&db, &s, &inaccessible);
        let c = db.connect(false).unwrap();
        schema(&c).unwrap();
        c.execute(
            "INSERT INTO native_network_dirs VALUES(?1,?2)",
            params![r.path().to_string_lossy(), inaccessible.to_string_lossy()],
        )
        .unwrap();
        drop(c);
        let out = step(&db, &s, r.path().into(), None, &AtomicBool::new(false)).unwrap();
        assert!(out.report.warning.is_some());
        assert!(out.report.done);
        assert_eq!(db.stats().unwrap().files, 1);
        assert_eq!(
            db.connect(true)
                .unwrap()
                .query_row("SELECT count(*) FROM native_changes", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn required_gap_rechecks_branches_completed_before_the_gap() {
        let (_h, r, s, db) = fixture();
        for branch in ["a", "b"] {
            let dir = r.path().join(branch);
            fs::create_dir(&dir).unwrap();
            let file = dir.join("one.txt");
            fs::write(&file, "before").unwrap();
            indexed(&db, &s, &file);
        }
        let cancel = AtomicBool::new(false);
        let out = step(&db, &s, r.path().into(), None, &cancel).unwrap();
        assert!(!out.report.done);
        let out = step(&db, &s, r.path().into(), None, &cancel).unwrap();
        assert!(!out.report.done);
        let changed = r.path().join("a").join("one.txt");
        fs::write(&changed, "changed after branch a was checked").unwrap();
        let mut reconciler = Reconciler::default();
        reconciler.request_recovery(r.path().into());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(report) = reconciler.poll(&db, &s, true) {
                if report.unwrap().done {
                    break;
                }
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            db.connect(true)
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM native_changes WHERE path=?1",
                    [changed.to_string_lossy().as_ref()],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }
    #[test]
    fn interrupted_listing_restarts_safely_and_missing_share_does_not_delete_index() {
        let (_h, r, s, db) = fixture();
        for i in 0..300 {
            let p = r.path().join(format!("file-{i}.txt"));
            fs::write(&p, "same").unwrap();
            indexed(&db, &s, &p);
        }
        let cancel = AtomicBool::new(false);
        let out = step(&db, &s, r.path().into(), None, &cancel).unwrap();
        assert!(!out.report.done);
        drop(out);
        let mut cursor = None;
        for _ in 0..10 {
            let out = step(&db, &s, r.path().into(), cursor, &cancel).unwrap();
            if out.report.done {
                break;
            }
            cursor = out.cursor;
        }
        assert_eq!(
            db.connect(true)
                .unwrap()
                .query_row("SELECT count(*) FROM native_changes", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(step(&db, &s, r.path().join("unavailable"), None, &cancel).is_err());
        assert_eq!(db.stats().unwrap().files, 300);
    }
}

#[cfg(test)]
mod request_tests {
    use super::*;
    #[test]
    fn disable_periodic_preserves_required_recovery_and_full_coverage_preserves_new_gaps() {
        let mut r = Reconciler::default();
        let a = PathBuf::from("periodic");
        let b = PathBuf::from("old-gap");
        let c = PathBuf::from("new-gap");
        r.request(a.clone());
        r.request_recovery(b.clone());
        r.disable_periodic();
        assert!(!r.has_root(&a));
        assert!(r.has_root(&b));
        r.request_recovery(c.clone());
        r.requests.get_mut(&b).unwrap().1 = 100;
        r.requests.get_mut(&c).unwrap().1 = 300;
        let retired = r.covered_before(200);
        assert!(retired.contains(&b));
        assert!(r.has_root(&c));
        assert!(r.pending());
    }
}

#[cfg(test)]
mod live_share_tests {
    use super::*;
    #[test]
    #[ignore = "Requires an explicitly supplied writable network test root; creates and removes only its own temporary folder"]
    fn live_network_metadata_reconciliation_queues_one_new_and_three_updates() {
        let share = std::env::var("VECTORDASH_TEST_NETWORK_ROOT").unwrap();
        let root = tempfile::Builder::new()
            .prefix("VectorDash-network-QA-")
            .tempdir_in(share)
            .unwrap();
        let home = tempfile::tempdir().unwrap();
        let s = Settings {
            encrypt_index: false,
            include_hidden: true,
            skip_windows_churn: false,
            roots: vec![crate::config::Source {
                path: root.path().to_string_lossy().into(),
                mode: "quick".into(),
            }],
            ..Default::default()
        };
        let db = Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        for n in 0..3 {
            let path = root.path().join(format!("existing-{n}.txt"));
            fs::write(&path, "before").unwrap();
            let m = fs::metadata(&path).unwrap();
            let mt = m
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .min(i64::MAX as u128) as i64;
            Database::touch_name(&c, &path, "file", mt, m.len(), "quick", &s.roots[0].path, 1)
                .unwrap();
            fs::write(&path, "after with a different length").unwrap();
        }
        drop(c);
        fs::write(root.path().join("new.txt"), "new").unwrap();
        let mut cursor = None;
        let cancel = AtomicBool::new(false);
        for _ in 0..20 {
            let out = step(&db, &s, root.path().into(), cursor, &cancel).unwrap();
            if out.report.done {
                break;
            }
            cursor = out.cursor;
        }
        let c = db.connect(true).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM native_changes", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM native_changes WHERE path=?1",
                [root.path().to_string_lossy().as_ref()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}

#[cfg(test)]
mod query_plan_tests {
    use super::*;
    #[test]
    fn missing_child_lookup_uses_the_full_seen_key_instead_of_scanning_each_listing() {
        let home = tempfile::tempdir().unwrap();
        let s = Settings {
            encrypt_index: false,
            ..Default::default()
        };
        let db = Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        schema(&c).unwrap();
        let mut stmt=c.prepare("EXPLAIN QUERY PLAN SELECT path FROM names WHERE parent=?1 COLLATE NOCASE AND NOT EXISTS(SELECT 1 FROM native_network_seen s WHERE s.root=?2 AND s.directory=?1 AND s.path=names.path COLLATE NOCASE) LIMIT 256").unwrap();
        let plan = stmt
            .query_map(params!["parent", "root"], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
            .join("\n");
        assert!(plan.contains("names_parent_network"));
        assert!(plan.contains("root=? AND directory=? AND path=?"), "{plan}");
    }
}
