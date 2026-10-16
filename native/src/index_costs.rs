use crate::{config::Settings, content};
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::path::Path;
fn bucket(s: &Settings) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}|{}|{}|{}|{}|{}",
                s.model,
                s.remote_url,
                s.remote_model,
                s.hardware,
                s.device,
                s.backend()
            )
            .as_bytes()
        )
    )
}
fn kind(path: &str) -> &'static str {
    let e = content::extension(Path::new(path));
    if content::is_image(&e) {
        "image"
    } else if content::is_video(&e) {
        "video"
    } else if content::is_audio(&e) {
        "audio"
    } else if e == "pdf" {
        "pdf"
    } else {
        "text"
    }
}
fn units(kind: &str, size: u64) -> f64 {
    if kind == "image" {
        1.0
    } else {
        (size as f64 / 1024.0).max(1.0)
    }
}
pub fn tables(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS index_costs(backend TEXT,kind TEXT,seconds REAL,units REAL,samples INTEGER,PRIMARY KEY(backend,kind));CREATE TABLE IF NOT EXISTS index_job_time(path TEXT PRIMARY KEY,seconds REAL);")?;
    Ok(())
}
pub fn add_time(c: &Connection, path: &str, seconds: f64) -> Result<()> {
    c.execute("INSERT INTO index_job_time VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET seconds=seconds+excluded.seconds",params![path,seconds.max(0.0)])?;
    Ok(())
}
pub fn finish(c: &Connection, s: &Settings, path: &str, size: u64, seconds: f64) -> Result<()> {
    let previous: f64 = c
        .query_row(
            "SELECT seconds FROM index_job_time WHERE path=?1",
            [path],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0.0);
    let time = previous + seconds;
    let kind = kind(path);
    if time.is_finite() && time > 0.0 {
        c.execute("INSERT INTO index_costs VALUES(?1,?2,?3,?4,1) ON CONFLICT(backend,kind) DO UPDATE SET seconds=seconds*0.95+excluded.seconds,units=units*0.95+excluded.units,samples=samples+1",params![bucket(s),kind,time,units(kind,size)])?;
    }
    c.execute("DELETE FROM index_job_time WHERE path=?1", [path])?;
    Ok(())
}
pub fn estimate(c: &Connection, s: &Settings) -> Result<Option<f64>> {
    let mut rates = std::collections::HashMap::new();
    let mut q = c.prepare_cached(
        "SELECT kind,seconds/units FROM index_costs WHERE backend=?1 AND samples>=3 AND units>0",
    )?;
    for row in q.query_map([bucket(s)], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
    })? {
        let (k, v) = row?;
        rates.insert(k, v);
    }
    if rates.is_empty() {
        return Ok(None);
    }
    crate::db::register_search(c, s, "All files", "", "", None)?;
    let mut q=c.prepare_cached("SELECT n.path,n.size,coalesce(t.seconds,0) FROM native_jobs j JOIN names n ON n.path=j.path LEFT JOIN index_job_time t ON t.path=j.path WHERE j.error='' AND n.index_mode='full' AND vd_match(n.path,'text',n.mtime,n.size) LIMIT 2048")?;
    let mut total = 0.0;
    let mut count = 0;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, f64>(2)?,
        ))
    })? {
        let (path, size, spent) = row?;
        let k = kind(&path);
        let Some(rate) = rates.get(k) else {
            return Ok(None);
        };
        total += (units(k, size.max(0) as u64) * rate - spent).max(rate * 0.1);
        count += 1;
    }
    if count == 0 {
        return Ok(None);
    }
    let all: u64 = c.query_row("SELECT count(*) FROM native_jobs j JOIN names n ON n.path=j.path WHERE j.error='' AND n.index_mode='full' AND vd_match(n.path,'text',n.mtime,n.size)", [], |r| {
        r.get(0)
    })?;
    Ok(Some(total * all as f64 / count as f64))
}
