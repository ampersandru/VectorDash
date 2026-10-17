use crate::config::{Settings, read_key};
use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
#[derive(Clone)]
pub struct Database {
    pub home: PathBuf,
    pub filename: String,
    pub key: Option<Vec<u8>>,
    pub generation: Arc<AtomicU64>,
    pub cache_kib: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Hit {
    pub path: String,
    pub kind: String,
    pub text: String,
    pub score: f32,
    pub reason: String,
    pub page: u32,
    pub seconds: f32,
    pub line: u32,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Stats {
    pub files: u64,
    pub quick: u64,
    pub full: u64,
    pub folders: u64,
    pub vectors: u64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct SourceStats {
    pub files: u64,
    pub added_hour: u64,
}
impl Database {
    pub fn new(home: &Path, s: &Settings) -> Result<Self> {
        std::fs::create_dir_all(home)?;
        let filename = std::fs::read_to_string(home.join("index.active"))
            .unwrap_or_else(|_| "index.sqlite".into());
        let filename = filename.trim().to_string();
        anyhow::ensure!(
            Path::new(&filename).file_name().and_then(|n| n.to_str()) == Some(filename.as_str()),
            "Invalid database selection"
        );
        let path = home.join(&filename);
        let actual = if path.exists() && std::fs::metadata(&path)?.len() > 0 {
            use std::io::Read;
            let mut header = [0u8; 16];
            std::fs::File::open(&path)?.read_exact(&mut header)?;
            &header != b"SQLite format 3\0"
        } else {
            s.encrypt_index
        };

        let db = Self {
            home: home.into(),
            filename,
            key: if actual { Some(read_key(home)?) } else { None },
            generation: Arc::new(AtomicU64::new(0)),
            cache_kib: match s.performance.as_str() {
                "Fast search" => 65536,
                "Low memory" => 8192,
                _ => 32768,
            },
        };
        let conn = db.connect(false)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS names(path TEXT PRIMARY KEY,name TEXT,parent TEXT,kind TEXT,folded TEXT,base TEXT,mtime INTEGER DEFAULT -1,size INTEGER DEFAULT -1,index_mode TEXT DEFAULT 'full'); CREATE INDEX IF NOT EXISTS names_base ON names(base); CREATE VIRTUAL TABLE IF NOT EXISTS name_words USING fts5(folded,content='names',content_rowid='rowid',tokenize='trigram'); CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY,text TEXT,vector TEXT,mtime INTEGER,size INTEGER); CREATE TABLE IF NOT EXISTS segments(path TEXT,namespace TEXT,kind TEXT,text TEXT,vector TEXT,location TEXT); CREATE INDEX IF NOT EXISTS segments_path ON segments(path,namespace); CREATE VIRTUAL TABLE IF NOT EXISTS words USING fts5(path,text,tokenize='unicode61'); CREATE TABLE IF NOT EXISTS segment_vectors(id INTEGER PRIMARY KEY,namespace TEXT,kind TEXT,channel TEXT,dimension INTEGER,vector BLOB,path TEXT); CREATE INDEX IF NOT EXISTS vectors_path ON segment_vectors(namespace,dimension,path,channel); CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT); CREATE TABLE IF NOT EXISTS source_seen(path TEXT PRIMARY KEY,source TEXT,epoch INTEGER,added INTEGER); CREATE INDEX IF NOT EXISTS seen_source ON source_seen(source,epoch); CREATE TABLE IF NOT EXISTS native_content(path TEXT PRIMARY KEY,fingerprint TEXT,error TEXT); ")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS native_jobs(path TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,token TEXT NOT NULL,error TEXT NOT NULL DEFAULT ''); CREATE TABLE IF NOT EXISTS native_staging(token TEXT,seq INTEGER,kind TEXT,text TEXT,location TEXT,dimension INTEGER,vector BLOB,PRIMARY KEY(token,seq)); CREATE TABLE IF NOT EXISTS native_changes(path TEXT PRIMARY KEY,generation INTEGER NOT NULL,queued INTEGER NOT NULL);")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS native_rebuild_requests(path TEXT PRIMARY KEY);",
        )?;
        crate::index_costs::tables(&conn)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS native_quick_queue(path TEXT PRIMARY KEY);",
        )?;
        anyhow::ensure!(
            conn.query_row("PRAGMA cipher_version", [], |r| r.get::<_, String>(0))
                .is_ok(),
            "SQLCipher support required"
        );
        Ok(db)
    }
    pub fn converted_copy(&self, encrypted: bool) -> Result<Self> {
        let filename = format!(
            "index-{}-{}.sqlite",
            if encrypted { "encrypted" } else { "plain" },
            chrono::Utc::now().timestamp_millis()
        );
        let target = self.home.join(&filename);
        anyhow::ensure!(!target.exists(), "Conversion destination already exists");
        let key = if encrypted {
            Some(read_key(&self.home)?)
        } else {
            None
        };
        let literal = key
            .as_ref()
            .map(|k| {
                format!(
                    "x'{}'",
                    k.iter().map(|x| format!("{x:02x}")).collect::<String>()
                )
            })
            .unwrap_or_default();
        let c = self.connect(false)?;
        cached_execute(
            &c,
            "ATTACH DATABASE ?1 AS converted KEY ?2",
            params![target.to_string_lossy(), literal],
        )?;
        cached_query_row(&c, "SELECT sqlcipher_export('converted')", [], |_| Ok(()))?;
        let original: i64 = cached_query_row(&c, "SELECT count(*) FROM names", [], |r| r.get(0))?;
        let copied: i64 =
            cached_query_row(&c, "SELECT count(*) FROM converted.names", [], |r| r.get(0))?;
        anyhow::ensure!(original == copied, "Encryption copy validation failed");
        c.execute_batch("DETACH DATABASE converted")?;
        drop(c);
        let out = Self {
            home: self.home.clone(),
            filename: filename.clone(),
            key,
            generation: Arc::new(AtomicU64::new(
                self.generation.fetch_add(1, Ordering::Relaxed) + 1,
            )),
            cache_kib: self.cache_kib,
        };
        out.connect(true)?
            .query_row("SELECT count(*) FROM names", [], |r| r.get::<_, i64>(0))?;
        std::fs::write(self.home.join("index.active.tmp"), filename)?;
        std::fs::rename(
            self.home.join("index.active.tmp"),
            self.home.join("index.active"),
        )?;
        Ok(out)
    }
    pub fn connect(&self, read_only: bool) -> Result<Connection> {
        let flags = if read_only {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        };
        let conn = Connection::open_with_flags(self.home.join(&self.filename), flags)?;
        if let Some(key) = &self.key {
            let hex: String = key.iter().map(|x| format!("{x:02x}")).collect();
            conn.execute_batch(&format!("PRAGMA key=\"x'{hex}'\";"))?;
        }
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.set_prepared_statement_cache_capacity(32);
        conn.execute_batch(&format!("PRAGMA cache_size=-{}; PRAGMA mmap_size=0; PRAGMA temp_store=MEMORY; PRAGMA synchronous=NORMAL;",if read_only{self.cache_kib}else{16384}))?;
        if read_only {
            conn.execute_batch("PRAGMA query_only=ON;")?;
        }
        Ok(conn)
    }
    pub fn clear_index(&self) -> Result<String> {
        let mut c = self.connect(false)?;
        c.execute_batch("PRAGMA secure_delete=ON;")?;
        let tx = c.transaction()?;
        for table in [
            "words",
            "segment_vectors",
            "segments",
            "files",
            "names",
            "source_seen",
            "native_content",
            "native_staging",
            "native_jobs",
            "native_changes",
            "native_quick_queue",
            "native_rebuild_requests",
            "native_prune_prefixes",
            "index_costs",
            "index_job_time",
            "metadata",
        ] {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |r| r.get(0),
            )?;
            if exists {
                tx.execute(&format!("DELETE FROM {table}"), [])?;
            }
        }
        tx.execute("INSERT INTO name_words(name_words) VALUES('rebuild')", [])?;
        tx.commit()?;
        self.generation.fetch_add(1, Ordering::SeqCst);
        // Search readers can delay reclamation; logical clearing is already committed.
        let reclamation = c.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA wal_checkpoint(TRUNCATE);",
        );
        Ok(match reclamation {
            Ok(()) => "Entire active index cleared. Automatic indexing is paused; Update index to rebuild when ready.".into(),
            Err(e) => format!("Entire active index cleared; storage reclamation deferred: {e}. Update index to rebuild when ready."),
        })
    }

    /// Upgrade cleanup uses direct indexed path ranges, preserving existing policy cursors.
    pub fn queue_internal_cleanup(&self, s: &Settings) -> Result<bool> {
        let mut c = self.connect(false)?;
        let mut volumes = std::collections::BTreeSet::new();
        for root in &s.roots {
            let b = root.path.as_bytes();
            if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
                volumes.insert(root.path[..2].to_uppercase());
            }
        }
        let signature = format!("{:?}", volumes);
        let previous: Option<String> = c
            .query_row(
                "SELECT value FROM metadata WHERE key='ntfs_internal_cleanup_v1'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if previous.as_deref() == Some(&signature) {
            return Ok(false);
        }
        let tx = c.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS native_prune_prefixes(path TEXT PRIMARY KEY,cursor TEXT NOT NULL DEFAULT '')")?;
        for volume in volumes {
            for name in [
                "$Extend", "$Mft", "$MftMirr", "$LogFile", "$Volume", "$AttrDef", "$Bitmap",
                "$Boot", "$BadClus", "$Secure", "$UpCase",
            ] {
                tx.execute(
                    "INSERT OR IGNORE INTO native_prune_prefixes VALUES(?1,'')",
                    [format!("{volume}\\{name}")],
                )?;
            }
        }
        {
            let mut q=tx.prepare("SELECT path FROM names WHERE base IN ('$extend','$mft','$mftmirr','$logfile','$volume','$attrdef','$bitmap','$boot','$badclus','$secure','$upcase')")?;
            let rows = q.query_map([], |r| r.get::<_, String>(0))?;
            for path in rows {
                let path = path?;
                if crate::config::ntfs_internal_path(&path) {
                    tx.execute(
                        "INSERT OR IGNORE INTO native_prune_prefixes VALUES(?1,'')",
                        [path],
                    )?;
                }
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO metadata VALUES('ntfs_internal_cleanup_v1',?1)",
            [signature],
        )?;
        tx.execute(
            "DELETE FROM metadata WHERE key='pruned_source_signature'",
            [],
        )?;
        tx.commit()?;
        Ok(true)
    }
    /// Bounded metadata-only cleanup; current search policy already hides removed sources.
    pub fn prune_step(&self, s: &Settings, signature: &str) -> Result<bool> {
        let mut c = self.connect(false)?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS native_prune_prefixes(path TEXT PRIMARY KEY,cursor TEXT NOT NULL DEFAULT '')")?;
        let active = cached_query_row(
            &c,
            "SELECT value FROM metadata WHERE key='prune_target'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
        if active.as_deref() != Some(signature) {
            // Discover excluded folder prefixes through names_base once. Drain
            // them through path-index ranges instead of walking millions of
            // unrelated names before reaching a Windows/cache subtree.
            let tx = c.transaction()?;
            let old_prefixes={let mut q=tx.prepare("SELECT path FROM native_prune_prefixes")?;q.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?};
            for path in old_prefixes {if !crate::config::ntfs_internal_path(&path){tx.execute("DELETE FROM native_prune_prefixes WHERE path=?1",[path])?;}}
            if s.skip_windows_churn {
                let policy = crate::config::SearchPolicy::new(s, "All files", "", "");
                let mut q = tx.prepare("SELECT path FROM names WHERE base IN ('windows','appdata','temp','cache','caches') AND kind='folder'")?;
                let paths = q.query_map([], |r| r.get::<_, String>(0))?;
                for path in paths {
                    let path = path?;
                    if !policy.allowed(&path, "folder") {
                        tx.execute("INSERT OR IGNORE INTO native_prune_prefixes VALUES(?1,'')", [&path])?;
                    }
                }
            }
            tx.commit()?;
            cached_execute(
                &c,
                "INSERT OR REPLACE INTO metadata VALUES('prune_target',?1)",
                [signature],
            )?;
            cached_execute(
                &c,
                "INSERT OR REPLACE INTO metadata VALUES('prune_cursor','')",
                [],
            )?;
        }
        let started = std::time::Instant::now();
        let policy = crate::config::SearchPolicy::new(s, "All files", "", "");
        loop {
            let prefix: Option<(String, String)> = c.query_row("SELECT path,cursor FROM native_prune_prefixes ORDER BY path LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
            let Some((prefix, cursor)) = prefix else { break; };
            let lower = format!("{}\\", prefix.trim_end_matches(['\\', '/']));
            let upper = format!("{lower}{}", char::MAX);
            let paths: Vec<String> = c.prepare_cached("SELECT path FROM names WHERE path>=?1 AND path<?3 AND (path=?1 OR path>=?2) AND path>?4 ORDER BY path LIMIT 128")?
                .query_map(params![prefix, lower, upper, cursor], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            if paths.is_empty() {
                c.execute("DELETE FROM native_prune_prefixes WHERE path=?1", [&prefix])?;
            } else {
                let last = paths.last().unwrap();
                let removed: Vec<String> = paths.iter().filter(|p| !policy.allowed(p, "file")).cloned().collect();
                let tx = c.transaction()?;
                Self::delete_batch(&tx, &removed)?;
                tx.execute("UPDATE native_prune_prefixes SET cursor=?2 WHERE path=?1", params![prefix, last])?;
                tx.commit()?;
            }
            if started.elapsed() >= Duration::from_millis(25) { return Ok(false); }
        }
        let cursor = cached_query_row(
            &c,
            "SELECT value FROM metadata WHERE key='prune_cursor'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_default();
        let paths = c
            .prepare_cached("SELECT path FROM names WHERE path>?1 ORDER BY path LIMIT 1024")?
            .query_map([cursor], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if paths.is_empty() {
            cached_execute(
                &c,
                "INSERT OR REPLACE INTO metadata VALUES('pruned_source_signature',?1)",
                [signature],
            )?;
            return Ok(true);
        }
        let policy = crate::config::SearchPolicy::new(s, "All files", "", "");
        let tx = c.transaction()?;
        let started = std::time::Instant::now();
        let mut cursor = String::new();
        let mut removed = vec![];
        for path in paths {
            if !policy.allowed(&path, "file") {
                removed.push(path.clone());
            }
            cursor = path;
            if started.elapsed() > Duration::from_millis(25) {
                break;
            }
        }
        Self::delete_batch(&tx, &removed)?;
        tx.execute(
            "INSERT OR REPLACE INTO metadata VALUES('prune_cursor',?1)",
            [cursor],
        )?;
        tx.commit()?;
        Ok(false)
    }
    pub fn stats(&self) -> Result<Stats> {
        let c = self.connect(true)?;
        let mut s = Stats::default();
        s.files = cached_query_row(
            &c,
            "SELECT count(*) FROM names WHERE kind!='folder'",
            [],
            |r| r.get(0),
        )?;
        s.full = cached_query_row(
            &c,
            "SELECT count(DISTINCT v.path) FROM segment_vectors v JOIN names n ON n.path=v.path WHERE n.kind!='folder' AND n.index_mode='full'",
            [],
            |r| r.get(0),
        )?;
        s.quick = s.files.saturating_sub(s.full);
        s.folders = cached_query_row(
            &c,
            "SELECT count(*) FROM names WHERE kind='folder'",
            [],
            |r| r.get(0),
        )?;
        s.vectors = cached_query_row(&c, "SELECT count(*) FROM segment_vectors", [], |r| r.get(0))?;
        Ok(s)
    }
    pub fn source_stats(&self, root: &str) -> Result<SourceStats> {
        let c = self.connect(true)?;
        let normalized = root.replace('/', "\\");
        let root = normalized.as_str();
        let p = root.trim_end_matches(['\\', '/']);
        let upper = format!("{p}]");
        let lower = format!("{p}\\");
        let files = cached_query_row(
            &c,
            "SELECT count(*) FROM names WHERE kind!='folder' AND (path=?1 OR (path>=?2 AND path<?3))",
            params![root, lower, upper],
            |r| r.get(0),
        )?;
        let added = cached_query_row(
            &c,
            "SELECT count(*) FROM source_seen s JOIN names n ON n.path=s.path WHERE n.kind!='folder' AND added>=?1 AND (s.path=?2 OR (s.path>=?3 AND s.path<?4))",
            params![chrono::Utc::now().timestamp() - 3600, root, lower, upper],
            |r| r.get(0),
        )?;
        Ok(SourceStats {
            files,
            added_hour: added,
        })
    }
    pub fn touch_name(
        c: &Connection,
        path: &Path,
        kind: &str,
        mtime: i64,
        size: u64,
        mode: &str,
        root: &str,
        epoch: i64,
    ) -> Result<bool> {
        Ok(Self::touch_name_detail(c, path, kind, mtime, size, mode, root, epoch)? != "unchanged")
    }
    pub fn touch_name_detail(
        c: &Connection,
        path: &Path,
        kind: &str,
        mtime: i64,
        size: u64,
        mode: &str,
        root: &str,
        epoch: i64,
    ) -> Result<&'static str> {
        let path = path.to_string_lossy().into_owned();
        let name = Path::new(&path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let parent = Path::new(&path)
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .into_owned();
        let folded = path.to_lowercase();
        let old: Option<(i64, String, i64, i64, String)> = c
            .query_row(
                "SELECT rowid,folded,mtime,size,index_mode FROM names WHERE path=?1",
                [&path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        let added = if old.is_some() {
            0
        } else {
            chrono::Utc::now().timestamp()
        };
        let changed = old.as_ref().is_none_or(|o| {
            o.2 != mtime
                || u64::try_from(o.3).ok() != Some(size)
                || (o.4 != mode
                    && !(matches!(o.4.as_str(), "names" | "quick")
                        && matches!(mode, "names" | "quick")))
        });
        let reason = if old.is_none() {
            "added"
        } else if changed {
            "metadata/mode changed"
        } else {
            "unchanged"
        };
        if changed {
            if let Some(o) = old {
                let renamed = o.1 != folded;
                if renamed {
                    cached_execute(
                        &c,
                        "INSERT INTO name_words(name_words,rowid,folded) VALUES('delete',?1,?2)",
                        params![o.0, o.1],
                    )?;
                }
                cached_execute(
                    &c,
                    "UPDATE names SET name=?1,parent=?2,kind=?3,folded=?4,base=?5,mtime=?6,size=?7,index_mode=?8 WHERE rowid=?9",
                    params![
                        name,
                        parent,
                        kind,
                        folded,
                        name.to_lowercase(),
                        mtime,
                        size,
                        mode,
                        o.0
                    ],
                )?;
                if renamed {
                    cached_execute(
                        &c,
                        "INSERT INTO name_words(rowid,folded) VALUES(?1,?2)",
                        params![o.0, folded],
                    )?;
                }
            } else {
                cached_execute(
                    &c,
                    "INSERT INTO names VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![
                        path,
                        name,
                        parent,
                        kind,
                        folded,
                        name.to_lowercase(),
                        mtime,
                        size,
                        mode
                    ],
                )?;
                cached_execute(
                    &c,
                    "INSERT INTO name_words(rowid,folded) VALUES(?1,?2)",
                    params![c.last_insert_rowid(), folded],
                )?;
            }
        }
        cached_execute(
            &c,
            "INSERT INTO source_seen VALUES(?1,?2,?3,?4) ON CONFLICT(path) DO UPDATE SET source=excluded.source,epoch=excluded.epoch",
            params![path, root, epoch, added],
        )?;
        Ok(reason)
    }
    /// words.rowid and segment_vectors.id are the owning segments.rowid.
    /// Resolve those IDs through segments_path before deleting the segments.
    fn delete_content(c: &Connection, path: &str) -> Result<()> {
        cached_execute(
            c,
            "DELETE FROM words WHERE rowid IN (SELECT rowid FROM segments WHERE path=?1)",
            [path],
        )?;
        cached_execute(
            c,
            "DELETE FROM segment_vectors WHERE id IN (SELECT rowid FROM segments WHERE path=?1)",
            [path],
        )?;
        cached_execute(c, "DELETE FROM segments WHERE path=?1", [path])?;
        Ok(())
    }
    pub fn delete(c: &Connection, path: &str) -> Result<()> {
        Self::delete_batch(c, &[path.to_owned()])
    }
    /// Atomic, bounded batches avoid per-path FTS/vector table scans. The temp
    /// table belongs only to this connection; no schema migration is required.
    pub fn delete_batch(c: &Connection, paths: &[String]) -> Result<()> {
        for paths in paths.chunks(128) {
            c.execute_batch("CREATE TEMP TABLE IF NOT EXISTS cleanup_paths(path TEXT PRIMARY KEY) WITHOUT ROWID; SAVEPOINT cleanup_batch;")?;
            let result = (|| -> Result<()> {
                c.execute("DELETE FROM cleanup_paths", [])?;
                for path in paths {
                    cached_execute(c, "INSERT OR IGNORE INTO cleanup_paths VALUES(?1)", [path])?;
                }
                c.execute_batch("INSERT INTO name_words(name_words,rowid,folded) SELECT 'delete',rowid,folded FROM names WHERE path IN (SELECT path FROM cleanup_paths);
                    DELETE FROM words WHERE rowid IN (SELECT rowid FROM segments WHERE path IN (SELECT path FROM cleanup_paths));
                    DELETE FROM segment_vectors WHERE id IN (SELECT rowid FROM segments WHERE path IN (SELECT path FROM cleanup_paths));
                    DELETE FROM segments WHERE path IN (SELECT path FROM cleanup_paths);
                    DELETE FROM native_staging WHERE token IN (SELECT token FROM native_jobs WHERE path IN (SELECT path FROM cleanup_paths));")?;
                for table in [
                    "names",
                    "native_jobs",
                    "native_changes",
                    "native_quick_queue",
                    "native_rebuild_requests",
                    "index_job_time",
                    "files",
                    "source_seen",
                    "native_content",
                ] {
                    c.execute(
                        &format!(
                            "DELETE FROM {table} WHERE path IN (SELECT path FROM cleanup_paths)"
                        ),
                        [],
                    )?;
                }
                cached_execute(
                    c,
                    "INSERT INTO metadata VALUES('native_vectors_generation','1') ON CONFLICT(key) DO UPDATE SET value=cast(value AS INTEGER)+1",
                    [],
                )?;
                c.execute("DELETE FROM cleanup_paths", [])?;
                Ok(())
            })();
            if result.is_ok() {
                c.execute_batch("RELEASE cleanup_batch")?;
            } else {
                c.execute_batch("ROLLBACK TO cleanup_batch; RELEASE cleanup_batch")?;
            }
            result?;
        }
        Ok(())
    }
    pub fn save_segments(
        c: &Connection,
        path: &str,
        records: &[crate::content::Segment],
        vectors: &[Vec<f32>],
        fingerprint: &str,
    ) -> Result<()> {
        c.execute_batch("CREATE TABLE IF NOT EXISTS native_jobs(path TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,token TEXT NOT NULL,error TEXT NOT NULL DEFAULT ''); SAVEPOINT file_update")?;
        cached_execute(
            &c,
            "INSERT INTO metadata VALUES('native_vectors_generation','1') ON CONFLICT(key) DO UPDATE SET value=cast(value AS INTEGER)+1",
            [],
        )?;
        let result = (|| -> Result<()> {
            Self::delete_content(c, path)?;
            for (i, segment) in records.iter().enumerate() {
                let v = vectors.get(i).cloned().unwrap_or_default();
                let location=serde_json::json!({"page":segment.page,"seconds":segment.seconds,"line":segment.line,"embedding_source":if segment.media.is_empty(){"text"}else{"media"}}).to_string();
                cached_execute(
                    &c,
                    "INSERT INTO segments VALUES(?1,'files',?2,?3,'[]',?4)",
                    params![path, segment.kind, segment.text, location],
                )?;
                let id = c.last_insert_rowid();
                cached_execute(
                    &c,
                    "INSERT INTO words(rowid,path,text) VALUES(?1,?2,?3)",
                    params![id, path, segment.text],
                )?;
                if !v.is_empty() {
                    let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
                    cached_execute(
                        &c,
                        "INSERT INTO segment_vectors VALUES(?1,'files',?2,?3,?4,?5,?6)",
                        params![
                            id,
                            segment.kind,
                            if segment.media.is_empty() {
                                "text"
                            } else {
                                &segment.kind
                            },
                            v.len(),
                            bytes,
                            path
                        ],
                    )?;
                }
            }
            cached_execute(
                &c,
                "INSERT OR REPLACE INTO native_content VALUES(?1,?2,'')",
                params![path, fingerprint],
            )?;
            Ok(())
        })();
        if result.is_ok() {
            c.execute_batch("RELEASE file_update")?;
        } else {
            c.execute_batch("ROLLBACK TO file_update; RELEASE file_update")?;
        }
        result
    }
    pub fn import(
        &self,
        source: &Path,
        encrypted: bool,
        progress: impl Fn(u64, u64),
    ) -> Result<()> {
        anyhow::ensure!(source != self.home, "Import destination must be different");
        anyhow::ensure!(
            !encrypted || source.join("index.key").is_file(),
            "Source index encryption key is missing; source was not changed"
        );
        let src = Self {
            home: source.into(),
            filename: "index.sqlite".into(),
            key: if encrypted {
                Some(read_key(source)?)
            } else {
                None
            },
            generation: Arc::new(AtomicU64::new(0)),
            cache_kib: 8192,
        };
        let reader = src.connect(true)?;
        let mut writer = self.connect(false)?;
        let backup = rusqlite::backup::Backup::new(&reader, &mut writer)?;
        loop {
            match backup.step(256)? {
                rusqlite::backup::StepResult::Done => break,
                rusqlite::backup::StepResult::More => {
                    let p = backup.progress();
                    progress((p.pagecount - p.remaining) as u64, p.pagecount as u64);
                }
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        drop(backup);
        self.generation.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
/// A bounded connection-local predicate keeps current roots/exclusions before SQL LIMITs.
fn cached_execute<P: rusqlite::Params>(
    c: &Connection,
    sql: &str,
    params: P,
) -> rusqlite::Result<usize> {
    c.prepare_cached(sql)?.execute(params)
}
fn cached_query_row<
    T,
    P: rusqlite::Params,
    F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
>(
    c: &Connection,
    sql: &str,
    params: P,
    map: F,
) -> rusqlite::Result<T> {
    c.prepare_cached(sql)?.query_row(params, map)
}
pub fn register_search(
    c: &Connection,
    s: &Settings,
    category: &str,
    scope: &str,
    excluded: &str,
    filters: Option<&crate::search_filters::Filters>,
) -> Result<()> {
    let policy = crate::config::SearchPolicy::new(s, category, scope, excluded);
    let filters = filters.filter(|f| f.active()).cloned();
    c.create_scalar_function(
        "vd_match",
        4,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            let path = ctx.get_raw(0).as_str()?;
            let kind = ctx.get_raw(1).as_str()?;
            Ok(policy.allowed(path, kind)
                && filters.as_ref().is_none_or(|f| {
                    f.matches(
                        path.rsplit(['\\', '/']).next().unwrap_or(""),
                        ctx.get::<i64>(3).unwrap_or(-1),
                        ctx.get::<i64>(2).unwrap_or(-1),
                    )
                }))
        },
    )?;
    Ok(())
}

pub fn names(
    db: &Database,
    query: &str,
    s: &Settings,
    category: &str,
    scope: &str,
    excluded: &str,
) -> Result<Vec<Hit>> {
    let filters = crate::search_filters::Filters::parse(query)?;
    if let Some(path_query) = path_query(&filters.text) {
        return path_names(db, &path_query, &filters, s, category, scope, excluded);
    }
    if let Some(pattern) = filters
        .pattern
        .as_deref()
        .filter(|_| filters.text.is_empty())
    {
        return path_names(db, pattern, &filters, s, category, scope, excluded);
    }
    if filters.active() {
        return filtered_names(db, &filters, s, category, scope, excluded);
    }
    let c = db.connect(true)?;
    register_search(&c, s, category, scope, excluded, None)?;
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    let low = q.clone();
    let upper = format!("{q}\u{10ffff}");
    let scope_low = crate::config::fold(scope);
    let scope_prefix = format!("{scope_low}\\");
    let scope_upper = format!("{scope_prefix}\u{10ffff}");
    let mut prefix = c.prepare_cached("SELECT path,name,kind FROM names WHERE base>=?1 AND base<?2 AND (?3='' OR folded=?3 OR (folded>=?4 AND folded<?5)) AND vd_match(path,kind,mtime,size) LIMIT 400")?;
    for row in prefix.query_map(
        params![low, upper, scope_low, scope_prefix, scope_upper],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        },
    )? {
        add_name(&mut result, row?, &q, s, category, scope, excluded);
    }
    if q.chars().count() >= 3 {
        let mut terms = q
            .split_whitespace()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        if !scope.is_empty() {
            terms.push_str(&format!(" AND \"{}\"", scope_prefix.replace('"', "\"\"")));
        }
        let mut stmt=c.prepare_cached("SELECT n.path,n.name,n.kind FROM name_words f JOIN names n ON n.rowid=f.rowid WHERE name_words MATCH ?1 AND vd_match(n.path,n.kind,n.mtime,n.size) LIMIT 2000")?;
        for row in stmt.query_map([terms], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            add_name(&mut result, row?, &q, s, category, scope, excluded);
        }
    }
    result.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.path.len().cmp(&b.path.len()))
    });
    result.dedup_by(|a, b| a.path == b.path);
    result.truncate(100);
    Ok(result)
}
/// Explicit path fragments constrain the ancestor folders and final name.
/// Leading separators mean "in any parent folder", rather than a drive root.
pub fn path_query(query: &str) -> Option<String> {
    let query = query.trim().trim_matches('"');
    if !query.contains(['\\', '/']) || query.contains("://") {
        return None;
    }
    let normalized = query.replace('/', "\\").to_lowercase();
    let parts: Vec<_> = normalized.split('\\').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("\\"))
}

fn path_fragment_matches(path: &str, query: &str) -> bool {
    let path = crate::config::fold(path);
    let (parent, name) = path.rsplit_once('\\').unwrap_or(("", &path));
    let (folders, last) = query.rsplit_once('\\').unwrap_or(("", query));
    let name_matches = if last.contains(['*', '?']) {
        crate::config::wildcard_match(name, last)
    } else {
        name.starts_with(last)
    };
    if !name_matches || folders.is_empty() {
        return name_matches;
    }
    let pattern = format!("*\\{folders}");
    let mut ancestor = parent;
    loop {
        if crate::config::wildcard_match(&format!("\\{ancestor}"), &pattern) {
            return true;
        }
        let Some((next, _)) = ancestor.rsplit_once('\\') else {
            return false;
        };
        ancestor = next;
    }
}

fn path_names(
    db: &Database,
    query: &str,
    filters: &crate::search_filters::Filters,
    s: &Settings,
    category: &str,
    scope: &str,
    excluded: &str,
) -> Result<Vec<Hit>> {
    let c = db.connect(true)?;
    register_search(&c, s, category, scope, excluded, Some(filters))?;
    let query_owned = query.to_owned();
    c.create_scalar_function(
        "vd_path_match",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            Ok(path_fragment_matches(
                ctx.get_raw(0).as_str()?,
                &query_owned,
            ))
        },
    )?;
    // Descendants may separate the parent fragment and final filename, so use
    // indexed literal terms rather than requiring one contiguous FTS phrase.
    let mut terms = query
        .split(['\\', '*', '?'])
        .filter(|s| s.chars().count() >= 3)
        .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    // Explorer-scoped short wildcards such as *nr* cannot use name trigrams,
    // but their folder scope can. Restrict candidates in the existing index.
    let scope_folded = crate::config::fold(scope);
    if scope_folded.chars().count() >= 3 {
        if !terms.is_empty() {
            terms.push_str(" AND ");
        }
        terms.push_str(&format!("\"{}\"", scope_folded.replace('"', "\"\"")));
    }
    let use_fts = !terms.is_empty();
    let sql = format!(
        "SELECT n.path,n.name,n.kind FROM names n WHERE {} vd_path_match(n.path) AND vd_match(n.path,n.kind,n.mtime,n.size) ORDER BY length(n.path),n.path LIMIT 400",
        if use_fts {
            "n.rowid IN (SELECT rowid FROM name_words WHERE name_words MATCH ?1) AND"
        } else {
            ""
        }
    );
    let mut stmt = c.prepare_cached(&sql)?;
    let values = if use_fts { vec![terms] } else { vec![] };
    let mut result = vec![];
    for row in stmt.query_map(rusqlite::params_from_iter(values), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (path, name, kind) = row?;
        let exact = crate::config::fold(&path).ends_with(query);
        add_name(
            &mut result,
            (path, name, kind),
            query,
            s,
            category,
            scope,
            excluded,
        );
        if let Some(hit) = result.last_mut() {
            hit.score = if exact { 2.0 } else { 1.5 };
            hit.reason = if query.contains('\\') {
                "Matches the folder path"
            } else {
                "Matches the file or folder name"
            }
            .into();
        }
    }
    result.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.path.len().cmp(&b.path.len()))
            .then(a.path.cmp(&b.path))
    });
    result.truncate(100);
    Ok(result)
}

