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
        c.execute(
            "ATTACH DATABASE ?1 AS converted KEY ?2",
            params![target.to_string_lossy(), literal],
        )?;
        c.query_row("SELECT sqlcipher_export('converted')", [], |_| Ok(()))?;
        let original: i64 = c.query_row("SELECT count(*) FROM names", [], |r| r.get(0))?;
        let copied: i64 = c.query_row("SELECT count(*) FROM converted.names", [], |r| r.get(0))?;
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
        conn.execute_batch(&format!("PRAGMA cache_size=-{}; PRAGMA mmap_size=0; PRAGMA temp_store=MEMORY; PRAGMA synchronous=NORMAL;",if read_only{self.cache_kib}else{16384}))?;
        if read_only {
            conn.execute_batch("PRAGMA query_only=ON;")?;
        }
        Ok(conn)
    }
    pub fn stats(&self) -> Result<Stats> {
        let c = self.connect(true)?;
        let mut s = Stats::default();
        s.files = c.query_row("SELECT count(*) FROM names WHERE kind!='folder'", [], |r| {
            r.get(0)
        })?;
        s.quick = c.query_row(
            "SELECT count(*) FROM names WHERE kind!='folder' AND index_mode IN ('quick','names')",
            [],
            |r| r.get(0),
        )?;
        s.full = s.files - s.quick;
        s.folders = c.query_row("SELECT count(*) FROM names WHERE kind='folder'", [], |r| {
            r.get(0)
        })?;
        s.vectors = c.query_row("SELECT count(*) FROM segment_vectors", [], |r| r.get(0))?;
        Ok(s)
    }
    pub fn source_stats(&self, root: &str) -> Result<SourceStats> {
        let c = self.connect(true)?;
        let normalized = root.replace('/', "\\");
        let root = normalized.as_str();
        let p = root.trim_end_matches(['\\', '/']);
        let upper = format!("{p}]");
        let lower = format!("{p}\\");
        let files=c.query_row("SELECT count(*) FROM names WHERE kind!='folder' AND (path=?1 OR (path>=?2 AND path<?3))",params![root,lower,upper],|r|r.get(0))?;
        let added=c.query_row("SELECT count(*) FROM source_seen s JOIN names n ON n.path=s.path WHERE n.kind!='folder' AND added>=?1 AND (s.path=?2 OR (s.path>=?3 AND s.path<?4))",params![chrono::Utc::now().timestamp()-3600,root,lower,upper],|r|r.get(0))?;
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
        let old: Option<(i64, String, i64, u64, String)> = c
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
                || o.3 != size
                || (o.4 != mode
                    && !(matches!(o.4.as_str(), "names" | "quick")
                        && matches!(mode, "names" | "quick")))
        });
        if changed {
            if let Some(o) = old {
                c.execute(
                    "INSERT INTO name_words(name_words,rowid,folded) VALUES('delete',?1,?2)",
                    params![o.0, o.1],
                )?;
                c.execute("UPDATE names SET name=?1,parent=?2,kind=?3,folded=?4,base=?5,mtime=?6,size=?7,index_mode=?8 WHERE rowid=?9",params![name,parent,kind,folded,name.to_lowercase(),mtime,size,mode,o.0])?;
                c.execute(
                    "INSERT INTO name_words(rowid,folded) VALUES(?1,?2)",
                    params![o.0, folded],
                )?;
            } else {
                c.execute(
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
                c.execute(
                    "INSERT INTO name_words(rowid,folded) VALUES(?1,?2)",
                    params![c.last_insert_rowid(), folded],
                )?;
            }
        }
        c.execute("INSERT INTO source_seen VALUES(?1,?2,?3,?4) ON CONFLICT(path) DO UPDATE SET source=excluded.source,epoch=excluded.epoch",params![path,root,epoch,added])?;
        Ok(changed)
    }
    pub fn delete(c: &Connection, path: &str) -> Result<()> {
        if let Some((id, folded)) = c
            .query_row(
                "SELECT rowid,folded FROM names WHERE path=?1",
                [path],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            c.execute(
                "INSERT INTO name_words(name_words,rowid,folded) VALUES('delete',?1,?2)",
                params![id, folded],
            )?;
            c.execute("DELETE FROM names WHERE rowid=?1", [id])?;
        }
        c.execute("INSERT INTO metadata VALUES('native_vectors_generation','1') ON CONFLICT(key) DO UPDATE SET value=cast(value AS INTEGER)+1",[])?;
        for table in [
            "words",
            "segments",
            "segment_vectors",
            "files",
            "source_seen",
            "native_content",
        ] {
            c.execute(&format!("DELETE FROM {table} WHERE path=?1"), [path])?;
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
        c.execute_batch("SAVEPOINT file_update")?;
        c.execute("INSERT INTO metadata VALUES('native_vectors_generation','1') ON CONFLICT(key) DO UPDATE SET value=cast(value AS INTEGER)+1",[])?;
        let result = (|| -> Result<()> {
            for table in ["words", "segments", "segment_vectors"] {
                c.execute(&format!("DELETE FROM {table} WHERE path=?1"), [path])?;
            }
            for (i, segment) in records.iter().enumerate() {
                let v = vectors.get(i).cloned().unwrap_or_default();
                let location=serde_json::json!({"page":segment.page,"seconds":segment.seconds,"line":segment.line,"embedding_source":if segment.media.is_empty(){"text"}else{"media"}}).to_string();
                c.execute(
                    "INSERT INTO segments VALUES(?1,'files',?2,?3,'[]',?4)",
                    params![path, segment.kind, segment.text, location],
                )?;
                let id = c.last_insert_rowid();
                c.execute(
                    "INSERT INTO words(rowid,path,text) VALUES(?1,?2,?3)",
                    params![id, path, segment.text],
                )?;
                if !v.is_empty() {
                    let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
                    c.execute(
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
            c.execute(
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
pub fn names(
    db: &Database,
    query: &str,
    s: &Settings,
    category: &str,
    scope: &str,
    excluded: &str,
) -> Result<Vec<Hit>> {
    let c = db.connect(true)?;
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    let low = q.clone();
    let upper = format!("{q}\u{10ffff}");
    let mut prefix =
        c.prepare("SELECT path,name,kind FROM names WHERE base>=?1 AND base<?2 LIMIT 400")?;
    for row in prefix.query_map(params![low, upper], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        add_name(&mut result, row?, &q, s, category, scope, excluded);
    }
    if q.chars().count() >= 3 {
        let terms = q
            .split_whitespace()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        let mut stmt=c.prepare("SELECT n.path,n.name,n.kind FROM name_words f JOIN names n ON n.rowid=f.rowid WHERE name_words MATCH ?1 LIMIT 2000")?;
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
    if !s.allowed(p) || !scope.is_empty() && !crate::config::under(path, scope) {
        return false;
    }
    let ext = p
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if excluded
        .split([',', ';', ' '])
        .any(|x| !x.is_empty() && x.trim_start_matches('.').eq_ignore_ascii_case(&ext))
    {
        return false;
    }
    match category {
        "Images" => crate::content::is_image(&ext),
        "Documents" => [
            "txt", "md", "pdf", "docx", "xlsx", "pptx", "csv", "log", "html", "rtf",
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
    pub fn stage(
        c: &Connection,
        token: &str,
        offset: usize,
        records: &[crate::content::Segment],
        vectors: &[Vec<f32>],
    ) -> Result<()> {
        c.execute_batch("CREATE TABLE IF NOT EXISTS native_staging(token TEXT,seq INTEGER,kind TEXT,text TEXT,location TEXT,dimension INTEGER,vector BLOB,PRIMARY KEY(token,seq)); BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            for (i, record) in records.iter().enumerate() {
                let vector = vectors.get(i).cloned().unwrap_or_default();
                let bytes: Vec<u8> = vector.iter().flat_map(|v| v.to_le_bytes()).collect();
                let location=serde_json::json!({"page":record.page,"seconds":record.seconds,"line":record.line,"embedding_source":if record.media.is_empty(){"text"}else{"media"}}).to_string();
                c.execute(
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
        c.execute_batch("SAVEPOINT file_update")?;
        let result = (|| -> Result<()> {
            for table in ["words", "segment_vectors", "segments"] {
                c.execute(&format!("DELETE FROM {table} WHERE path=?1"), [path])?;
            }
            let mut stmt=c.prepare("SELECT kind,text,location,dimension,vector FROM native_staging WHERE token=?1 ORDER BY seq")?;
            let mut rows = stmt.query([token])?;
            while let Some(row) = rows.next()? {
                let kind: String = row.get(0)?;
                let text: String = row.get(1)?;
                let location: String = row.get(2)?;
                let dim: usize = row.get(3)?;
                let bytes: Vec<u8> = row.get(4)?;
                c.execute(
                    "INSERT INTO segments VALUES(?1,'files',?2,?3,'[]',?4)",
                    params![path, kind, text, location],
                )?;
                let id = c.last_insert_rowid();
                c.execute(
                    "INSERT INTO words(rowid,path,text) VALUES(?1,?2,?3)",
                    params![id, path, text],
                )?;
                if dim > 0 {
                    let v: serde_json::Value = serde_json::from_str(&location)?;
                    c.execute(
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
            c.execute(
                "INSERT OR REPLACE INTO native_content VALUES(?1,?2,'')",
                params![path, fingerprint],
            )?;
            c.execute("DELETE FROM native_staging WHERE token=?1", [token])?;
            c.execute("INSERT INTO metadata VALUES('native_vectors_generation','1') ON CONFLICT(key) DO UPDATE SET value=cast(value AS INTEGER)+1",[])?;
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
            c.query_row("SELECT count(*) FROM native_staging", [], |r| r
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
