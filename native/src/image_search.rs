//! Image query preprocessing and bounded visual retrieval; no second index/service.
use crate::{config::Settings, db::Hit, engine::Engine};
use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
pub enum Input {
    File(PathBuf),
    Rgba {
        width: u32,
        height: u32,
        bytes: Vec<u8>,
    },
}
struct Task {
    input: Input,
    settings: Settings,
    generation: Arc<AtomicU64>,
    id: u64,
    tx: crate::engine::NotifySender,
}
pub fn prepare(
    input: Input,
    settings: Settings,
    generation: Arc<AtomicU64>,
    id: u64,
    tx: crate::engine::NotifySender,
) {
    static QUEUE: OnceLock<(
        crossbeam_channel::Sender<Task>,
        crossbeam_channel::Receiver<Task>,
    )> = OnceLock::new();
    let (sender, pending) = QUEUE.get_or_init(|| {
        let (sender, receiver) = crossbeam_channel::bounded::<Task>(1);
        let worker = receiver.clone();
        std::thread::spawn(move || {
            while let Ok(task) = worker.recv() {
                if task.generation.load(Ordering::Acquire) != task.id {
                    continue;
                }
                let result = (|| -> Result<Vec<u8>> {
                    let image = match task.input {
                        Input::File(path) => {
                            anyhow::ensure!(
                                crate::content::is_image(&crate::content::extension(&path)),
                                "Drop an image file to search"
                            );
                            crate::content::decode(
                                &crate::platform::preview_path(&path.to_string_lossy()),
                                &task.settings,
                            )?
                        }
                        Input::Rgba {
                            width,
                            height,
                            bytes,
                        } => {
                            anyhow::ensure!(
                                (width as u64) * (height as u64) <= 16_000_000,
                                "Image exceeds the 16 megapixel clipboard limit"
                            );
                            image::DynamicImage::ImageRgba8(
                                image::RgbaImage::from_raw(width, height, bytes)
                                    .context("Invalid clipboard image")?,
                            )
                        }
                    };
                    crate::content::jpeg(
                        &image,
                        if task.settings.visual_detail == "Detailed" {
                            1536
                        } else {
                            1024
                        },
                    )
                })()
                .map_err(|e| e.to_string());
                if task.generation.load(Ordering::Acquire) == task.id {
                    let _ = task
                        .tx
                        .send(crate::engine::Event::ImageQueryReady(task.id, result));
                }
            }
        });
        (sender, receiver)
    });
    while pending.try_recv().is_ok() {}
    let _ = sender.try_send(Task {
        input,
        settings,
        generation,
        id,
        tx,
    });
}
/// Collapse to best segment per file before truncation, so one long video cannot
/// consume all candidate slots; at most 512 paths are held by the collector.
fn trim(pool: &mut Vec<(f32, i64, String)>) {
    let mut best = HashMap::<String, (f32, i64)>::new();
    for (score, id, path) in pool.drain(..) {
        let old = best.entry(path).or_insert((score, id));
        if score > old.0 || score == old.0 && id < old.1 {
            *old = (score, id);
        }
    }
    pool.extend(
        best.into_iter()
            .map(|(path, (score, id))| (score, id, path)),
    );
    pool.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.2.cmp(&b.2))
            .then(a.1.cmp(&b.1))
    });
    pool.truncate(100);
}
impl Engine {
    pub fn has_visual_vectors(&self) -> Result<bool> {
        Ok(self.db.connect(true)?.query_row("SELECT EXISTS(SELECT 1 FROM segment_vectors WHERE namespace='files' AND channel IN ('image','video','pdf'))",[],|r|r.get(0))?)
    }
    pub fn image_matches(
        &self,
        vector: &[f32],
        s: &Settings,
        category: &str,
        scope: &str,
        excluded: &str,
        checkpoint: &dyn Fn() -> Result<()>,
    ) -> Result<Vec<Hit>> {
        let conn = self.db.connect(true)?;
        let dimension:Option<usize>=conn.query_row("SELECT dimension FROM segment_vectors WHERE namespace='files' AND channel IN ('image','video','pdf') LIMIT 1",[],|r|r.get(0)).optional()?;
        anyhow::ensure!(
            dimension.is_some(),
            "No visual embeddings found. Full-index images with image embeddings enabled first."
        );
        let compatible:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM segment_vectors WHERE namespace='files' AND channel IN ('image','video','pdf') AND dimension=?1)",[vector.len()],|r|r.get(0))?;
        anyhow::ensure!(
            compatible,
            "Image embedding dimensions differ from the index. Select a compatible embedding model."
        );
        let policy = crate::config::SearchPolicy::new(s, category, scope, excluded);
        let allowed = |path: &str, kind: &str, channel: &str, namespace: &str| {
            namespace == "files"
                && matches!(channel, "image" | "video" | "pdf")
                && policy.allowed(path, kind)
        };
        let mut pool = Vec::new();
        if let Some(matrix) =
            self.vectors
                .get(&conn, vector.len(), &s.performance, &s.cache_precision)?
        {
            let groups = matrix
                .groups
                .iter()
                .map(|g| allowed(&g.path, &g.kind, &g.channel, &g.namespace))
                .collect::<Vec<_>>();
            for i in 0..matrix.ids.len() {
                if i % 256 == 0 {
                    checkpoint()?;
                }
                let group = matrix.group_ids[i];
                if !groups[group] {
                    continue;
                }
                let score = matrix.score(i, vector);
                if !score.is_finite() {
                    continue;
                }
                pool.push((score, matrix.ids[i], matrix.groups[group].path.clone()));
                if pool.len() >= 512 {
                    trim(&mut pool);
                }
            }
        } else {
            let mut stmt=conn.prepare("SELECT id,path,kind,channel,vector FROM segment_vectors WHERE namespace='files' AND dimension=?1 AND channel IN ('image','video','pdf')")?;
            let mut rows = stmt.query([vector.len()])?;
            let mut count = 0;
            while let Some(row) = rows.next()? {
                count += 1;
                if count % 256 == 0 {
                    checkpoint()?;
                }
                let path: String = row.get(1)?;
                let kind: String = row.get(2)?;
                let channel: String = row.get(3)?;
                if !allowed(&path, &kind, &channel, "files") {
                    continue;
                }
                let blob: Vec<u8> = row.get(4)?;
                anyhow::ensure!(
                    blob.len() == vector.len() * 4,
                    "Invalid visual embedding length"
                );
                let values = blob
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                    .collect::<Vec<_>>();
                let score = crate::vector_cache::dot(&values, vector);
                if !score.is_finite() {
                    continue;
                }
                pool.push((score, row.get(0)?, path));
                if pool.len() >= 512 {
                    trim(&mut pool);
                }
            }
        }
        checkpoint()?;
        trim(&mut pool);
        let scores = pool
            .iter()
            .map(|(v, id, _)| (*id, *v))
            .collect::<HashMap<_, _>>();
        let ids = serde_json::to_string(&pool.iter().map(|(_, id, _)| *id).collect::<Vec<_>>())?;
        let mut stmt=conn.prepare("SELECT rowid,path,kind,text,location FROM segments WHERE rowid IN (SELECT value FROM json_each(?1))")?;
        let mut rows = stmt.query([ids])?;
        let mut hits = Vec::new();
        while let Some(row) = rows.next()? {
            checkpoint()?;
            let path: String = row.get(1)?;
            let text: String = row.get(3)?;
            if crate::platform::missing_local_file(&path)
                || s.excluded_sensitive && crate::engine::sensitive(&text)
            {
                continue;
            }
            let loc: serde_json::Value = serde_json::from_str(&row.get::<_, String>(4)?)?;
            hits.push(Hit {
                path,
                kind: row.get(2)?,
                text,
                score: scores[&row.get::<_, i64>(0)?],
                reason: "Visually similar to your image".into(),
                page: loc["page"].as_u64().unwrap_or(0) as u32,
                seconds: loc["seconds"].as_f64().unwrap_or(-1.0) as f32,
                line: 0,
            });
        }
        hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.path.cmp(&b.path)));
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Source,
        db::Database,
        engine::{Control, NotifySender},
        models::Models,
    };
    use std::sync::atomic::AtomicBool;
    #[test]
    fn visual_ranking_filters_precision_and_cancellation() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("photos");
        std::fs::create_dir(&root).unwrap();
        let mut s = Settings {
            encrypt_index: false,
            ..Default::default()
        };
        s.roots = vec![Source {
            path: root.to_string_lossy().into(),
            mode: "full".into(),
        }];
        let db = Database::new(home.path(), &s).unwrap();
        let c = db.connect(false).unwrap();
        for (name, channel, kind, x) in [
            ("actual.png", "image", "image", 1f32),
            ("dogwords.png", "image", "image", 0.4),
            ("text.png", "text", "image", 1.0),
            ("movie.mp4", "video", "video", 0.8),
        ] {
            let path = root.join(name);
            std::fs::write(&path, b"fixture").unwrap();
            Database::touch_name(&c, &path, "file", 1, 7, "full", &root.to_string_lossy(), 1)
                .unwrap();
            c.execute(
                "INSERT INTO segments VALUES(?1,'files',?2,?3,'[]',?4)",
                params![
                    path.to_string_lossy(),
                    kind,
                    if name == "dogwords.png" {
                        "dog photo dogs image"
                    } else {
                        ""
                    },
                    "{\"embedding_source\":\"media\"}"
                ],
            )
            .unwrap();
            let id = c.last_insert_rowid();
            let blob = [x, (1. - x * x).sqrt()]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>();
            c.execute(
                "INSERT INTO segment_vectors VALUES(?1,'files',?2,?3,2,?4,?5)",
                params![id, kind, channel, blob, path.to_string_lossy()],
            )
            .unwrap();
        }
        let (tx, _) = crossbeam_channel::unbounded();
        let engine = Engine {
            db,
            models: Models::default(),
            control: Control::default(),
            tx: NotifySender::new(tx, None),
            searching: Arc::new(AtomicBool::new(false)),
            vectors: Default::default(),
        };
        for (profile, precision) in [
            ("Balanced", "FP32"),
            ("Balanced", "FP16"),
            ("Low memory", "FP32"),
        ] {
            s.performance = profile.into();
            s.cache_precision = precision.into();
            let hits = engine
                .image_matches(&[1., 0.], &s, "Images", "", "", &|| Ok(()))
                .unwrap();
            assert_eq!(hits.len(), 2);
            assert!(hits[0].path.ends_with("actual.png"));
            let hits = engine
                .image_matches(&[1., 0.], &s, "Videos", "", "", &|| Ok(()))
                .unwrap();
            assert_eq!(hits.len(), 1);
            assert!(
                engine
                    .image_matches(&[1., 0.], &s, "Images", "", "*.png", &|| Ok(()))
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(
            engine
                .image_matches(&[1., 0., 0.], &s, "Images", "", "", &|| Ok(()))
                .is_err()
        );
        assert!(
            engine
                .image_matches(&[1., 0.], &s, "Images", "", "", &|| anyhow::bail!(
                    "cancelled"
                ))
                .is_err()
        );
        s.roots[0].mode = "quick".into();
        let cancel = Arc::new(AtomicBool::new(false));
        let result = engine
            .ai_local_search("name:actual.png", &s, &cancel)
            .unwrap();
        assert_eq!(result["results"].as_array().unwrap().len(), 1);
        s.exclusions.push(root.to_string_lossy().into());
        assert!(
            engine
                .ai_local_search("name:actual.png", &s, &cancel)
                .unwrap()["results"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        s.exclusions.clear();
        cancel.store(true, Ordering::Release);
        assert!(
            engine
                .ai_local_search("name:actual.png", &s, &cancel)
                .is_err()
        );

        assert!(
            engine
                .image_matches(&[1., 0.], &s, "Images", "", "", &|| Ok(()))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn long_video_segments_cannot_hide_other_files() {
        let mut pool = (0..700)
            .map(|i| (1f32, i, "video.mp4".into()))
            .collect::<Vec<_>>();
        pool.extend((0..110).map(|i| (0.9, 1000 + i, format!("photo{i:03}.png"))));
        trim(&mut pool);
        assert_eq!(pool.len(), 100);
        assert_eq!(pool.iter().filter(|x| x.2 == "video.mp4").count(), 1);
    }
}
