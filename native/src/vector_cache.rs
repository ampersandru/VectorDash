use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
#[derive(Clone)]
pub struct Group {
    pub path: String,
    pub kind: String,
    pub channel: String,
    pub namespace: String,
}
pub struct Matrix {
    pub revision: i64,
    pub dimension: usize,
    pub ids: Vec<i64>,
    pub groups: Vec<Group>,
    pub group_ids: Vec<usize>,
    pub values: Vec<f32>,
    pub bytes: usize,
}
#[derive(Clone, Default)]
pub struct Cache {
    inner: Arc<Mutex<Option<Arc<Matrix>>>>,
}
impl Cache {
    pub fn clear(&self) {
        if let Ok(mut entry) = self.inner.lock() {
            *entry = None;
        }
    }
    pub fn get(
        &self,
        db: &Connection,
        dimension: usize,
        profile: &str,
    ) -> Result<Option<Arc<Matrix>>> {
        let limit = match profile {
            "Low memory" => 0,
            "Fast search" => 512 * 1024 * 1024,
            _ => 128 * 1024 * 1024,
        };
        if limit == 0 {
            self.clear();
            return Ok(None);
        }
        let revision = db
            .query_row(
                "SELECT cast(value AS INTEGER) FROM metadata WHERE key='native_vectors_generation'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);
        let mut lock = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Vector cache unavailable"))?;
        if let Some(matrix) = lock
            .as_ref()
            .filter(|m| m.revision == revision && m.dimension == dimension && m.bytes <= limit)
        {
            return Ok(Some(matrix.clone()));
        }
        let(count,estimated):(usize,usize)=db.query_row("SELECT count(*),coalesce(sum(length(vector)+length(path)+length(kind)+length(channel)+64),0) FROM segment_vectors WHERE dimension=?1",[dimension],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if count == 0 || estimated > limit {
            return Ok(None);
        }
        let mut matrix = Matrix {
            revision,
            dimension,
            ids: Vec::with_capacity(count),
            groups: vec![],
            group_ids: Vec::with_capacity(count),
            values: Vec::with_capacity(count * dimension),
            bytes: 0,
        };
        let mut lookup = HashMap::new();
        let mut stmt=db.prepare("SELECT id,path,kind,channel,namespace,vector FROM segment_vectors WHERE dimension=?1 ORDER BY id")?;
        let mut rows = stmt.query([dimension])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            let kind: String = row.get(2)?;
            let channel: String = row.get(3)?;
            let namespace: String = row.get(4)?;
            let key = (
                path.clone(),
                kind.clone(),
                channel.clone(),
                namespace.clone(),
            );
            let group = *lookup.entry(key).or_insert_with(|| {
                let i = matrix.groups.len();
                matrix.bytes += path.len() + kind.len() + channel.len() + namespace.len() + 128;
                matrix.groups.push(Group {
                    path,
                    kind,
                    channel,
                    namespace,
                });
                i
            });
            let blob: Vec<u8> = row.get(5)?;
            anyhow::ensure!(
                blob.len() == dimension * 4,
                "Invalid vector length in index"
            );
            matrix.values.extend(
                blob.chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            );
            matrix.ids.push(id);
            matrix.group_ids.push(group);
            matrix.bytes += dimension * 4 + 16;
            if matrix.bytes > limit {
                return Ok(None);
            }
        }
        let matrix = Arc::new(matrix);
        *lock = Some(matrix.clone());
        Ok(Some(matrix))
    }
}
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut accum = [0.0f32; 8];
    let mut total = 0.0;
    let mut i = 0;
    while i + 8 <= a.len().min(b.len()) {
        for lane in 0..8 {
            accum[lane] += a[i + lane] * b[i + lane];
        }
        i += 8;
    }
    for j in i..a.len().min(b.len()) {
        total += a[j] * b[j];
    }
    total + accum.iter().sum::<f32>()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dot_matches_scalar() {
        let a = (0..768).map(|i| i as f32 / 768.0).collect::<Vec<_>>();
        let b = (0..768)
            .map(|i| (768 - i) as f32 / 768.0)
            .collect::<Vec<_>>();
        let scalar: f32 = a.iter().zip(&b).map(|(a, b)| a * b).sum();
        assert!((dot(&a, &b) - scalar).abs() < 0.001);
    }
}
#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::{config::Settings, content::Segment, db::Database};
    #[test]
    fn revision_and_limits() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(
            dir.path(),
            &Settings {
                encrypt_index: false,
                ..Default::default()
            },
        )
        .unwrap();
        let c = db.connect(false).unwrap();
        let segment = Segment::text("text", "hello".into(), 0, 1);
        Database::save_segments(
            &c,
            "C:\\note.txt",
            &[segment.clone()],
            &[vec![1.0, 0.0]],
            "first",
        )
        .unwrap();
        let cache = Cache::default();
        assert!(cache.get(&c, 2, "Low memory").unwrap().is_none());
        let a = cache.get(&c, 2, "Balanced").unwrap().unwrap();
        assert_eq!(a.ids.len(), 1);
        assert!(a.bytes <= 128 * 1024 * 1024);
        let again = cache.get(&c, 2, "Balanced").unwrap().unwrap();
        assert!(Arc::ptr_eq(&a, &again));
        Database::save_segments(&c, "C:\\note.txt", &[segment], &[vec![0.0, 1.0]], "second")
            .unwrap();
        let b = cache.get(&c, 2, "Balanced").unwrap().unwrap();
        assert_ne!(a.revision, b.revision);
        assert_eq!(b.values, vec![0.0, 1.0]);
    }
}
