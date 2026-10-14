use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};
pub const MAX_TEXT: usize = 16 * 1024;
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
#[derive(Clone)]
pub struct Clipboard {
    file: PathBuf,
}
impl Clipboard {
    pub fn new(home: &Path) -> Self {
        Self {
            file: home.join("clipboard.sqlite"),
        }
    }
    fn connect(&self) -> Result<Connection> {
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
        Ok(c)
    }
    fn model_key(s: &crate::config::Settings) -> String {
        format!("{}|{}|{}|{}", s.model, s.projector, s.remote_url, s.server)
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
            "UPDATE clips SET embedding=?1,model_key=?2 WHERE id=?3",
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
        let mut items = self.search(q, s.clipboard_hours)?;
        if !q.trim().is_empty() {
            items.sort_by_key(|item| item.image);
        }
        if !s.clipboard_image_embeddings || q.trim().is_empty() || !self.file.exists() {
            return Ok(items);
        }
        let c = self.connect()?;
        let mut stmt=c.prepare("SELECT id,created,embedding FROM clips WHERE kind='image' AND embedding!='' AND model_key=?1 ORDER BY created DESC LIMIT 500")?;
        let rows = stmt
            .query_map([Self::model_key(s)], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.is_empty() {
            return Ok(items);
        }
        let Ok(query) = models.query(s, q.trim().trim_start_matches("clipboard:").trim()) else {
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
        result.truncate(100);
        Ok(result)
    }
    pub fn purge(&self, hours: u32, now: i64) -> Result<()> {
        if !self.file.exists() {
            return Ok(());
        }
        let c = self.connect()?;
        c.execute(
            "DELETE FROM clips WHERE created < ?1",
            [now - (hours.max(1) as i64 * 3600)],
        )?;
        c.execute("DELETE FROM clips WHERE id NOT IN (SELECT id FROM clips ORDER BY created DESC,id DESC LIMIT 500)",[])?;
        let ids: Vec<(i64, u64)> = c
            .prepare("SELECT id,bytes FROM clips ORDER BY created DESC,id DESC")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut total = 0u64;
        for (id, bytes) in ids {
            total += bytes;
            if total > 128 * 1024 * 1024 {
                c.execute("DELETE FROM clips WHERE id=?1", [id])?;
            }
        }
        Ok(())
    }
    pub fn record(&self, text: &str, hours: u32, now: i64) -> Result<()> {
        use sha2::{Digest, Sha256};
        if text.trim().is_empty() || text.len() > MAX_TEXT {
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
        c.execute("INSERT INTO clips(created,payload,digest) VALUES(?1,?2,?3) ON CONFLICT(digest) DO UPDATE SET created=excluded.created,payload=excluded.payload",params![now,protected,digest])?;
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
            "SELECT id,created,CASE WHEN kind=\'image\' THEN \'\' ELSE payload END,kind FROM clips ORDER BY created DESC,id DESC LIMIT 500",
        )?;
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
            ))
        })? {
            let (id, created, payload, kind) = row?;
            let image = kind == "image";
            let value = if image {
                String::new()
            } else {
                crate::config::reveal(&payload)?
            };
            let normalized = value.to_lowercase();
            if q.is_empty()
                || image && "image picture screenshot".contains(&q)
                || words.iter().all(|word| normalized.contains(*word))
            {
                let label = if image {
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
                    label,
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
            bytes.len() <= 4 * 1024 * 1024,
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
        while !semantic_stop.load(std::sync::atomic::Ordering::Relaxed) {
            let s = semantic_settings.lock().unwrap().clone();
            if s.clipboard_enabled
                && s.clipboard_image_embeddings
                && crate::scheduling::idle_allowed(&s, crate::scheduling::metrics()).is_ok()
            {
                match store.index_next_image(&s, &models) {
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
            let (clipboard_enabled, clipboard_hours, clipboard_excluded_apps) = {
                let s = settings.lock().unwrap();
                (
                    s.clipboard_enabled,
                    s.clipboard_hours,
                    s.clipboard_excluded_apps.clone(),
                )
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
                    if let Ok(value) = crate::platform::clipboard_text(&clipboard_excluded_apps) {
                        if value.is_none()
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
                        if let Some(value) = value {
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
            for _ in 0..3 {
                let request = server
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap()
                    .expect("embedding request");
                let body = if request.url() == "/props" {
                    r#"{"media_marker":"<__image__>"}"#
                } else {
                    r#"{"data":[{"index":0,"embedding":[1,0]}]}"#
                };
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