fn add_name(
    out: &mut Vec<Hit>,
    (path, name, kind): (String, String, String),
    q: &str,
    s: &Settings,
    category: &str,
    scope: &str,
    excluded: &str,
) {
    if !allowed(&path, &kind, s, category, scope, excluded) {
        return;
    }
    let low = name.to_lowercase();
    let page = if path.to_lowercase().ends_with(".pdf") {
        1
    } else {
        0
    };
    out.push(Hit {
        path,
        kind,
        score: if low == q {
            2.0
        } else if low.starts_with(q) {
            1.5
        } else if low.contains(q) {
            1.2
        } else {
            0.5
        },
        text: String::new(),
        reason: if low.contains(q) {
            "Matches the file or folder name"
        } else {
            "Matches the folder path"
        }
        .into(),
        page,
        seconds: -1.0,
        line: 0,
    });
}
pub fn allowed(
    path: &str,
    kind: &str,
    s: &Settings,
    category: &str,
    scope: &str,
    excluded: &str,
) -> bool {
    let p = Path::new(path);
    if !s.roots.is_empty()
        && !["file", "folder"].contains(&kind)
        && crate::config::mode(path, s) != "full"
    {
        return false;
    }
    if !s.searched(p) || !scope.is_empty() && !crate::config::under(path, scope) {
        return false;
    }
    let ext = p
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if crate::config::excluded_type(p, excluded) {
        return false;
    }
    match category {
        "Images" => crate::content::is_image(&ext),
        "Videos" => crate::content::is_video(&ext),
        "Documents" => [
            "txt", "md", "pdf", "docx", "xlsx", "pptx", "csv", "log", "html", "htm", "rtf",
        ]
        .contains(&ext.as_str()),
        "Audio / video" => crate::content::is_audio(&ext) || crate::content::is_video(&ext),
        "Folders" => kind == "folder",
        "Code" => crate::content::is_code(&ext),
        _ => true,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Source;
    #[test]
    fn encrypted_and_plain_catalogs() {
        for encryption in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let s = Settings {
                roots: vec![crate::config::Source {
                    path: r"C:\".into(),
                    mode: "full".into(),
                }],
                encrypt_index: encryption,
                ..Default::default()
            };
            let db = Database::new(dir.path(), &s).unwrap();
            let c = db.connect(false).unwrap();
            c.execute_batch("BEGIN").unwrap();
            Database::touch_name(
                &c,
                Path::new("C:\\data\\ForzaHorizon6.exe"),
                "file",
                10,
                20,
                "quick",
                "C:\\data",
                1,
            )
            .unwrap();
            c.execute_batch("COMMIT").unwrap();
            let hits = names(&db, "forza", &s, "All files", "", "").unwrap();
            assert_eq!(hits.len(), 1);
            assert!(
                names(&db, "forza", &s, "Images", "", "")
                    .unwrap()
                    .is_empty()
            );
            assert!(
                names(&db, "forza", &s, "All files", "", "exe")
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(db.stats().unwrap().quick, 1);
            drop(c);
            let bytes = std::fs::read(dir.path().join("index.sqlite")).unwrap();
            assert_eq!(bytes.starts_with(b"SQLite format 3"), !encryption);
        }
    }
    #[test]
    fn conversion_preserves_original_and_activates_copy() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings {
            roots: vec![crate::config::Source {
                path: r"C:\".into(),
                mode: "full".into(),
            }],
            encrypt_index: false,
            ..Default::default()
        };
        let db = Database::new(dir.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        Database::touch_name(
            &c,
            Path::new("C:\\data\\note.txt"),
            "file",
            1,
            1,
            "quick",
            "C:\\data",
            1,
        )
        .unwrap();
        drop(c);
        let encrypted = db.converted_copy(true).unwrap();
        assert!(dir.path().join("index.sqlite").exists());
        assert!(encrypted.key.is_some());
        assert_eq!(
            names(&encrypted, "note", &s, "All files", "", "")
                .unwrap()
                .len(),
            1
        );
        let reopened = Database::new(
            dir.path(),
            &Settings {
                encrypt_index: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(reopened.filename, encrypted.filename);
        let plain = encrypted.converted_copy(false).unwrap();
        assert!(plain.key.is_none());
        assert_eq!(plain.stats().unwrap().files, 1);
        assert!(dir.path().join(encrypted.filename).exists());
    }
    #[test]
    fn existing_paths_are_not_new_additions() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings {
            encrypt_index: false,
            ..Default::default()
        };
        let db = Database::new(dir.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        let path = Path::new("C:\\data\\note.txt");
        assert!(Database::touch_name(&c, path, "file", 1, 1, "quick", "C:\\data", 1).unwrap());
        assert!(!Database::touch_name(&c, path, "file", 1, 1, "quick", "C:\\data", 2).unwrap());
        let added: i64 = c
            .query_row(
                "SELECT added FROM source_seen WHERE path=?1",
                [path.to_str().unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        assert!(added > 0);
        assert_eq!(db.source_stats("C:\\data").unwrap().files, 1);
    }
    #[test]
    fn exclusions_and_overlapping_sources() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings {
            encrypt_index: false,
            roots: vec![
                Source {
                    path: "C:\\".into(),
                    mode: "quick".into(),
                },
                Source {
                    path: "C:\\documents".into(),
                    mode: "full".into(),
                },
            ],
            exclusions: vec!["C:\\private".into()],
            ..Default::default()
        };
        assert!(!allowed(
            "C:\\private\\note.txt",
            "text",
            &s,
            "All files",
            "",
            ""
        ));
        assert!(allowed(
            "C:\\documents\\note.txt",
            "text",
            &s,
            "Documents",
            "C:\\documents",
            ""
        ));
        assert!(!allowed(
            "C:\\other\\note.txt",
            "text",
            &s,
            "Documents",
            "C:\\documents",
            ""
        ));
    }
}
impl Database {
    /// Remove an obsolete queued attempt without touching completed embeddings.
    pub fn discard_job(c: &Connection, path: &str) -> Result<()> {
        c.execute_batch("SAVEPOINT discard_job")?;
        let result = (|| -> Result<()> {
            cached_execute(
                c,
                "DELETE FROM native_staging WHERE token IN (SELECT token FROM native_jobs WHERE path=?1)",
                [path],
            )?;
            cached_execute(c, "DELETE FROM native_jobs WHERE path=?1", [path])?;
            cached_execute(c, "DELETE FROM index_job_time WHERE path=?1", [path])?;
            Ok(())
        })();
        if result.is_ok() {
            c.execute_batch("RELEASE discard_job")?;
        } else {
            c.execute_batch("ROLLBACK TO discard_job; RELEASE discard_job")?;
        }
        result
    }
    pub fn prepare_job(c: &Connection, path: &str, fingerprint: &str) -> Result<(String, usize)> {
        c.execute_batch("CREATE TABLE IF NOT EXISTS native_jobs(path TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,token TEXT NOT NULL,error TEXT NOT NULL DEFAULT ''); CREATE TABLE IF NOT EXISTS native_staging(token TEXT,seq INTEGER,kind TEXT,text TEXT,location TEXT,dimension INTEGER,vector BLOB,PRIMARY KEY(token,seq));")?;
        let old = c
            .query_row(
                "SELECT fingerprint,token FROM native_jobs WHERE path=?1",
                [path],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        if let Some((stamp, token)) = &old {
            if stamp == fingerprint
                || crate::engine::content_equivalent(
                    stamp,
                    fingerprint,
                    &crate::content::extension(Path::new(path)),
                )
            {
                cached_execute(
                    &c,
                    "UPDATE native_jobs SET fingerprint=?2,error='' WHERE path=?1",
                    params![path, fingerprint],
                )?;
                let next = cached_query_row(
                    &c,
                    "SELECT coalesce(max(seq)+1,0) FROM native_staging WHERE token=?1",
                    [token],
                    |r| r.get(0),
                )?;
                return Ok((token.clone(), next));
            }
        }
        let token = crate::config::random_key();
        c.execute_batch("SAVEPOINT prepare_job")?;
        let result = (|| -> Result<()> {
            if let Some((_, old)) = old {
                cached_execute(&c, "DELETE FROM native_staging WHERE token=?1", [old])?;
            }
            cached_execute(&c, "DELETE FROM index_job_time WHERE path=?1", [path])?;
            cached_execute(
                &c,
                "INSERT OR REPLACE INTO native_jobs(path,fingerprint,token,error) VALUES(?1,?2,?3,'')",
                params![path, fingerprint, token],
            )?;
            Ok(())
        })();
        if result.is_ok() {
            c.execute_batch("RELEASE prepare_job")?;
        } else {
            c.execute_batch("ROLLBACK TO prepare_job; RELEASE prepare_job")?;
        }
        result?;
        Ok((token, 0))
    }
    pub fn begin_session(c: &Connection) -> Result<()> {
        c.execute_batch("CREATE TABLE IF NOT EXISTS native_jobs(path TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,token TEXT NOT NULL,error TEXT NOT NULL DEFAULT '');")?;
        if cached_query_row(
            &c,
            "SELECT count(*) FROM native_jobs WHERE error=''",
            [],
            |r| r.get::<_, usize>(0),
        )? == 0
        {
            cached_execute(
                &c,
                "INSERT OR REPLACE INTO metadata VALUES('index_session_completed','0')",
                [],
            )?;
        }
        Ok(())
    }
    pub fn session(c: &Connection) -> Result<(u64, u64)> {
        let complete = c
            .query_row(
                "SELECT cast(value AS INTEGER) FROM metadata WHERE key='index_session_completed'",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let pending = cached_query_row(
            &c,
            "SELECT count(*) FROM native_jobs WHERE error=''",
            [],
            |r| r.get(0),
        )?;
        Ok((complete, pending))
    }
    pub fn stage(
        c: &Connection,
        token: &str,
        offset: usize,
        records: &[crate::content::Segment],
        vectors: &[Vec<f32>],
    ) -> Result<()> {
        c.execute_batch("CREATE TABLE IF NOT EXISTS native_staging(token TEXT,seq INTEGER,kind TEXT,text TEXT,location TEXT,dimension INTEGER,vector BLOB,PRIMARY KEY(token,seq)); BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            let previous: Option<usize> = c
                .query_row(
                    "SELECT dimension FROM native_staging WHERE token=?1 AND dimension>0 LIMIT 1",
                    [token],
                    |r| r.get(0),
                )
                .optional()?;
            let expected =
                previous.or_else(|| vectors.iter().find(|v| !v.is_empty()).map(Vec::len));
            anyhow::ensure!(
                vectors
                    .iter()
                    .filter(|v| !v.is_empty())
                    .all(|v| Some(v.len()) == expected),
                "Embedding dimensions differ from saved chunks. Select a compatible model or explicitly rebuild full content."
            );
            for (i, record) in records.iter().enumerate() {
                let vector = vectors.get(i).cloned().unwrap_or_default();
                let bytes: Vec<u8> = vector.iter().flat_map(|v| v.to_le_bytes()).collect();
                let location=serde_json::json!({"page":record.page,"seconds":record.seconds,"line":record.line,"embedding_source":if record.media.is_empty(){"text"}else{"media"}}).to_string();
                cached_execute(
                    &c,
                    "INSERT OR REPLACE INTO native_staging VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![
                        token,
                        offset + i,
                        record.kind,
                        record.text,
                        location,
                        vector.len(),
                        bytes
                    ],
                )?;
            }
            Ok(())
        })();
        if result.is_ok() {
            c.execute_batch("COMMIT")?;
        } else {
            c.execute_batch("ROLLBACK")?;
        }
        result
    }
    pub fn finish_stage(c: &Connection, token: &str, path: &str, fingerprint: &str) -> Result<()> {
        c.execute_batch("CREATE TABLE IF NOT EXISTS native_jobs(path TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,token TEXT NOT NULL,error TEXT NOT NULL DEFAULT ''); SAVEPOINT file_update")?;
        let result = (|| -> Result<()> {
            Self::delete_content(c, path)?;
            let mut stmt=c.prepare_cached("SELECT kind,text,location,dimension,vector FROM native_staging WHERE token=?1 ORDER BY seq")?;
            let mut rows = stmt.query([token])?;
            while let Some(row) = rows.next()? {
                let kind: String = row.get(0)?;
                let text: String = row.get(1)?;
                let location: String = row.get(2)?;
                let dim: usize = row.get(3)?;
                let bytes: Vec<u8> = row.get(4)?;
                cached_execute(
                    &c,
                    "INSERT INTO segments VALUES(?1,'files',?2,?3,'[]',?4)",
                    params![path, kind, text, location],
                )?;
                let id = c.last_insert_rowid();
                cached_execute(
                    &c,
                    "INSERT INTO words(rowid,path,text) VALUES(?1,?2,?3)",
                    params![id, path, text],
                )?;
                if dim > 0 {
                    let v: serde_json::Value = serde_json::from_str(&location)?;
                    cached_execute(
                        &c,
                        "INSERT INTO segment_vectors VALUES(?1,'files',?2,?3,?4,?5,?6)",
                        params![
                            id,
                            kind,
                            if v["embedding_source"] == "media" {
                                &kind
                            } else {
                                "text"
                            },
                            dim,
                            bytes,
                            path
                        ],
                    )?;
                }
            }
            cached_execute(
                &c,
                "INSERT OR REPLACE INTO native_content VALUES(?1,?2,'')",
                params![path, fingerprint],
            )?;
            cached_execute(&c, "DELETE FROM native_staging WHERE token=?1", [token])?;
            cached_execute(
                &c,
                "DELETE FROM native_jobs WHERE path=?1 AND token=?2",
                params![path, token],
            )?;
            cached_execute(
                c,
                "DELETE FROM native_rebuild_requests WHERE path=?1",
                [path],
            )?;
            cached_execute(
                &c,
                "INSERT INTO metadata VALUES('index_session_completed','1') ON CONFLICT(key) DO UPDATE SET value=cast(value AS INTEGER)+1",
                [],
            )?;
            cached_execute(
                &c,
                "INSERT INTO metadata VALUES('native_vectors_generation','1') ON CONFLICT(key) DO UPDATE SET value=cast(value AS INTEGER)+1",
                [],
            )?;
            Ok(())
        })();
        if result.is_ok() {
            c.execute_batch("RELEASE file_update")?;
        } else {
            c.execute_batch("ROLLBACK TO file_update; RELEASE file_update")?;
        }
        result
    }
}
#[cfg(test)]
mod stage_tests {
    use super::*;
    #[test]
    fn legacy_negative_size_refreshes_without_failure() {
        let home = tempfile::tempdir().unwrap();
        let db = Database::new(
            home.path(),
            &Settings {
                encrypt_index: false,
                ..Default::default()
            },
        )
        .unwrap();
        let c = db.connect(false).unwrap();
        let p = Path::new("C:\u{005c}old.txt");
        Database::touch_name(&c, p, "file", 1, 0, "quick", "C:", 1).unwrap();
        cached_execute(&c, "UPDATE names SET size=-1", []).unwrap();
        assert!(Database::touch_name(&c, p, "file", 1, 0, "quick", "C:", 2).unwrap());
        assert!(!Database::touch_name(&c, p, "file", 1, 0, "quick", "C:", 3).unwrap());
    }
    #[test]
    fn staging_does_not_publish_partial_file() {
        let home = tempfile::tempdir().unwrap();
        let db = Database::new(
            home.path(),
            &Settings {
                encrypt_index: false,
                ..Default::default()
            },
        )
        .unwrap();
        let c = db.connect(false).unwrap();
        let old = crate::content::Segment::text("text", "old completed text".into(), 0, 1);
        Database::save_segments(&c, "C:\\note.txt", &[old], &[vec![1.0, 0.0]], "old").unwrap();
        let new = crate::content::Segment::text("text", "new complete text".into(), 0, 1);
        Database::stage(&c, "stage", 0, &[new], &[vec![0.0, 1.0]]).unwrap();
        let before: String = c
            .query_row("SELECT text FROM segments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, "old completed text");
        Database::finish_stage(&c, "stage", "C:\\note.txt", "new").unwrap();
        let after: String = c
            .query_row("SELECT text FROM segments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(after, "new complete text");
        assert_eq!(
            cached_query_row(&c, "SELECT count(*) FROM native_staging", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
            0
        );
    }
    #[test]
    fn imported_quick_rows_are_not_reindexed() {
        let home = tempfile::tempdir().unwrap();
        let db = Database::new(
            home.path(),
            &Settings {
                encrypt_index: false,
                ..Default::default()
            },
        )
        .unwrap();
        let c = db.connect(false).unwrap();
        Database::touch_name(
            &c,
            Path::new("C:\\data\\file.bin"),
            "file",
            10,
            20,
            "names",
            "C:\\data",
            1,
        )
        .unwrap();
        assert!(
            !Database::touch_name(
                &c,
                Path::new("C:\\data\\file.bin"),
                "file",
                10,
                20,
                "quick",
                "C:\\data",
                2
            )
            .unwrap()
        );
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    #[test]
    fn folder_scope_filters_before_result_limits_and_accepts_patterns() {
        let home = tempfile::tempdir().unwrap();
        let s = Settings {
            roots: vec![crate::config::Source {
                path: r"C:\".into(),
                mode: "full".into(),
            }],
            encrypt_index: false,
            ..Default::default()
        };
        let db = Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        for i in 0..500 {
            Database::touch_name(
                &c,
                Path::new(&format!("C:\\outside\\report{i:04}.log")),
                "file",
                1,
                1,
                "quick",
                "C:",
                1,
            )
            .unwrap();
        }
        Database::touch_name(
            &c,
            Path::new("C:\\selected\\nested\\report-inside.log"),
            "file",
            1,
            1,
            "quick",
            "C:",
            1,
        )
        .unwrap();
        let hits = names(&db, "report", &s, "All files", "C:\\selected", "").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("report-inside.log"));
        assert!(
            names(&db, "report", &s, "All files", "C:\\selected", "*.log")
                .unwrap()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod metadata_update_tests {
    use super::*;
    #[test]
    fn content_metadata_changes_do_not_rewrite_filename_fts() {
        let home = tempfile::tempdir().unwrap();
        let s = Settings {
            roots: vec![crate::config::Source {
                path: r"C:\".into(),
                mode: "full".into(),
            }],
            encrypt_index: false,
            ..Default::default()
        };
        let db = Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        let path = Path::new("C:\\report.txt");
        Database::touch_name(&c, path, "file", 1, 42, "quick", "C:", 1).unwrap();
        let before = c.total_changes();
        assert!(Database::touch_name(&c, path, "file", 2, 43, "full", "C:", 2).unwrap());
        assert_eq!(c.total_changes() - before, 2);
        assert_eq!(
            names(&db, "report", &s, "All files", "", "").unwrap().len(),
            1
        );
    }
}

fn filtered_names(
    db: &Database,
    f: &crate::search_filters::Filters,
    s: &Settings,
    category: &str,
    scope: &str,
    excluded: &str,
) -> Result<Vec<Hit>> {
    let c = db.connect(true)?;
    register_search(&c, s, category, scope, excluded, Some(f))?;
    let pattern = f.pattern.as_deref().unwrap_or("*");
    let literal = pattern.split(['*', '?', '[']).next().unwrap_or("");
    let upper = format!("{literal}\u{10ffff}");
    let low = crate::config::fold(scope);
    let prefix = format!("{low}\\");
    let scope_upper = format!("{prefix}\u{10ffff}");
    let ext = serde_json::to_string(
        &f.extension
            .as_deref()
            .unwrap_or("")
            .split(',')
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>(),
    )?;
    let text = f.text.to_lowercase();
    let terms = text
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    let use_fts = text.chars().count() >= 3;
    let sql = format!(
        "SELECT n.path,n.name,n.kind FROM names n WHERE n.base GLOB ?1 AND (?2='' OR (n.base>=?2 AND n.base<?3)) AND n.size>=?4 AND n.size<=?5 AND n.mtime>=?6 AND n.mtime<=?7 AND (?8='[]' OR EXISTS(SELECT 1 FROM json_each(?8) e WHERE n.base GLOB ('*.'||e.value))) AND (?9='' OR n.folded=?9 OR (n.folded>=?10 AND n.folded<?11)) {} AND vd_match(n.path,n.kind,n.mtime,n.size) LIMIT 1000",
        if text.is_empty() {
            ""
        } else if use_fts {
            "AND n.rowid IN (SELECT rowid FROM name_words WHERE name_words MATCH ?12)"
        } else {
            "AND n.base>=?12 AND n.base<?13"
        }
    );
    let mut stmt = c.prepare_cached(&sql)?;
    let mut result = vec![];
    let mut values: Vec<rusqlite::types::Value> = vec![
        pattern.to_owned().into(),
        literal.to_owned().into(),
        upper.into(),
        f.min_size.into(),
        f.max_size.into(),
        f.after.into(),
        f.before.into(),
        ext.into(),
        low.into(),
        prefix.into(),
        scope_upper.into(),
    ];
    if !text.is_empty() {
        if use_fts {
            values.push(terms.into());
        } else {
            values.push(text.clone().into());
            values.push(format!("{text}\u{10ffff}").into());
        }
    }
    for row in stmt.query_map(rusqlite::params_from_iter(values), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        add_name(
            &mut result,
            row?,
            if text.is_empty() { literal } else { &text },
            s,
            category,
            scope,
            excluded,
        );
    }
    result.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.path.cmp(&b.path)));
    result.truncate(100);
    Ok(result)
}

#[cfg(test)]
mod advanced_filter_tests {
    use super::*;
    #[test]
    fn filters_are_applied_before_result_limit() {
        let home = tempfile::tempdir().unwrap();
        let mut s = crate::config::Settings::default();
        s.encrypt_index = false;
        s.roots = vec![crate::config::Source {
            path: r"C:\".into(),
            mode: "full".into(),
        }];
        let db = Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        for i in 0..1100 {
            Database::touch_name(
                &c,
                Path::new(&format!("C:\\data\\dog{i}.log")),
                "file",
                1,
                10,
                "quick",
                "C:\\data",
                1,
            )
            .unwrap();
        }
        Database::touch_name(
            &c,
            Path::new("C:\\data\\dog_final.pdf"),
            "file",
            1700000000000000000,
            4096,
            "quick",
            "C:\\data",
            1,
        )
        .unwrap();
        let hits = names(&db, "dog*.pdf size:>1KB", &s, "All files", "", "").unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("dog_final.pdf"));
        assert!(
            names(&db, "ext:pdf size:<1KB", &s, "All files", "", "")
                .unwrap()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod clear_index_tests {
    use super::*;
    #[test]
    fn clears_all_catalog_content_and_vectors_but_keeps_files_settings_and_key() {
        for encrypted in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let settings = Settings {
                encrypt_index: encrypted,
                ..Default::default()
            };
            settings.save(home.path()).unwrap();
            let original = home.path().join("original.txt");
            std::fs::write(&original, "original file").unwrap();
            let db = Database::new(home.path(), &settings).unwrap();
            let saved = std::fs::read(home.path().join("settings.json")).unwrap();
            let key = std::fs::read(home.path().join("index.key")).ok();
            let c = db.connect(false).unwrap();
            cached_execute(&c,"INSERT INTO names(path,name,parent,kind,folded,base) VALUES('junk.txt','junk.txt','','text','junk','junk')",[]).unwrap();
            cached_execute(
                &c,
                "INSERT INTO name_words(name_words) VALUES('rebuild')",
                [],
            )
            .unwrap();
            cached_execute(
                &c,
                "INSERT INTO words(path,text) VALUES('junk.txt','junk secret text')",
                [],
            )
            .unwrap();
            cached_execute(
                &c,
                "INSERT INTO files VALUES('junk.txt','junk','[1,0]',1,1)",
                [],
            )
            .unwrap();
            cached_execute(
                &c,
                "INSERT INTO segments VALUES('junk.txt','n','text','junk','[1,0]','{}')",
                [],
            )
            .unwrap();
            cached_execute(&c,"INSERT INTO segment_vectors(namespace,kind,channel,dimension,vector,path) VALUES('n','text','text',2,X'0000000000000000','junk.txt')",[]).unwrap();
            cached_execute(
                &c,
                "INSERT INTO source_seen VALUES('junk.txt','root',1,1)",
                [],
            )
            .unwrap();
            cached_execute(
                &c,
                "INSERT INTO native_content VALUES('junk.txt','fingerprint','')",
                [],
            )
            .unwrap();
            cached_execute(
                &c,
                "INSERT INTO metadata VALUES('native_vectors_generation','1')",
                [],
            )
            .unwrap();
            drop(c);
            db.clear_index().unwrap();
            let c = db.connect(true).unwrap();
            for table in [
                "names",
                "name_words",
                "words",
                "files",
                "segments",
                "segment_vectors",
                "source_seen",
                "native_content",
                "metadata",
            ] {
                let count: u64 = c
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                    .unwrap();
                assert_eq!(count, 0, "{table}");
            }
            assert!(db.generation.load(Ordering::SeqCst) > 0);
            assert_eq!(
                std::fs::read(home.path().join("settings.json")).unwrap(),
                saved
            );
            assert_eq!(std::fs::read(home.path().join("index.key")).ok(), key);
            assert_eq!(std::fs::read_to_string(&original).unwrap(), "original file");
        }
    }
}

#[cfg(test)]
mod path_search_tests {
    use super::*;
    #[test]
    fn path_fragments_match_consecutive_folders_and_files() {
        let home = tempfile::tempdir().unwrap();
        let s = Settings {
            encrypt_index: false,
            excluded_sensitive: false,
            skip_windows_churn: false,
            roots: [r"C:\", r"D:\"]
                .iter()
                .map(|p| crate::config::Source {
                    path: (*p).into(),
                    mode: "quick".into(),
                })
                .collect(),
            ..Default::default()
        };
        let db = Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        for (path, kind) in [
            (r"C:\Users\test\AppData\Local", "folder"),
            (r"C:\Users\test\AppData\Local\RHI", "folder"),
            (r"C:\Users\test\AppData\Local\RHI\nvngx_dlssnr.dll", "file"),
            (r"D:\Games\RHI\nvngx_dlssnr.dll", "file"),
            (r"D:\Games\NotRHI\nvngx_dlssnr.dll", "file"),
            (r"D:\Games\RHI\nested\nvngx_dlssnr.dll", "file"),
            (r"D:\Games\Local\other\RHI", "folder"),
            (r"D:\AppData\LocalBackup", "folder"),
            (r"D:\Space Folder\RHI\note.txt", "file"),
        ] {
            Database::touch_name(&c, Path::new(path), kind, 10, 20, "quick", r"C:\", 1).unwrap();
        }
        for (query, count) in [
            (r"\Local\RHI", 2),
            (r"appdata\local", 2),
            (r"\RHI\nvngx_dlssnr.dll", 3),
            ("/rhi/NVNGX_DLSSNR.DLL", 3),
            (r"\RHI\nvngx", 3),
            (r"C:\Users\test\AppData\Local\RHI", 1),
            (r"\RHI\nvngx_dlssnr.dll ext:dll size:>10", 3),
            (r"\RHI\nvngx_dlssnr.dll ext:txt", 0),
            (r"Space Folder\RHI\note", 1),
            (r"*\*nr*", 4),
            (r"RHI\*nr*", 3),
            (r"*nr*", 4),
        ] {
            let hits = names(&db, query, &s, "All files", "", "").unwrap();
            assert_eq!(hits.len(), count, "{query}: {hits:?}");
        }
        let hits = names(
            &db,
            r"\RHI\nvngx_dlssnr.dll",
            &s,
            "All files",
            r"D:\Games",
            "",
        )
        .unwrap();
        assert_eq!(hits.len(), 2);
        assert!(
            names(&db, r"\RHI\nvngx_dlssnr.dll", &s, "All files", "", "dll")
                .unwrap()
                .is_empty()
        );
        assert!(
            names(&db, r"\RHI\nvngx_dlssnr.dll", &s, "Folders", "", "")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            names(&db, "nvngx_dlssnr.dll", &s, "All files", "", "")
                .unwrap()
                .len(),
            4
        );
        assert!(path_query("https://example.com").is_none());
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    fn fixture(encrypted: bool) -> (tempfile::TempDir, Database) {
        let home = tempfile::tempdir().unwrap();
        let s = Settings {
            encrypt_index: encrypted,
            ..Default::default()
        };
        let db = Database::new(home.path(), &s).unwrap();
        (home, db)
    }
    fn insert(c: &Connection, path: &str, content: bool) {
        Database::touch_name(
            c,
            Path::new(path),
            "file",
            1,
            20,
            if content { "full" } else { "quick" },
            r"C:\",
            1,
        )
        .unwrap();
        if content {
            Database::save_segments(
                c,
                path,
                &[crate::content::Segment::text(
                    "text",
                    format!("unique {path}"),
                    1,
                    0,
                )],
                &[vec![1.0, 0.0]],
                "fixture",
            )
            .unwrap();
        }
    }
    #[test]
    fn batched_cleanup_is_atomic_and_preserves_unrelated_search_records() {
        for encrypted in [false, true] {
            let (_h, db) = fixture(encrypted);
            let c = db.connect(false).unwrap();
            insert(&c, r"C:\keep.txt", true);
            insert(&c, r"C:\remove.txt", true);
            insert(&c, r"C:\quick.txt", false);
            c.execute(
                "INSERT INTO native_jobs VALUES(?1,'f','token','')",
                [r"C:\remove.txt"],
            )
            .unwrap();
            c.execute_batch("INSERT INTO native_staging VALUES('token',0,'text','pending','{}',2,X'0000000000000000');").unwrap();
            c.execute_batch("CREATE TEMP TRIGGER fail_cleanup BEFORE DELETE ON names BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
            let paths = vec![r"C:\remove.txt".into(), r"C:\quick.txt".into()];
            assert!(Database::delete_batch(&c, &paths).is_err());
            for table in ["names", "source_seen"] {
                assert_eq!(
                    c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    3
                );
            }
            assert_eq!(
                c.query_row(
                    "SELECT count(*) FROM words WHERE words MATCH 'unique'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                2
            );
            c.execute_batch("DROP TRIGGER fail_cleanup").unwrap();
            Database::delete_batch(&c, &paths).unwrap();
            Database::delete_batch(&c, &paths).unwrap(); // retry is safe
            for table in [
                "names",
                "source_seen",
                "segments",
                "segment_vectors",
                "words",
                "native_content",
            ] {
                assert_eq!(
                    c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    1,
                    "{table}"
                );
            }
            for table in ["native_jobs", "native_staging"] {
                assert_eq!(
                    c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                        .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
            }
            assert_eq!(
                c.query_row(
                    "SELECT count(*) FROM name_words WHERE name_words MATCH 'keep'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            assert_eq!(
                c.query_row(
                    "SELECT count(*) FROM name_words WHERE name_words MATCH 'remove'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            // Replacing content must use the same indexed route and retain new content.
            Database::save_segments(
                &c,
                r"C:\keep.txt",
                &[crate::content::Segment::text(
                    "text",
                    "replacement".into(),
                    1,
                    0,
                )],
                &[vec![0.0, 1.0]],
                "new",
            )
            .unwrap();
            assert_eq!(
                c.query_row(
                    "SELECT count(*) FROM words WHERE words MATCH 'unique'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            assert_eq!(
                c.query_row(
                    "SELECT count(*) FROM words WHERE words MATCH 'replacement'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            for sql in [
                "DELETE FROM words WHERE rowid IN (SELECT rowid FROM segments WHERE path=?1)",
                "DELETE FROM segment_vectors WHERE id IN (SELECT rowid FROM segments WHERE path=?1)",
            ] {
                let plans = c
                    .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                    .unwrap()
                    .query_map([r"C:\quick.txt"], |r| r.get::<_, String>(3))
                    .unwrap()
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .unwrap()
                    .join("; ");
                assert!(plans.contains("segments_path"), "{plans}");
                assert!(!plans.contains("SCAN segment_vectors"), "{plans}");
                assert!(
                    !plans
                        .split(';')
                        .any(|p| p.trim() == "SCAN words VIRTUAL TABLE INDEX 0:"),
                    "{plans}"
                );
            }
        }
    }
    #[test]
    #[ignore = "explicit cleanup performance comparison"]
    fn cleanup_benchmark_old_vs_batched() {
        let (_h, db) = fixture(false);
        let c = db.connect(false).unwrap();
        let records: Vec<_> = (0..20000)
            .map(|i| {
                crate::content::Segment::text(
                    "text",
                    format!("retained chunk {i} with searchable content"),
                    1,
                    0,
                )
            })
            .collect();
        Database::save_segments(
            &c,
            r"C:\retained.txt",
            &records,
            &vec![vec![1.0, 0.0]; records.len()],
            "fixture",
        )
        .unwrap();
        let paths: Vec<String> = (0..500).map(|i| format!(r"C:\stale{i}.txt")).collect();
        c.execute_batch("BEGIN").unwrap();
        for path in &paths {
            insert(&c, path, false);
        }
        c.execute_batch("COMMIT").unwrap();
        let mut legacy = Connection::open_in_memory().unwrap();
        rusqlite::backup::Backup::new(&c, &mut legacy)
            .unwrap()
            .run_to_completion(1000, Duration::from_millis(1), None)
            .unwrap();
        let began = std::time::Instant::now();
        legacy.execute_batch("BEGIN").unwrap();
        for path in &paths {
            let (id, folded): (i64, String) = legacy
                .query_row(
                    "SELECT rowid,folded FROM names WHERE path=?1",
                    [path],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            legacy
                .execute(
                    "INSERT INTO name_words(name_words,rowid,folded) VALUES('delete',?1,?2)",
                    params![id, folded],
                )
                .unwrap();
            for table in [
                "names",
                "words",
                "segments",
                "segment_vectors",
                "source_seen",
                "native_content",
            ] {
                legacy
                    .execute(&format!("DELETE FROM {table} WHERE path=?1"), [path])
                    .unwrap();
            }
        }
        legacy.execute_batch("COMMIT").unwrap();
        let old = began.elapsed().as_secs_f64();
        let began = std::time::Instant::now();
        Database::delete_batch(&c, &paths).unwrap();
        let new = began.elapsed().as_secs_f64();
        assert_eq!(
            c.query_row("SELECT count(*) FROM words", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            20000
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM segment_vectors", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            20000
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM names", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        println!(
            "CLEANUP BENCHMARK: 20,000 retained content chunks + 500 stale Quick files; old {old:.4}s; batched {new:.4}s; {:.1}x faster",
            old / new
        );
    }
}

#[cfg(test)]
mod deployment_resume_tests {
    use super::*;
    #[test]
    fn provider_switch_keeps_staged_chunks_and_rejects_mixed_dimensions() {
        let h = tempfile::tempdir().unwrap();
        let db = Database::new(
            h.path(),
            &Settings {
                encrypt_index: false,
                ..Default::default()
            },
        )
        .unwrap();
        let c = db.connect(false).unwrap();
        let remote = "v3|1|42|model|http://old-server/v1|true|false|ocr:png-v1";
        let local = "v3|1|42|model||ocr|ocr:png-v1";
        let (token, _) = Database::prepare_job(&c, "file.pdf", remote).unwrap();
        let record = crate::content::Segment::text("text", "saved".into(), 1, 0);
        Database::stage(&c, &token, 0, &[record.clone()], &[vec![1.0, 0.0]]).unwrap();
        let (same, count) = Database::prepare_job(&c, "file.pdf", local).unwrap();
        assert_eq!(same, token);
        assert_eq!(count, 1);
        assert!(Database::stage(&c, &token, 1, &[record], &[vec![1.0, 0.0, 0.0]]).is_err());
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM native_staging WHERE token=?1",
                [&token],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let cache = crate::vector_cache::Cache::default();
        Database::save_segments(
            &c,
            "other.txt",
            &[crate::content::Segment::text("text", "word".into(), 1, 0)],
            &[vec![1.0, 0.0]],
            "fixture",
        )
        .unwrap();
        assert!(
            cache
                .get(&c, 3, "Balanced", "FP32")
                .err()
                .unwrap()
                .to_string()
                .contains("dimensions differ")
        );
    }
}

#[cfg(test)]
mod internal_cleanup_tests {
    use super::*;
    #[test]
    fn metadata_upgrade_purges_internal_records_without_resetting_existing_cleanup_cursor() {
        let home = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.encrypt_index = false;
        s.skip_windows_churn = false;
        s.roots = vec![crate::config::Source {
            path: "C:\\".into(),
            mode: "quick".into(),
        }];
        let db = Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        for (path, kind) in [
            ("C:\\$Extend", "folder"),
            ("C:\\$Extend\\$Deleted\\old", "file"),
            ("C:\\$MFT", "file"),
            ("C:\\Projects\\$Extend\\notes.txt", "file"),
            ("C:\\ordinary.txt", "file"),
        ] {
            Database::touch_name(&c, Path::new(path), kind, 1, 20, "quick", "C:\\", 1).unwrap();
        }
        c.execute_batch("CREATE TABLE IF NOT EXISTS native_prune_prefixes(path TEXT PRIMARY KEY,cursor TEXT NOT NULL DEFAULT ''); INSERT OR REPLACE INTO metadata VALUES('prune_target','same-target'); INSERT OR REPLACE INTO metadata VALUES('prune_cursor','zzzz'); INSERT OR REPLACE INTO metadata VALUES('pruned_source_signature','same-target');").unwrap();
        drop(c);
        assert!(db.queue_internal_cleanup(&s).unwrap());
        assert!(!db.queue_internal_cleanup(&s).unwrap());
        let start = std::time::Instant::now();
        loop {
            if db.prune_step(&s, "same-target").unwrap() {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(3));
        }
        let c = db.connect(true).unwrap();
        let paths = c
            .prepare("SELECT path FROM names ORDER BY path")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            paths,
            vec!["C:\\Projects\\$Extend\\notes.txt", "C:\\ordinary.txt"]
        );
        assert_eq!(
            c.query_row(
                "SELECT value FROM metadata WHERE key='prune_cursor'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "zzzz"
        );
    }
}
