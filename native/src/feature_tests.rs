use crate::{
    config::{Settings, Source},
    db::Database,
    engine::{Control, Engine},
    models::Models,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
fn engine(home: &std::path::Path, s: &Settings) -> Engine {
    let (tx, _) = crossbeam_channel::unbounded();
    Engine {
        db: Database::new(home, s).unwrap(),
        models: Models::default(),
        control: Control::default(),
        tx: tx.into(),
        searching: Arc::new(AtomicBool::new(false)),
        vectors: Default::default(),
    }
}
struct Mock {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    inputs: Arc<std::sync::Mutex<Vec<String>>>,
}
impl Mock {
    fn start(s: &mut Settings, observe: impl Fn(&str) + Send + 'static) -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        s.embedding_server = "Remote".into();
        s.remote_url = format!("http://{}", server.server_addr());
        let stop = Arc::new(AtomicBool::new(false));
        let inputs = Arc::new(std::sync::Mutex::new(vec![]));
        let (done, log) = (stop.clone(), inputs.clone());
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::Acquire) {
                let Some(mut request) = server.recv_timeout(Duration::from_millis(50)).unwrap()
                else {
                    continue;
                };
                let mut text = String::new();
                request.as_reader().read_to_string(&mut text).unwrap();
                let body: serde_json::Value = serde_json::from_str(&text).unwrap();
                observe(&text);
                log.lock().unwrap().push(text);
                let n = body["input"].as_array().map_or(1, Vec::len);
                let response = serde_json::json!({"data":(0..n).map(|i|serde_json::json!({"index":i,"embedding":[1.0,0.0]})).collect::<Vec<_>>()});
                let _ = request.respond(tiny_http::Response::from_string(response.to_string()));
            }
        });
        Self {
            stop,
            thread: Some(thread),
            inputs,
        }
    }
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn settings() -> Settings {
    Settings {
        encrypt_index: false,
        include_programs: false,
        count_files: false,
        check_cpu: false,
        check_gpu: false,
        check_user_idle: false,
        pause_for_game: false,
        pause_for_fullscreen: false,
        ..Default::default()
    }
}
#[test]
fn quick_and_full_priorities_are_applied_before_embedding_and_do_not_lose_names() {
    for order in ["Quick first", "Full first"] {
        let home = tempfile::tempdir().unwrap();
        let source = tempfile::Builder::new()
            .prefix("VD-order-")
            .tempdir()
            .unwrap();
        let quick = source.path().join("quick");
        let full = source.path().join("full");
        std::fs::create_dir(&quick).unwrap();
        std::fs::create_dir(&full).unwrap();
        let quick_file = quick.join("ordinary.txt");
        std::fs::write(&quick_file, "quick").unwrap();
        std::fs::write(full.join("document.txt"), "full content").unwrap();
        let mut s = settings();
        s.index_order = order.into();
        s.roots = vec![
            Source {
                path: quick.to_string_lossy().into(),
                mode: "quick".into(),
            },
            Source {
                path: full.to_string_lossy().into(),
                mode: "full".into(),
            },
        ];
        let checked = Arc::new(AtomicBool::new(false));
        let marker = checked.clone();
        let dbpath = home.path().join("index.sqlite");
        let expected = order == "Quick first";
        let _mock = Mock::start(&mut s, move |_| {
            let c = rusqlite::Connection::open(&dbpath).unwrap();
            let found: i64 = c
                .query_row(
                    "SELECT count(*) FROM names WHERE path=?1",
                    [quick_file.to_string_lossy().as_ref()],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(found > 0, expected);
            marker.store(true, Ordering::Release);
        });
        let e = engine(home.path(), &s);
        e.index_inner(&s, false).unwrap();
        assert!(checked.load(Ordering::Acquire));
        let c = e.db.connect(false).unwrap();
        let count: i64 = c
            .query_row("SELECT count(*) FROM names WHERE kind='file'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
        let pending: i64 = c
            .query_row("SELECT count(*) FROM native_quick_queue", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pending, 0);
    }
}
#[test]
fn long_file_yields_to_small_files_and_finishes_with_unique_vectors() {
    let home = tempfile::tempdir().unwrap();
    let source = tempfile::Builder::new()
        .prefix("VD-fair-")
        .tempdir()
        .unwrap();
    let long = source.path().join("long.txt");
    let short = source.path().join("short.txt");
    let body = "LONGMARKER seasonal garden plants and flowers. ".repeat(6000);
    std::fs::write(&long, &body).unwrap();
    std::fs::write(&short, "SMALLMARKER invoice document").unwrap();
    let mut s = settings();
    s.roots = vec![
        Source {
            path: long.to_string_lossy().into(),
            mode: "full".into(),
        },
        Source {
            path: short.to_string_lossy().into(),
            mode: "full".into(),
        },
    ];
    let visible = Arc::new(AtomicBool::new(false));
    let marker = visible.clone();
    let path = long.clone();
    let dbpath = home.path().join("index.sqlite");
    let mock = Mock::start(&mut s, move |input| {
        if input.contains("SMALLMARKER") {
            let c = rusqlite::Connection::open(&dbpath).unwrap();
            let count: i64 = c
                .query_row(
                    "SELECT count(*) FROM native_staging WHERE token IN (SELECT token FROM native_jobs WHERE path=?1)",
                    [path.to_string_lossy().as_ref()],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(count > 0 && count <= 32);
            let published: i64 = c
                .query_row(
                    "SELECT count(*) FROM segments WHERE path=?1",
                    [path.to_string_lossy().as_ref()],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(published, 0);
            marker.store(true, Ordering::Release);
        }
    });
    let e = engine(home.path(), &s);
    e.index_inner(&s, false).unwrap();
    assert!(visible.load(Ordering::Acquire));
    let inputs = mock.inputs.lock().unwrap();
    let small = inputs
        .iter()
        .position(|x| x.contains("SMALLMARKER"))
        .unwrap();
    assert!(inputs[small + 1..].iter().any(|x| x.contains("LONGMARKER")));
    let c = e.db.connect(false).unwrap();
    let n: i64 = c
        .query_row(
            "SELECT count(*) FROM segment_vectors WHERE path=?1",
            [long.to_string_lossy().as_ref()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n as usize, crate::content::chunks(&body, 1800).len());
    let pending: i64 = c
        .query_row("SELECT count(*) FROM native_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(pending, 0);
}
#[test]
fn fp16_cache_uses_half_storage_preserves_scores_and_does_not_rewrite_index() {
    let home = tempfile::tempdir().unwrap();
    let s = settings();
    let db = Database::new(home.path(), &s).unwrap();
    let c = db.connect(false).unwrap();
    let mut v = (0..768)
        .map(|i| ((i as f32 + 1.0) * 0.17).sin())
        .collect::<Vec<_>>();
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    for x in &mut v {
        *x /= norm
    }
    Database::save_segments(
        &c,
        "sample.txt",
        &[crate::content::Segment::text("text", "sample".into(), 0, 0)],
        &[v.clone()],
        "same",
    )
    .unwrap();
    let cache = crate::vector_cache::Cache::default();
    let full = cache.get(&c, 768, "Balanced", "FP32").unwrap().unwrap();
    let half = cache.get(&c, 768, "Balanced", "FP16").unwrap().unwrap();
    assert!(half.values.is_empty());
    assert_eq!(half.halves.len(), 768);
    assert_eq!(full.values.len(), 768);
    assert!((full.score(0, &v) - half.score(0, &v)).abs() < 0.0001);
    assert!(half.bytes < full.bytes);
    assert_eq!(half.revision, full.revision);
    assert!(cache.get(&c, 768, "Low memory", "FP16").unwrap().is_none());
}
#[test]
fn learned_estimates_are_backend_specific_and_clearable() {
    let home = tempfile::tempdir().unwrap();
    let source = tempfile::Builder::new()
        .prefix("VD-estimate-")
        .tempdir()
        .unwrap();
    let pending = source.path().join("pending.txt");
    std::fs::write(&pending, "fixture").unwrap();
    let mut s = settings();
    s.roots = vec![Source {
        path: source.path().to_string_lossy().into(),
        mode: "full".into(),
    }];
    let db = Database::new(home.path(), &s).unwrap();
    let c = db.connect(false).unwrap();
    for _ in 0..4 {
        crate::index_costs::finish(&c, &s, "done.txt", 1024, 0.2).unwrap();
    }
    Database::touch_name(&c, &pending, "file", 1, 2048, "full", "root", 1).unwrap();
    Database::prepare_job(&c, &pending.to_string_lossy(), "x").unwrap();
    let eta = crate::index_costs::estimate(&c, &s).unwrap().unwrap();
    assert!((eta - 0.4).abs() < 0.001);
    let mut other = s.clone();
    other.model = "different".into();
    assert!(crate::index_costs::estimate(&c, &other).unwrap().is_none());
    db.clear_index().unwrap();
    assert!(crate::index_costs::estimate(&c, &s).unwrap().is_none());
}

#[test]
fn rank_calibration_is_query_time_and_does_not_change_the_index() {
    let home = tempfile::tempdir().unwrap();
    let source = tempfile::Builder::new()
        .prefix("VD-calibration-")
        .tempdir()
        .unwrap();
    let mut s = settings();
    s.roots = vec![Source {
        path: source.path().to_string_lossy().into(),
        mode: "full".into(),
    }];
    let _mock = Mock::start(&mut s, |_| {});
    let e = engine(home.path(), &s);
    let c = e.db.connect(false).unwrap();
    for (name, score, image) in [
        ("picture.png", 0.81f32, true),
        ("strong.txt", 0.795, false),
        ("other1.txt", 0.1, false),
        ("other2.txt", 0.1, false),
    ] {
        let path = source.path().join(name);
        Database::touch_name(&c, &path, "file", 1, 100, "full", &s.roots[0].path, 1).unwrap();
        let mut segment = crate::content::Segment::text(
            if image { "image" } else { "text" },
            "indexed document".into(),
            0,
            0,
        );
        if image {
            segment.media = vec![1];
        }
        Database::save_segments(
            &c,
            &path.to_string_lossy(),
            &[segment],
            &[vec![score, (1.0 - score * score).sqrt()]],
            "unchanged",
        )
        .unwrap();
    }
    let before: i64 = c
        .query_row("SELECT sum(length(vector)) FROM segment_vectors", [], |r| {
            r.get(0)
        })
        .unwrap();
    let first = e
        .search_inner("semanticprobe", &s, "Semantic", "All files", "", "")
        .unwrap();
    assert!(first[0].path.ends_with("picture.png"));
    s.rank_calibration = true;
    let calibrated = e
        .search_inner("semanticprobe", &s, "Semantic", "All files", "", "")
        .unwrap();
    assert!(calibrated[0].path.ends_with("strong.txt"));
    let after: i64 = c
        .query_row("SELECT sum(length(vector)) FROM segment_vectors", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(e.db.stats().unwrap().files, 4);
}

#[test]
fn staged_replacement_retains_old_content_until_the_whole_file_finishes() {
    let home = tempfile::tempdir().unwrap();
    let s = settings();
    let db = Database::new(home.path(), &s).unwrap();
    let c = db.connect(false).unwrap();
    Database::save_segments(
        &c,
        "long.txt",
        &[crate::content::Segment::text(
            "text",
            "old complete file".into(),
            0,
            0,
        )],
        &[vec![1.0, 0.0]],
        "old",
    )
    .unwrap();
    let (token, _) = Database::prepare_job(&c, "long.txt", "new").unwrap();
    Database::stage(
        &c,
        &token,
        0,
        &[crate::content::Segment::text("text", "first".into(), 0, 0)],
        &[vec![1.0, 0.0]],
    )
    .unwrap();
    assert_eq!(
        "old complete file",
        c.query_row::<String, _, _>("SELECT text FROM segments", [], |r| r.get(0))
            .unwrap()
    );
    Database::stage(
        &c,
        &token,
        1,
        &[crate::content::Segment::text("text", "second".into(), 0, 0)],
        &[vec![0.0, 1.0]],
    )
    .unwrap();
    Database::finish_stage(&c, &token, "long.txt", "new").unwrap();
    assert_eq!(
        2,
        c.query_row::<i64, _, _>("SELECT count(*) FROM segments", [], |r| r.get(0))
            .unwrap()
    );
    assert_eq!(
        0,
        c.query_row::<i64, _, _>(
            "SELECT count(*) FROM segments WHERE text='old complete file'",
            [],
            |r| r.get(0)
        )
        .unwrap()
    );
}
#[test]
fn preparation_stop_interrupts_a_paused_pipeline() {
    let control = Control::default();
    control.paused.store(true, Ordering::Relaxed);
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(30));
            stop.store(true, Ordering::Release);
        });
        let start = std::time::Instant::now();
        assert!(
            control
                .check_with(&|| stop.load(Ordering::Acquire))
                .is_err()
        );
        assert!(start.elapsed() < Duration::from_millis(500));
    });
}
