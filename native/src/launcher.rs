use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};
pub const MAX_TEXT: usize = 1024 * 1024;
#[derive(Clone, Debug)]
pub struct Item {
    pub value: String,
    pub label: String,
    pub detail: String,
    pub id: i64,
    pub image: bool,
}

pub fn emoji(query: &str) -> Vec<Item> {
    static DATA: OnceLock<Vec<(String, String, String)>> = OnceLock::new();
    let data = DATA.get_or_init(|| {
        let rows: Vec<(String, String)> =
            serde_json::from_str(include_str!("../assets/emoji.json"))
                .expect("bundled emoji catalog");
        let keywords: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../assets/emoji-keywords.json"))
                .expect("bundled emoji keywords");
        rows.into_iter()
            .map(|(glyph, name)| {
                let extra = keywords
                    .get(&glyph.replace('\u{fe0f}', ""))
                    .map(String::as_str)
                    .unwrap_or("");
                let searchable = format!("{name} {extra}").to_lowercase();
                (glyph, name, searchable)
            })
            .collect()
    });
    let query = query.trim().to_lowercase();
    let raw = query.strip_prefix("emoji:").unwrap_or(&query).trim();
    let q = match raw {
        "happy" => "smile",
        "lol" | "haha" | "xd" => "laugh",
        "usa" | "us" => "united states",
        "uk" => "united kingdom",
        _ => raw,
    };
    let words = q
        .split_whitespace()
        .map(|v| {
            if v.len() > 3 && v.ends_with('s') && !v.ends_with("ss") {
                &v[..v.len() - 1]
            } else {
                v
            }
        })
        .collect::<Vec<_>>();
    let normalized = q.replace('\u{fe0f}', "");
    let glyph_query = q.chars().any(|c| !c.is_ascii());
    data.iter()
        .filter(|(glyph, _, searchable)| {
            q.is_empty()
                || glyph == q
                || glyph_query && glyph.replace('\u{fe0f}', "") == normalized
                || words.iter().all(|w| {
                    if w.len() <= 2 {
                        searchable
                            .split(|c: char| !c.is_alphanumeric())
                            .any(|t| t == *w)
                    } else {
                        searchable.contains(w)
                    }
                })
        })
        .take(4096)
        .map(|(glyph, name, _)| Item {
            value: glyph.clone(),
            label: format!("{glyph}  {name}"),
            detail: "Emoji · Enter to copy".into(),
            id: 0,
            image: false,
        })
        .collect()
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(default)]
pub struct ClipOptions {
    pub capture_text: bool,
    pub capture_images: bool,
    pub image_hours: Option<u32>,
    pub max_items: u32,
    pub max_mb: u32,
    pub max_saved_mb: u32,
    pub max_text_kib: u32,
    pub max_image_mib: u32,
    pub paste_on_enter: bool,
    pub pause_minutes: u32,
    pub ocr: bool,
}
impl Default for ClipOptions {
    fn default() -> Self {
        Self {
            capture_text: true,
            capture_images: true,
            image_hours: None,
            max_items: 500,
            max_mb: 128,
            max_saved_mb: 256,
            max_text_kib: 16,
            max_image_mib: 4,
            paste_on_enter: false,
            pause_minutes: 15,
            ocr: false,
        }
    }
}
static PAUSE_UNTIL: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
static IGNORE_NEXT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn pause(minutes: u32) {
    PAUSE_UNTIL.store(
        if minutes == 0 {
            0
        } else {
            chrono::Utc::now().timestamp() + minutes as i64 * 60
        },
        std::sync::atomic::Ordering::Release,
    );
}
pub fn paused() -> bool {
    PAUSE_UNTIL.load(std::sync::atomic::Ordering::Acquire) > chrono::Utc::now().timestamp()
}
pub fn ignore_next() {
    IGNORE_NEXT.store(true, std::sync::atomic::Ordering::Release);
}
#[derive(Clone)]
pub struct Clipboard {
    file: PathBuf,
    options: ClipOptions,
}
impl Clipboard {
    pub fn new(home: &Path) -> Self {
        Self {
            file: home.join("clipboard.sqlite"),
            options: ClipOptions::default(),
        }
    }
    pub fn configured(mut self, s: &crate::config::Settings) -> Self {
        self.options = s.clipboard_options.clone();
        self
    }
    fn connect(&self) -> Result<Connection> {
        static MIGRATIONS: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _migration = MIGRATIONS.lock().unwrap_or_else(|p| p.into_inner());
        let c = Connection::open(&self.file)?;
        c.busy_timeout(std::time::Duration::from_secs(2))?;
        c.execute_batch("PRAGMA secure_delete=ON; CREATE TABLE IF NOT EXISTS clips(id INTEGER PRIMARY KEY,created INTEGER NOT NULL,payload TEXT NOT NULL,digest TEXT NOT NULL UNIQUE);")?;
        let columns: Vec<String> = c
            .prepare("PRAGMA table_info(clips)")?
            .query_map([], |r| r.get(1))?
            .collect::<rusqlite::Result<_>>()?;
        if !columns.iter().any(|v| v == "kind") {
            c.execute_batch("ALTER TABLE clips ADD COLUMN kind TEXT NOT NULL DEFAULT 'text';ALTER TABLE clips ADD COLUMN bytes INTEGER NOT NULL DEFAULT 0;")?;
        }
        if !columns.iter().any(|v| v == "embedding") {
            c.execute_batch("ALTER TABLE clips ADD COLUMN embedding TEXT NOT NULL DEFAULT ''; ALTER TABLE clips ADD COLUMN model_key TEXT NOT NULL DEFAULT '';")?;
        }
        let columns: Vec<String> = c
            .prepare("PRAGMA table_info(clips)")?
            .query_map([], |r| r.get(1))?
            .collect::<rusqlite::Result<_>>()?;
        for (name, sql) in [
            (
                "pinned",
                "ALTER TABLE clips ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
            ),
            (
                "title",
                "ALTER TABLE clips ADD COLUMN title TEXT NOT NULL DEFAULT ''",
            ),
            (
                "tags",
                "ALTER TABLE clips ADD COLUMN tags TEXT NOT NULL DEFAULT ''",
            ),
            (
                "ocr",
                "ALTER TABLE clips ADD COLUMN ocr TEXT NOT NULL DEFAULT ''",
            ),
        ] {
            if !columns.iter().any(|v| v == name) {
                c.execute_batch(sql)?;
            }
        }
        if !columns.iter().any(|v| v == "ocr_done") {
            c.execute_batch("ALTER TABLE clips ADD COLUMN ocr_done INTEGER NOT NULL DEFAULT 0")?;
        }
        if !columns.iter().any(|v| v == "sticky") {
            c.execute_batch("ALTER TABLE clips ADD COLUMN sticky INTEGER NOT NULL DEFAULT 0")?;
        }
        let version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            c.execute("UPDATE clips SET bytes=length(payload) WHERE bytes=0", [])?;
            c.execute_batch("PRAGMA user_version=1")?;
        }
        Ok(c)
    }
    fn model_key(s: &crate::config::Settings) -> String {
        format!(
            "{}|{}|{}|{}",
            s.model,
            s.projector,
            if s.remote_embeddings() {
                format!("{}|{}", s.remote_url, s.remote_model)
            } else {
                String::new()
            },
            s.server
        )
    }
    pub fn index_next_image(
        &self,
        s: &crate::config::Settings,
        models: &crate::models::Models,
    ) -> Result<bool> {
        if !self.file.exists() {
            return Ok(false);
        }
        let key = Self::model_key(s);
        let row:Option<i64>=self.connect()?.query_row("SELECT id FROM clips WHERE kind='image' AND (embedding='' OR model_key!=?1) ORDER BY created DESC LIMIT 1",[&key],|r|r.get(0)).optional()?;
        let Some(id) = row else { return Ok(false) };
        let media = self.image(id)?;
        let segment = crate::content::Segment {
            kind: "image".into(),
            text: String::new(),
            page: 0,
            seconds: -1.0,
            line: 0,
            media,
        };
        let vectors = models.segments(s, &[segment], &|| {
            crate::scheduling::idle_allowed(s, crate::scheduling::metrics())
                .map_err(|e| anyhow::anyhow!(e))
        })?;
        let vector = vectors
            .into_iter()
            .next()
            .context("Missing clipboard image embedding")?;
        let payload = crate::config::protect(&serde_json::to_string(&vector)?)?;
        self.connect()?.execute(
            "UPDATE clips SET embedding=?1,model_key=?2,bytes=bytes-length(embedding)+length(?1) WHERE id=?3",
            params![payload, key, id],
        )?;
        Ok(true)
    }
    pub fn search_with_images(
        &self,
        q: &str,
        s: &crate::config::Settings,
        models: &crate::models::Models,
    ) -> Result<Vec<Item>> {
        let mut items = self.clone().configured(s).search(q, s.clipboard_hours)?;
        if !q.trim().is_empty() {
            items.sort_by_key(|item| item.image);
        }
        let (filter, query) = q
            .split_once(':')
            .filter(|(p, _)| matches!(*p, "saved" | "pinned" | "images" | "text" | "links"))
            .unwrap_or(("all", q));
        if !s.clipboard_image_embeddings
            || query.trim().is_empty()
            || matches!(filter, "text" | "links")
            || !self.file.exists()
        {
            return Ok(items);
        }
        let c = self.connect()?;
        let mut stmt=c.prepare("SELECT id,created,embedding FROM clips WHERE kind='image' AND embedding!='' AND model_key=?1 AND (?2!=1 OR pinned=1) AND (?3!=1 OR sticky=1) ORDER BY sticky DESC,created DESC LIMIT 500")?;
        let rows = stmt
            .query_map(
                params![Self::model_key(s), filter == "saved", filter == "pinned"],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.is_empty() {
            return Ok(items);
        }
        let Ok(query) = models.query(s, query.trim().trim_start_matches("clipboard:").trim())
        else {
            return Ok(items);
        };
        let mut scored = Vec::new();
        for (id, created, payload) in rows {
            let vector: Vec<f32> = serde_json::from_str(&crate::config::reveal(&payload)?)?;
            let Some(score) = cosine(&query, &vector) else {
                continue;
            };
            if score < 0.15 {
                continue;
            }
            scored.push((
                score,
                Item {
                    id,
                    image: true,
                    value: String::new(),
                    label: "Copied image".into(),
                    detail: format!(
                        "Image meaning · {:.3} · {} · Enter to copy",
                        score,
                        chrono::DateTime::from_timestamp(created, 0)
                            .map(|d| d
                                .with_timezone(&chrono::Local)
                                .format("%b %d, %Y · %H:%M:%S")
                                .to_string())
                            .unwrap_or_default()
                    ),
                },
            ));
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut found = std::collections::HashSet::new();
        let mut result = items;
        result.extend(scored.into_iter().map(|(_, item)| item));
        result.retain(|i| found.insert(i.id));
        if filter == "all" {
            let pinned = self
                .search("pinned:", s.clipboard_hours)?
                .into_iter()
                .map(|i| i.id)
                .collect::<std::collections::HashSet<_>>();
            result.sort_by_key(|i| !pinned.contains(&i.id));
        }
        result.truncate(100);
        Ok(result)
    }
    pub fn purge(&self, hours: u32, now: i64) -> Result<()> {
        if !self.file.exists() {
            return Ok(());
        }
        let c = self.connect()?;
        if hours > 0 {
            c.execute(
                "DELETE FROM clips WHERE pinned=0 AND kind!='image' AND created<?1",
                [now - hours as i64 * 3600],
            )?;
        }
        let image_hours = self.options.image_hours.unwrap_or(hours);
        if image_hours > 0 {
            c.execute(
                "DELETE FROM clips WHERE pinned=0 AND kind='image' AND created<?1",
                [now - image_hours as i64 * 3600],
            )?;
        }
        if self.options.max_items > 0 {
            c.execute("DELETE FROM clips WHERE pinned=0 AND id NOT IN (SELECT id FROM clips WHERE pinned=0 ORDER BY created DESC,id DESC LIMIT ?1)",[self.options.max_items])?;
        }
        if self.options.max_mb > 0 {
            c.execute("DELETE FROM clips WHERE pinned=0 AND id IN (SELECT id FROM (SELECT id,SUM(bytes) OVER (ORDER BY created DESC,id DESC) AS total FROM clips WHERE pinned=0) WHERE total>?1)",[self.options.max_mb as i64*1024*1024])?;
        }
        Ok(())
    }
    pub fn pinned(&self, id: i64) -> Result<bool> {
        Ok(self
            .connect()?
            .query_row("SELECT pinned FROM clips WHERE id=?1", [id], |r| r.get(0))?)
    }
    pub fn pin(&self, id: i64, pinned: bool) -> Result<()> {
        let mut c = self.connect()?;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let already: bool =
            tx.query_row("SELECT pinned FROM clips WHERE id=?1", [id], |r| r.get(0))?;
        if pinned && !already && self.options.max_saved_mb > 0 {
            let bytes: i64 = tx.query_row(
                "SELECT COALESCE(SUM(bytes),0) FROM clips WHERE pinned=1 OR id=?1",
                [id],
                |r| r.get(0),
            )?;
            anyhow::ensure!(
                bytes <= self.options.max_saved_mb as i64 * 1024 * 1024,
                "Saved clipboard storage is full. Increase its limit or remove saved items."
            );
        }
        tx.execute(
            "UPDATE clips SET pinned=?1,sticky=CASE WHEN ?1=0 THEN 0 ELSE sticky END WHERE id=?2",
            params![pinned, id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn stick(&self, id: i64, sticky: bool) -> Result<()> {
        if sticky {
            self.pin(id, true)?;
        }
        self.connect()?.execute(
            "UPDATE clips SET sticky=?1 WHERE id=?2",
            params![sticky, id],
        )?;
        Ok(())
    }
    pub fn sticky(&self, id: i64) -> Result<bool> {
        Ok(self
            .connect()?
            .query_row("SELECT sticky FROM clips WHERE id=?1", [id], |r| r.get(0))?)
    }
    pub fn annotate(&self, id: i64, title: &str, tags: &str) -> Result<()> {
        anyhow::ensure!(
            title.len() <= 1024 && tags.len() <= 4096,
            "Title or tags are too long"
        );
        self.connect()?.execute(
            "UPDATE clips SET title=?1,tags=?2,bytes=bytes-length(title)-length(tags)+length(?1)+length(?2) WHERE id=?3",
            params![
                crate::config::protect(title)?,
                crate::config::protect(tags)?,
                id
            ],
        )?;
        Ok(())
    }
    pub fn metadata(&self, id: i64) -> Result<(String, String)> {
        let (title, tags): (String, String) =
            self.connect()?
                .query_row("SELECT title,tags FROM clips WHERE id=?1", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
        Ok((
            crate::config::reveal(&title)?,
            crate::config::reveal(&tags)?,
        ))
    }
    pub fn edit(&self, id: i64, value: &str) -> Result<()> {
        anyhow::ensure!(
            !value.trim().is_empty() && value.len() <= self.options.max_text_kib as usize * 1024,
            "Text is empty or exceeds the configured limit"
        );
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(crate::config::read_key(self.file.parent().unwrap())?);
        hash.update(value.as_bytes());
        let payload = crate::config::protect(value)?;
        self.connect()?.execute(
            "UPDATE clips SET payload=?1,digest=?2,bytes=bytes-length(payload)+?3 WHERE id=?4 AND kind='text'",
            params![
                payload,
                format!("{:x}", hash.finalize()),
                payload.len() as i64,
                id
            ],
        )?;
        Ok(())
    }
    pub fn clear_recent(&self, seconds: Option<i64>) -> Result<()> {
        if self.file.exists() {
            self.connect()?.execute(
                "DELETE FROM clips WHERE pinned=0 AND created>=?1",
                [seconds.map_or(0, |v| chrono::Utc::now().timestamp() - v)],
            )?;
        }
        Ok(())
    }
    pub fn stats(&self) -> Result<(u64, u64, u64)> {
        if !self.file.exists() {
            return Ok((0, 0, 0));
        }
        Ok(self.connect()?.query_row(
            "SELECT COUNT(*),COALESCE(SUM(pinned),0),COALESCE(SUM(bytes),0) FROM clips",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    }
    pub fn record(&self, text: &str, hours: u32, now: i64) -> Result<()> {
        use sha2::{Digest, Sha256};
        if text.trim().is_empty()
            || text.len() > MAX_TEXT
            || text.len() > self.options.max_text_kib as usize * 1024
        {
            return Ok(());
        }
        let protected = crate::config::protect(text)?;
        let digest = format!("{:x}", {
            let mut hasher = Sha256::new();
            hasher.update(crate::config::read_key(self.file.parent().unwrap())?);
            hasher.update(text.as_bytes());
            hasher.finalize()
        });
        let c = self.connect()?;
        c.execute("INSERT INTO clips(created,payload,digest,bytes) VALUES(?1,?2,?3,?4) ON CONFLICT(digest) DO UPDATE SET created=excluded.created,payload=excluded.payload",params![now,protected,digest,protected.len() as i64])?;
        drop(c);
        self.purge(hours, now)
    }
    pub fn search(&self, query: &str, hours: u32) -> Result<Vec<Item>> {
        let now = chrono::Utc::now().timestamp();
        self.purge(hours, now)?;
        if !self.file.exists() {
            return Ok(vec![]);
        }
        let c = self.connect()?;
        let mut statement = c.prepare(
            "SELECT id,created,CASE WHEN kind=\'image\' THEN \'\' ELSE payload END,kind,pinned,title,tags,ocr,sticky FROM clips ORDER BY sticky DESC,created DESC,id DESC",
        )?;
        let (filter, query) = query
            .split_once(':')
            .filter(|(p, _)| matches!(*p, "saved" | "pinned" | "text" | "images" | "links"))
            .unwrap_or(("all", query));
        let q = query
            .trim()
            .trim_start_matches("clipboard:")
            .trim()
            .to_lowercase();
        let words = q.split_whitespace().collect::<Vec<_>>();
        let mut result = Vec::new();
        for row in statement.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, bool>(8)?,
            ))
        })? {
            let (id, created, payload, kind, pinned, title, tags, ocr, sticky) = row?;
            if filter == "pinned" && !sticky
                || filter == "saved" && !pinned
                || filter == "images" && kind != "image"
                || filter == "text" && kind != "text"
            {
                continue;
            }
            let title = crate::config::reveal(&title)?;
            let tags = crate::config::reveal(&tags)?;
            let ocr = crate::config::reveal(&ocr)?;
            let image = kind == "image";
            let value = if image {
                String::new()
            } else {
                crate::config::reveal(&payload)?
            };
            if filter == "links" && !(value.starts_with("https://") || value.starts_with("http://"))
            {
                continue;
            }
            let normalized = format!("{value} {title} {tags} {ocr}").to_lowercase();
            if q.is_empty()
                || image && "image picture screenshot".contains(&q)
                || words.iter().all(|word| normalized.contains(*word))
            {
                let label = if !title.is_empty() {
                    title.clone()
                } else if image {
                    "Copied image".into()
                } else {
                    value
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect()
                };
                result.push(Item {
                    value,
                    label: if sticky {
                        format!("📌 {label}")
                    } else if pinned {
                        format!("☆ {label}")
                    } else {
                        label
                    },
                    detail: format!(
                        "Clipboard · {} · Enter to copy",
                        chrono::DateTime::from_timestamp(created, 0)
                            .map(|d| d
                                .with_timezone(&chrono::Local)
                                .format("%b %d, %Y · %H:%M:%S")
                                .to_string())
                            .unwrap_or_default()
                    ),
                    id,
                    image,
                });
                if result.len() == 100 {
                    break;
                }
            }
        }
        Ok(result)
    }

    pub fn record_image(&self, bytes: &[u8], hours: u32, now: i64) -> Result<()> {
        use base64::Engine as _;
        use sha2::{Digest, Sha256};
        anyhow::ensure!(
            bytes.len() <= self.options.max_image_mib.min(4) as usize * 1024 * 1024,
            "Clipboard image exceeds 4 MiB"
        );
        let img = crate::capture::rgba(bytes)?;
        anyhow::ensure!(
            img.width() as u64 * img.height() as u64 <= 16_000_000,
            "Clipboard image exceeds 16 megapixels"
        );
        let mut hash = Sha256::new();
        hash.update(crate::config::read_key(self.file.parent().unwrap())?);
        hash.update(b"image");
        hash.update(bytes);
        let digest = format!("{:x}", hash.finalize());
        let payload =
            crate::config::protect(&base64::engine::general_purpose::STANDARD.encode(bytes))?;
        let c = self.connect()?;
        c.execute("INSERT INTO clips(created,payload,digest,kind,bytes) VALUES(?1,?2,?3,'image',?4) ON CONFLICT(digest) DO UPDATE SET created=excluded.created",params![now,payload,digest,payload.len() as i64])?;
        drop(c);
        self.purge(hours, now)
    }
    pub fn image(&self, id: i64) -> Result<Vec<u8>> {
        use base64::Engine as _;
        let payload: String = self.connect()?.query_row(
            "SELECT payload FROM clips WHERE id=?1 AND kind='image'",
            [id],
            |r| r.get(0),
        )?;
        let value = crate::config::reveal(&payload)?;
        anyhow::ensure!(value.len() <= 6 * 1024 * 1024, "Stored image exceeds limit");
        Ok(base64::engine::general_purpose::STANDARD.decode(value)?)
    }
    pub fn copy_image(&self, id: i64) -> Result<()> {
        let img = crate::capture::rgba(&self.image(id)?)?;
        arboard::Clipboard::new()?.set_image(arboard::ImageData {
            width: img.width() as usize,
            height: img.height() as usize,
            bytes: std::borrow::Cow::Owned(img.into_raw()),
        })?;
        Ok(())
    }
    pub fn ocr_next(&self, s: &crate::config::Settings) -> Result<bool> {
        if !s.clipboard_options.ocr || s.ocr.is_empty() || !self.file.exists() {
            return Ok(false);
        }
        let id: Option<i64> = self
            .connect()?
            .query_row(
                "SELECT id FROM clips WHERE kind='image' AND ocr_done=0 ORDER BY created DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = id else { return Ok(false) };
        let value = crate::content::ocr_bytes(&self.image(id)?, s)?;
        self.connect()?.execute(
            "UPDATE clips SET ocr=?1,ocr_done=1,bytes=bytes-length(ocr)+length(?1) WHERE id=?2",
            params![crate::config::protect(&value)?, id],
        )?;
        Ok(true)
    }
    pub fn export_saved(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
        if !self.file.exists() {
            return Ok(());
        }
        let c = self.connect()?;
        let mut stmt = c.prepare(
            "SELECT kind,payload,title,tags,sticky FROM clips WHERE pinned=1 ORDER BY id",
        )?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, bool>(4)?,
            ))
        })? {
            let (kind, payload, title, tags, sticky) = row?;
            serde_json::to_writer(
                &mut out,
                &serde_json::json!({"version":1,"sticky":sticky,"kind":kind,"value":crate::config::reveal(&payload)?,"title":crate::config::reveal(&title)?,"tags":crate::config::reveal(&tags)?}),
            )?;
            out.write_all(b"\n")?;
        }
        out.flush()?;
        Ok(())
    }
    pub fn import_saved(&self, path: &Path) -> Result<()> {
        use base64::Engine as _;
        use sha2::{Digest, Sha256};
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(std::fs::File::open(path)?);
        let mut c = self.connect()?;
        let tx = c.transaction()?;
        loop {
            let mut line = Vec::new();
            let mut limited = (&mut reader).take(7 * 1024 * 1024);
            use std::io::Read;
            if limited.read_until(b'\n', &mut line)? == 0 {
                break;
            }
            anyhow::ensure!(line.len() < 7 * 1024 * 1024, "Backup item is too large");
            let row: serde_json::Value = serde_json::from_slice(&line)?;
            anyhow::ensure!(row["version"] == 1, "Unsupported clipboard backup version");
            let kind = row["kind"].as_str().unwrap_or("");
            anyhow::ensure!(
                kind == "text" || kind == "image",
                "Unsupported clipboard item type"
            );
            let value = row["value"].as_str().context("Missing clipboard content")?;
            let mut hash = Sha256::new();
            hash.update(crate::config::read_key(self.file.parent().unwrap())?);
            if kind == "image" {
                let bytes = base64::engine::general_purpose::STANDARD.decode(value)?;
                anyhow::ensure!(bytes.len() <= 4 * 1024 * 1024, "Image too large");
                crate::capture::rgba(&bytes)?;
                hash.update(b"image");
                hash.update(&bytes);
            } else {
                anyhow::ensure!(value.len() <= MAX_TEXT, "Text too large");
                hash.update(value.as_bytes());
            }
            let payload = crate::config::protect(value)?;
            let title = row["title"].as_str().unwrap_or("");
            let tags = row["tags"].as_str().unwrap_or("");
            anyhow::ensure!(
                title.len() <= 1024 && tags.len() <= 4096,
                "Metadata too long"
            );
            tx.execute("INSERT INTO clips(created,payload,digest,kind,bytes,pinned,title,tags) VALUES(?1,?2,?3,?4,?5,1,?6,?7) ON CONFLICT(digest) DO UPDATE SET pinned=1,title=excluded.title,tags=excluded.tags",params![chrono::Utc::now().timestamp(),payload,format!("{:x}",hash.finalize()),kind,payload.len() as i64,crate::config::protect(title)?,crate::config::protect(tags)?])?;
            if row["sticky"].as_bool().unwrap_or(false) {
                tx.execute(
                    "UPDATE clips SET sticky=1 WHERE digest=?1",
                    [format!("{:x}", {
                        let mut h = Sha256::new();
                        h.update(crate::config::read_key(self.file.parent().unwrap())?);
                        if kind == "image" {
                            h.update(b"image");
                            h.update(base64::engine::general_purpose::STANDARD.decode(value)?);
                        } else {
                            h.update(value.as_bytes());
                        }
                        h.finalize()
                    })],
                )?;
            }
            if self.options.max_saved_mb > 0 {
                let total: i64 = tx.query_row(
                    "SELECT COALESCE(SUM(bytes),0) FROM clips WHERE pinned=1",
                    [],
                    |r| r.get(0),
                )?;
                anyhow::ensure!(
                    total <= self.options.max_saved_mb as i64 * 1024 * 1024,
                    "Saved clipboard storage is full; import rolled back"
                );
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn remove(&self, id: i64) -> Result<()> {
        let c = self.connect()?;
        c.execute("DELETE FROM clips WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn clear(&self) -> Result<()> {
        if self.file.exists() {
            let c = self.connect()?;
            c.execute("DELETE FROM clips", [])?;
            c.execute_batch("VACUUM;")?;
        }
        Ok(())
    }
}
pub fn monitor(
    home: PathBuf,
    settings: std::sync::Arc<std::sync::Mutex<crate::config::Settings>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    tx: crate::engine::NotifySender,
    models: crate::models::Models,
) {
    let semantic_home = home.clone();
    let semantic_settings = settings.clone();
    let semantic_stop = stop.clone();
    let semantic_tx = tx.clone();
    std::thread::spawn(move || {
        let store = Clipboard::new(&semantic_home);
        let mut ocr_retry = std::time::Instant::now();
        while !semantic_stop.load(std::sync::atomic::Ordering::Relaxed) {
            let s = semantic_settings.lock().unwrap().clone();
            if s.clipboard_enabled
                && !paused()
                && s.clipboard_options.ocr
                && std::time::Instant::now() >= ocr_retry
                && crate::scheduling::idle_allowed(&s, crate::scheduling::metrics()).is_ok()
            {
                match store.ocr_next(&s) {
                    Ok(true) => {
                        let _ = semantic_tx.send(crate::engine::Event::ClipboardChanged);
                    }
                    Err(error) => {
                        ocr_retry = std::time::Instant::now() + std::time::Duration::from_secs(60);
                        let _ = semantic_tx.try_send(crate::engine::Event::Log(
                            "Warning".into(),
                            format!("Clipboard OCR paused for 60 seconds: {error}"),
                        ));
                    }
                    _ => {}
                }
            }
            if s.clipboard_enabled
                && s.clipboard_image_embeddings
                && crate::scheduling::idle_allowed(&s, crate::scheduling::metrics()).is_ok()
            {
                match store.clone().configured(&s).index_next_image(&s, &models) {
                    Ok(true) => {
                        let _ = semantic_tx.send(crate::engine::Event::ClipboardChanged);
                    }
                    Err(_) => {
                        for _ in 0..30 {
                            if semantic_stop.load(std::sync::atomic::Ordering::Relaxed) {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                    _ => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
    std::thread::spawn(move || {
        use std::sync::atomic::Ordering;
        let store = Clipboard::new(&home);
        let mut sequence = crate::platform::clipboard_sequence();
        let mut enabled = false;
        let mut purge = std::time::Instant::now() - std::time::Duration::from_secs(60);
        while !stop.load(Ordering::Relaxed) {
            let (clipboard_enabled, clipboard_hours, clipboard_excluded_apps, options) = {
                let s = settings.lock().unwrap();
                (
                    s.clipboard_enabled,
                    s.clipboard_hours,
                    s.clipboard_excluded_apps.clone(),
                    s.clipboard_options.clone(),
                )
            };
            let store = Clipboard {
                file: store.file.clone(),
                options: options.clone(),
            };
            if purge.elapsed().as_secs() >= 60 {
                let _ = store.purge(clipboard_hours, chrono::Utc::now().timestamp());
                if store.file.exists() {
                    let _ = tx.try_send(crate::engine::Event::ClipboardChanged);
                }
                purge = std::time::Instant::now();
            }
            if clipboard_enabled {
                let current = crate::platform::clipboard_sequence();
                if !enabled {
                    sequence = current;
                } // Enabling never records content copied before opt-in.
                if current != sequence {
                    if paused() || IGNORE_NEXT.swap(false, Ordering::AcqRel) {
                        sequence = current;
                        continue;
                    }
                    if let Ok(value) = crate::platform::clipboard_text(&clipboard_excluded_apps) {
                        if options.capture_images
                            && value.is_none()
                            && crate::platform::clipboard_allowed(&clipboard_excluded_apps)
                                .unwrap_or(false)
                        {
                            if let Ok(mut clip) = arboard::Clipboard::new() {
                                if let Ok(img) = clip.get_image() {
                                    if img.width as u64 * img.height as u64 <= 16_000_000
                                        && img.bytes.len() <= 64 * 1024 * 1024
                                        && crate::platform::clipboard_sequence() == current
                                    {
                                        if let Some(image) = image::RgbaImage::from_raw(
                                            img.width as u32,
                                            img.height as u32,
                                            img.bytes.into_owned(),
                                        ) {
                                            let mut png = std::io::Cursor::new(Vec::new());
                                            if image::DynamicImage::ImageRgba8(image)
                                                .write_to(&mut png, image::ImageFormat::Png)
                                                .is_ok()
                                                && store
                                                    .record_image(
                                                        png.get_ref(),
                                                        clipboard_hours,
                                                        chrono::Utc::now().timestamp(),
                                                    )
                                                    .is_ok()
                                            {
                                                let _ =
                                                    tx.send(crate::engine::Event::ClipboardChanged);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        sequence = current;
                        if let Some(value) = value.filter(|_| options.capture_text) {
                            if store
                                .record(&value, clipboard_hours, chrono::Utc::now().timestamp())
                                .is_ok()
                            {
                                let _ = tx.try_send(crate::engine::Event::ClipboardChanged);
                            }
                        }
                    } // A clipboard temporarily locked by another app is retried.
                }
            }
            enabled = clipboard_enabled;
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emoji_search_is_local_and_matches_multiple_words() {
        assert!(emoji("thumbs up").iter().any(|r| r.value == "👍"));
        assert!(emoji("dog").iter().any(|r| r.value == "🐶"));
        assert!(emoji("smile").iter().any(|r| r.value == "😃"));
        assert!(emoji("us").iter().any(|r| r.value == "🇺🇸"));
        assert!(emoji("kiss").iter().any(|r| r.value == "😘"));
        assert!(emoji("notarealemojiabcdef").is_empty());
    }
    #[test]
    fn history_expires_deduplicates_and_protects_payload() {
        let temp = tempfile::tempdir().unwrap();
        let store = Clipboard::new(temp.path());
        let now = chrono::Utc::now().timestamp();
        store
            .record("private clipboard fixture", 24, now - 100)
            .unwrap();
        store.record("private clipboard fixture", 24, now).unwrap();
        store.record("expired fixture", 24, now - 90000).unwrap();
        let result = store.search("private", 24).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, "private clipboard fixture");
        let data = std::fs::read(&store.file).unwrap();
        assert!(!data.windows(25).any(|w| w == b"private clipboard fixture"));
        store.clear().unwrap();
        assert!(store.search("", 24).unwrap().is_empty());
    }
}

#[cfg(test)]
mod bounded_tests {
    use super::*;
    #[test]
    fn capture_is_bounded_and_search_does_not_keep_entire_history() {
        let temp = tempfile::tempdir().unwrap();
        let store = Clipboard::new(temp.path());
        let now = chrono::Utc::now().timestamp();
        for i in 0..510 {
            store
                .record(&format!("bounded clipboard fixture {i}"), 24, now + i)
                .unwrap();
        }
        let c = store.connect().unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM clips", [], |r| r.get::<_, usize>(0))
                .unwrap(),
            500
        );
        let start = std::time::Instant::now();
        let items = store.search("fixture", 24).unwrap();
        println!("500-entry clipboard search: {:?}", start.elapsed());
        assert_eq!(items.len(), 100);
        store.record(&"a".repeat(MAX_TEXT + 1), 24, now).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM clips", [], |r| r.get::<_, usize>(0))
                .unwrap(),
            500
        );
    }
}

#[cfg(test)]
mod image_history_tests {
    use super::*;
    fn png() -> Vec<u8> {
        let mut image = image::RgbaImage::from_pixel(240, 160, image::Rgba([110, 75, 180, 255]));
        for y in 40..120 {
            for x in 60..180 {
                image.put_pixel(x, y, image::Rgba([40, 160, 210, 255]));
            }
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }
    #[test]
    fn images_are_protected_lazy_deduplicated_and_expire() {
        let home = tempfile::tempdir().unwrap();
        let store = Clipboard::new(home.path());
        let now = chrono::Utc::now().timestamp();
        let image = png();
        store
            .record("Markdown **clipboard fixture**\nsecond line", 24, now - 2)
            .unwrap();
        store.record_image(&image, 24, now).unwrap();
        store.record_image(&image, 24, now + 1).unwrap();
        let items = store.search("", 24).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].image);
        assert!(items[0].value.is_empty());
        assert!(items[0].detail.contains("Clipboard"));
        assert_eq!(store.image(items[0].id).unwrap(), image);
        let raw = std::fs::read(home.path().join("clipboard.sqlite")).unwrap();
        assert!(!raw.windows(8).any(|w| w == b"\x89PNG\r\n\x1a\n"));
        assert!(
            store
                .record_image(&vec![0; 4 * 1024 * 1024 + 1], 24, now)
                .is_err()
        );
        store.purge(1, now + 7200).unwrap();
        assert!(store.search("", 24).unwrap().is_empty());
    }
    #[test]
    #[ignore = "Seeds synthetic clipboard data into explicitly supplied UI fixture home"]
    fn seed_ui_history() {
        let home = PathBuf::from(std::env::var_os("VECTORDASH_CLIPBOARD_TEST_HOME").unwrap());
        std::fs::create_dir_all(&home).unwrap();
        let store = Clipboard::new(&home);
        let now = chrono::Utc::now().timestamp();
        store
            .record(
                "Clipboard fixture: a saved note\nThis full text appears in the preview.",
                24,
                now - 30,
            )
            .unwrap();
        store.record_image(&png(), 24, now).unwrap();
    }
}

fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let (dot, aa, bb) = a
        .iter()
        .zip(b)
        .fold((0.0_f32, 0.0_f32, 0.0_f32), |(dot, aa, bb), (&a, &b)| {
            (dot + a * b, aa + a * a, bb + b * b)
        });
    let norm = (aa * bb).sqrt();
    if !norm.is_finite() || norm == 0.0 {
        None
    } else {
        Some(dot / norm)
    }
}

#[cfg(test)]
mod semantic_clipboard_tests {
    use super::*;
    #[test]
    fn image_meaning_is_opt_in_and_expires_with_history() {
        let home = tempfile::tempdir().unwrap();
        let store = Clipboard::new(home.path());
        let mut image = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(24, 24)
            .write_to(&mut image, image::ImageFormat::Png)
            .unwrap();
        let now = chrono::Utc::now().timestamp();
        store.record_image(image.get_ref(), 1, now).unwrap();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let s = crate::config::Settings {
            clipboard_enabled: true,
            clipboard_image_embeddings: true,
            clipboard_hours: 1,
            remote_url: format!("http://{}", server.server_addr()),
            check_cpu: false,
            check_gpu: false,
            check_user_idle: false,
            ac_only: false,
            ..Default::default()
        };
        let thread = std::thread::spawn(move || {
            for _ in 0..2 {
                let request = server
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap()
                    .expect("embedding request");
                assert_eq!(request.url(), "/v1/embeddings");
                let body = r#"{"data":[{"index":0,"embedding":[1,0]}]}"#;
                request
                    .respond(tiny_http::Response::from_string(body))
                    .unwrap();
            }
        });
        let models = crate::models::Models::default();
        assert!(store.index_next_image(&s, &models).unwrap());
        assert!(
            !store.index_next_image(&s, &models).unwrap(),
            "Already-indexed image must not be embedded again"
        );
        store.record("Dogs playing outside", 1, now).unwrap();
        let hits = store.search_with_images("dogs", &s, &models).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(
            !hits[0].image,
            "Literal text must outrank semantic image matches"
        );
        assert!(hits[1].image);
        assert!(hits[1].detail.contains("Image meaning"));
        assert_eq!(store.search("outside dogs", 1).unwrap().len(), 1);
        assert!(
            store
                .search_with_images("images:dogs", &s, &models)
                .unwrap()
                .iter()
                .any(|i| i.image)
        );
        let image_id = hits[1].id;
        store.pin(image_id, true).unwrap();
        assert!(
            store
                .search_with_images("saved:dogs", &s, &models)
                .unwrap()
                .iter()
                .any(|i| i.image)
        );
        store.pin(image_id, false).unwrap();
        let disabled = crate::config::Settings {
            clipboard_image_embeddings: false,
            ..s
        };
        assert!(
            store
                .search_with_images("dogs", &disabled, &models)
                .unwrap()
                .iter()
                .all(|item| !item.image)
        );
        thread.join().unwrap();
        let c = store.connect().unwrap();
        let payload: String = c
            .query_row("SELECT embedding FROM clips", [], |r| r.get(0))
            .unwrap();
        assert_ne!(
            payload, "[1.0,0.0]",
            "Vector must use the clipboard's protected payload format"
        );
        drop(c);
        store.purge(1, now + 3601).unwrap();
        assert_eq!(
            store
                .connect()
                .unwrap()
                .query_row("SELECT count(*) FROM clips", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[cfg(test)]
mod manager_tests {
    use super::*;
    #[test]
    fn saved_items_survive_expiry_count_and_byte_cleanup_and_are_searchable() {
        let home = tempfile::tempdir().unwrap();
        let mut s = crate::config::Settings::default();
        s.clipboard_options.max_items = 1;
        let store = Clipboard::new(home.path()).configured(&s);
        let now = chrono::Utc::now().timestamp();
        store.record("keep permanently", 0, now - 10000).unwrap();
        let id = store.search("keep", 0).unwrap()[0].id;
        store.pin(id, true).unwrap();
        store.annotate(id, "Useful name", "work,notes").unwrap();
        store.record("temporary one", 1, now).unwrap();
        store.record("temporary two", 1, now + 1).unwrap();
        store.purge(1, now + 7200).unwrap();
        assert_eq!(store.search("saved:Useful work", 1).unwrap().len(), 1);
        assert_eq!(store.stats().unwrap().0, 1);
        store.clear_recent(None).unwrap();
        assert!(store.pinned(id).unwrap());
        store.pin(id, false).unwrap();
        store.purge(1, now + 7200).unwrap();
        assert_eq!(store.stats().unwrap().0, 0);
    }
    #[test]
    fn forever_retention_deduplicates_and_uses_configured_history_limit() {
        let home = tempfile::tempdir().unwrap();
        let mut s = crate::config::Settings::default();
        s.clipboard_options.max_items = 2;
        let store = Clipboard::new(home.path()).configured(&s);
        let now = chrono::Utc::now().timestamp();
        store.record("old forever", 0, now - 100000000).unwrap();
        store.record("new", 0, now).unwrap();
        assert_eq!(store.search("", 0).unwrap().len(), 2);
        store.record("new", 0, now + 1).unwrap();
        assert_eq!(store.stats().unwrap().0, 2);
        store.record("newest", 0, now + 2).unwrap();
        assert_eq!(store.stats().unwrap().0, 2);
    }
    #[test]
    fn streaming_backup_roundtrip_protects_imported_data_and_preserves_metadata() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let store = Clipboard::new(a.path());
        let now = chrono::Utc::now().timestamp();
        store.record("backup-private-fixture", 0, now).unwrap();
        let id = store.search("", 0).unwrap()[0].id;
        store.pin(id, true).unwrap();
        store.annotate(id, "saved fixture", "tagged").unwrap();
        let file = a.path().join("saved.jsonl");
        store.export_saved(&file).unwrap();
        let dest = Clipboard::new(b.path());
        dest.import_saved(&file).unwrap();
        assert_eq!(dest.search("saved:tagged", 0).unwrap().len(), 1);
        let bytes = std::fs::read(b.path().join("clipboard.sqlite")).unwrap();
        assert!(!bytes.windows(22).any(|v| v == b"backup-private-fixture"));
        let file = a.path().join("bad.jsonl");
        std::fs::write(&file, "{bad json}").unwrap();
        assert!(dest.import_saved(&file).is_err());
        assert_eq!(dest.stats().unwrap().0, 1);
    }
    #[test]
    fn edited_text_remains_saved_and_limits_are_enforced() {
        let home = tempfile::tempdir().unwrap();
        let store = Clipboard::new(home.path());
        let now = chrono::Utc::now().timestamp();
        store.record("original", 0, now).unwrap();
        let id = store.search("", 0).unwrap()[0].id;
        store.pin(id, true).unwrap();
        store.edit(id, "changed").unwrap();
        assert_eq!(
            store.search("saved:changed", 0).unwrap()[0].value,
            "changed"
        );
        assert!(store.edit(id, &"x".repeat(17 * 1024)).is_err());
        assert_eq!(store.stats().unwrap().0, 1);
    }
}

#[cfg(all(test, windows))]
mod manager_ui_fixture {
    #[test]
    #[ignore = "Writes synthetic clips only to an explicitly supplied fixture home"]
    fn seed_manager_fixture() {
        let home =
            std::path::PathBuf::from(std::env::var("VECTORDASH_CLIPBOARD_TEST_HOME").unwrap());
        std::fs::create_dir_all(&home).unwrap();
        let store = super::Clipboard::new(&home);
        let now = chrono::Utc::now().timestamp();
        store.record("A saved clipboard fixture", 0, now).unwrap();
        let id = store.search("", 0).unwrap()[0].id;
        store.pin(id, true).unwrap();
        store.annotate(id, "Reusable greeting", "work").unwrap();
        store.record("Ordinary copied text", 0, now + 1).unwrap();
    }
}

#[cfg(test)]
mod clipboard_limit_tests {
    use super::*;
    #[test]
    fn image_retention_is_independent_and_saved_capacity_rejects_instead_of_evicting() {
        let home = tempfile::tempdir().unwrap();
        let mut s = crate::config::Settings::default();
        s.clipboard_options.image_hours = Some(1);
        s.clipboard_options.max_saved_mb = 1;
        let store = Clipboard::new(home.path()).configured(&s);
        let now = chrono::Utc::now().timestamp();
        store.record("retained text", 24, now - 7200).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        store.record_image(png.get_ref(), 24, now - 7200).unwrap();
        store.purge(24, now).unwrap();
        assert_eq!(store.search("", 24).unwrap().len(), 1);
        let id = store.search("", 24).unwrap()[0].id;
        store
            .connect()
            .unwrap()
            .execute("UPDATE clips SET bytes=2097152 WHERE id=?1", [id])
            .unwrap();
        assert!(store.pin(id, true).is_err());
        assert_eq!(store.stats().unwrap().0, 1);
    }
}

#[cfg(test)]
mod pin_order_tests {
    use super::*;
    #[test]
    fn saving_does_not_reorder_but_pinning_does_and_backup_retains_it() {
        let home = tempfile::tempdir().unwrap();
        let store = Clipboard::new(home.path());
        let now = chrono::Utc::now().timestamp();
        store.record("older saved", 0, now - 10).unwrap();
        let id = store.search("", 0).unwrap()[0].id;
        store.pin(id, true).unwrap();
        store.record("newer ordinary", 0, now).unwrap();
        assert_ne!(store.search("", 0).unwrap()[0].id, id);
        assert_eq!(store.search("saved:", 0).unwrap()[0].id, id);
        store.stick(id, true).unwrap();
        assert_eq!(store.search("", 0).unwrap()[0].id, id);
        assert_eq!(store.search("pinned:", 0).unwrap()[0].id, id);
        let backup = home.path().join("saved.jsonl");
        store.export_saved(&backup).unwrap();
        let other = tempfile::tempdir().unwrap();
        let target = Clipboard::new(other.path());
        target.import_saved(&backup).unwrap();
        assert_eq!(target.search("pinned:", 0).unwrap().len(), 1);
        store.stick(id, false).unwrap();
        assert!(store.search("pinned:", 0).unwrap().is_empty());
        assert_eq!(store.search("saved:", 0).unwrap().len(), 1);
    }
}
