use crate::{
    config::{Settings, Source},
    db::{Database, Hit},
    engine::{Control, Engine, Event},
    models::Models,
};
use rusqlite::params;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    Settings,
    Engine,
    crossbeam_channel::Receiver<Event>,
) {
    let home = tempfile::tempdir().unwrap();
    let root = tempfile::Builder::new()
        .prefix("VectorDash-stability-")
        .tempdir()
        .unwrap();
    let s = Settings {
        encrypt_index: false,
        roots: vec![Source {
            path: root.path().to_string_lossy().into(),
            mode: "full".into(),
        }],
        include_programs: false,
        ..Default::default()
    };
    let db = Database::new(home.path(), &s).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let engine = Engine {
        db,
        models: Models::default(),
        control: Control::default(),
        tx: tx.into(),
        searching: Arc::new(AtomicBool::new(false)),
        vectors: crate::vector_cache::Cache::default(),
    };
    (home, root, s, engine, rx)
}
fn add(engine: &Engine, s: &Settings, path: &Path, text: &str) {
    let c = engine.db.connect(false).unwrap();
    Database::touch_name(&c, path, "file", 20, 2048, "full", &s.roots[0].path, 1).unwrap();
    Database::save_segments(
        &c,
        &path.to_string_lossy(),
        &[crate::content::Segment::text("text", text.into(), 1, 0)],
        &[vec![1.0, 0.0]],
        "fixture",
    )
    .unwrap();
}
fn names(engine: &Engine, s: &Settings, q: &str) -> Vec<Hit> {
    engine
        .search_inner(q, s, "Names", "All files", "", "")
        .unwrap()
}
#[test]
fn removed_and_replaced_roots_hide_names_and_content_before_cleanup() {
    let (_h, root, mut s, e, _) = fixture();
    let path = root.path().join("scopeprobe.txt");
    add(&e, &s, &path, "a unique identifier");
    assert_eq!(names(&e, &s, "scopeprobe").len(), 1);
    let replacement = tempfile::tempdir().unwrap();
    s.roots[0].path = replacement.path().to_string_lossy().into();
    assert!(names(&e, &s, "scopeprobe").is_empty());
    s.roots.clear();
    assert!(names(&e, &s, "scopeprobe").is_empty());
    assert!(
        e.search_inner("identifier", &s, "Both", "All files", "", "")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        e.db.stats().unwrap().files,
        1,
        "Visibility changes must not depend on destructive cleanup"
    );
}
#[test]
fn new_exclusion_and_quick_override_hide_existing_content() {
    let (_h, root, mut s, e, _) = fixture();
    let path = root.path().join("nested").join("report.txt");
    add(&e, &s, &path, "distinctinvoice");
    s.roots.push(Source {
        path: root.path().join("nested").to_string_lossy().into(),
        mode: "quick".into(),
    });
    assert_eq!(names(&e, &s, "report").len(), 1);
    assert!(!crate::db::allowed(
        &path.to_string_lossy(),
        "text",
        &s,
        "All files",
        "",
        ""
    ));
    s.exclusions
        .push(root.path().join("nested").to_string_lossy().into());
    assert!(names(&e, &s, "report").is_empty());
}
#[test]
fn valid_current_source_is_filtered_before_candidate_limit() {
    let (_h, root, s, e, _) = fixture();
    let c = e.db.connect(false).unwrap();
    for i in 0..450 {
        Database::touch_name(
            &c,
            Path::new(&format!(r"C:\outside\match{i}.txt")),
            "file",
            1,
            1,
            "quick",
            r"C:\outside",
            1,
        )
        .unwrap();
    }
    let inside = root.path().join("match-inside.txt");
    Database::touch_name(&c, &inside, "file", 1, 1, "full", &s.roots[0].path, 1).unwrap();
    assert_eq!(
        names(&e, &s, "match")
            .iter()
            .map(|h| &h.path)
            .collect::<Vec<_>>(),
        vec![&inside.to_string_lossy().to_string()]
    );
}
#[test]
fn hybrid_keeps_names_and_exact_words_when_model_returns_error() {
    let (_h, root, mut s, e, _) = fixture();
    add(
        &e,
        &s,
        &root.path().join("quarterly.txt"),
        "UNIQUEINV87231 confirmed payment",
    );
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    s.remote_url = format!("http://{}", server.server_addr());
    let reply = std::thread::spawn(move || {
        for _ in 0..2 {
            let request = server.recv().unwrap();
            request
                .respond(
                    tiny_http::Response::from_string("{\"error\":\"temporarily offline\"}")
                        .with_status_code(503),
                )
                .unwrap();
        }
    });
    assert_eq!(
        e.search_inner("quarterly", &s, "Both", "All files", "", "")
            .unwrap()
            .len(),
        1
    );
    let hits = e
        .search_inner("UNIQUEINV87231", &s, "Both", "All files", "", "")
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].page, 1);
    reply.join().unwrap();
}
#[test]
fn lexical_results_arrive_before_a_slow_embedding_request_finishes() {
    let (_h, root, mut s, e, rx) = fixture();
    add(
        &e,
        &s,
        &root.path().join("report.txt"),
        "report description",
    );
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    s.remote_url = format!("http://{}", server.server_addr());
    let started = Arc::new(AtomicBool::new(false));
    let marker = started.clone();
    let reply = std::thread::spawn(move || {
        let request = server.recv().unwrap();
        marker.store(true, Ordering::Release);
        std::thread::sleep(Duration::from_millis(700));
        let _ = request.respond(tiny_http::Response::from_string(
            "{\"data\":[{\"index\":0,\"embedding\":[1,0]}]}",
        ));
    });
    let begin = Instant::now();
    e.search(
        12,
        "report".into(),
        s,
        "Both".into(),
        "All files".into(),
        String::new(),
        String::new(),
    );
    let mut preview = None;
    let mut final_hits = None;
    while begin.elapsed() < Duration::from_secs(5) {
        match rx.recv_timeout(Duration::from_secs(2)).unwrap() {
            Event::SearchPartial(12, hits, _) => {
                preview = Some(begin.elapsed());
                assert_eq!(hits.len(), 1);
            }
            Event::Search(12, result, _) => {
                final_hits = Some(result.unwrap());
                break;
            }
            _ => {}
        }
    }
    assert!(preview.unwrap() < Duration::from_millis(500));
    assert_eq!(final_hits.unwrap().len(), 1);
    assert!(started.load(Ordering::Acquire));
    reply.join().unwrap();
}
#[test]
fn saved_chunks_resume_and_changed_fingerprints_discard_them() {
    let (_h, root, s, e, _) = fixture();
    let c = e.db.connect(false).unwrap();
    Database::begin_session(&c).unwrap();
    let path = root.path().join("long.txt").to_string_lossy().to_string();
    let (token, offset) = Database::prepare_job(&c, &path, "version1").unwrap();
    assert_eq!(offset, 0);
    Database::stage(
        &c,
        &token,
        0,
        &[crate::content::Segment::text(
            "text",
            "first passage".into(),
            0,
            0,
        )],
        &[vec![1.0, 0.0]],
    )
    .unwrap();
    let reopened = e.db.connect(false).unwrap();
    let (same, next) = Database::prepare_job(&reopened, &path, "version1").unwrap();
    assert_eq!(same, token);
    assert_eq!(next, 1);
    Database::begin_session(&reopened).unwrap();
    assert_eq!(Database::session(&reopened).unwrap(), (0, 1));
    let (new, next) = Database::prepare_job(&reopened, &path, "version2").unwrap();
    assert_ne!(new, token);
    assert_eq!(next, 0);
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM native_staging WHERE token=?1",
            [token],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0
    );
    drop(s);
}
#[test]
fn secret_names_are_rejected_before_any_reader_and_in_all_source_modes() {
    let (_h, root, mut s, _e, _) = fixture();
    for mode in ["quick", "full"] {
        s.roots[0].mode = mode.into();
        for name in [
            "passwd.txt",
            "passwords.csv",
            "id_ed25519",
            "backup.p12",
            "private.keystore",
            "credentials.json",
            ".env",
        ] {
            assert!(!s.searched(&root.path().join(name)), "{mode}: {name}");
        }
    }
}
#[test]
fn metadata_filters_apply_to_lexical_content_without_a_model() {
    let (_h, root, mut s, e, _) = fixture();
    add(&e, &s, &root.path().join("report.txt"), "invoicecode foo");
    s.roots[0].mode = "quick".into();
    assert!(
        e.search_inner("invoicecode size:>1KB", &s, "Both", "All files", "", "")
            .unwrap()
            .is_empty()
    );
    s.roots[0].mode = "full".into();
    s.remote_url = "http://127.0.0.1:1".into();
    assert_eq!(
        e.search_inner("invoicecode size:>1KB", &s, "Both", "All files", "", "")
            .unwrap()
            .len(),
        1
    );
    assert!(
        e.search_inner("invoicecode size:>1MB", &s, "Both", "All files", "", "")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn background_prune_is_bounded_and_keeps_current_sources() {
    let (_home, root, s, e, _) = fixture();
    let inside = root.path().join("retained.txt");
    add(&e, &s, &inside, "retained content");
    let c = e.db.connect(false).unwrap();
    for i in 0..1100 {
        Database::touch_name(
            &c,
            Path::new(&format!(r"C:\removed\stale{i:04}.txt")),
            "file",
            1,
            1,
            "quick",
            r"C:\removed",
            1,
        )
        .unwrap();
    }
    drop(c);
    let mut steps = 0;
    while !e.db.prune_step(&s, "fixture-policy").unwrap() {
        steps += 1;
        assert!(steps < 100);
    }
    assert!(steps >= 2);
    assert_eq!(names(&e, &s, "retained").len(), 1);
    assert_eq!(e.db.stats().unwrap().files, 1);
}
#[test]
fn cancel_interrupts_a_waiting_embedding_connection() {
    let (_h, _r, mut s, e, _) = fixture();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    s.remote_url = format!("http://{}", server.server_addr());
    let flag = Arc::new(AtomicBool::new(false));
    let stop = flag.clone();
    let thread = std::thread::spawn(move || {
        let _request = server.recv().unwrap();
        stop.store(true, Ordering::Release);
        std::thread::sleep(Duration::from_millis(700));
    });
    let cancel = e.control.cancel.clone();
    let marker = flag.clone();
    let trigger = std::thread::spawn(move || {
        while !marker.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        cancel.store(true, Ordering::Release);
    });
    let record = crate::content::Segment::text("text", "hello".into(), 0, 0);
    let began = Instant::now();
    assert!(
        e.models
            .segments(&s, &[record], &|| e.control.file_checkpoint())
            .is_err()
    );
    assert!(began.elapsed() < Duration::from_millis(500));
    trigger.join().unwrap();
    thread.join().unwrap();
}
#[test]
#[ignore = "requires bundled PDFium; run explicitly in packaged codec checks"]
fn pdf_metadata_streaming_and_cancellation_preserve_readable_pages() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pdf-invalid-info.pdf");
    let mut pages = vec![];
    let result = crate::pdf::visit(&path, true, false, &|| Ok(()), &mut |page, text, image| {
        pages.push((page, text));
        assert!(image.unwrap().starts_with(&[0xff, 0xd8]));
        anyhow::bail!("fixture cancellation")
    });
    assert!(result.is_err());
    assert_eq!(pages.len(), 1);
    assert!(pages[0].1.contains("UNIQUE-7281"));
    let mut again = vec![];
    crate::pdf::visit(&path, false, false, &|| Ok(()), &mut |page, text, _| {
        again.push((page, text));
        Ok(())
    })
    .unwrap();
    assert_eq!(again.len(), 2);
    let mut resumed = vec![];
    crate::pdf::visit_from(&path, 2, false, false, &|| Ok(()), &mut |page, _, _| {
        resumed.push(page);
        Ok(())
    })
    .unwrap();
    assert_eq!(resumed, vec![2]);
}

#[test]
#[ignore = "requires explicitly configured EmbeddingGemma 2 reference backend"]
fn real_backend_matches_pinned_text_and_image_reference() {
    use base64::Engine as _;
    let home = std::env::var("VECTORDASH_REFERENCE_HOME").expect("supply a configured test home");
    let mut s = Settings::load(Path::new(&home)).unwrap();
    s.defer_search_model = false;
    s.pause_for_game = false;
    s.pause_for_fullscreen = false;
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/embedding-reference.json")).unwrap();
    let records: Vec<crate::content::Segment> = reference["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let mut segment = crate::content::Segment::text(
                r["kind"].as_str().unwrap(),
                r["text"].as_str().unwrap().into(),
                0,
                0,
            );
            segment.media = base64::engine::general_purpose::STANDARD
                .decode(r["media"].as_str().unwrap())
                .unwrap();
            segment
        })
        .collect();
    let models = Models::default();
    let batched = models.segments(&s, &records, &|| Ok(())).unwrap();
    fn cosine(a: &[f32], b: &[f32]) -> f64 {
        let dot: f64 = a.iter().zip(b).map(|(a, b)| *a as f64 * *b as f64).sum();
        dot / (a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>()
            * b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>())
        .sqrt()
    }
    for (i, record) in records.iter().enumerate() {
        let single = models
            .segments(&s, std::slice::from_ref(record), &|| Ok(()))
            .unwrap();
        let expected: Vec<f32> = reference["records"][i]["vector"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap() as f32)
            .collect();
        assert_eq!(batched[i].len(), 768);
        assert!(
            cosine(&batched[i], &expected) > 0.999,
            "reference mismatch for fixture {i}"
        );
        assert!(
            cosine(&batched[i], &single[0]) > 0.99999,
            "batch mapping mismatch for fixture {i}"
        );
    }
}

#[test]
fn selected_action_dispatch_does_not_wait_for_inflight_embedding() {
    let (_home, root, mut s, e, rx) = fixture();
    add(&e, &s, &root.path().join("launch-report.exe"), "report");
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    s.remote_url = format!("http://{}", server.server_addr());
    let (arrived, arrival) = std::sync::mpsc::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let http = std::thread::spawn(move || {
        let request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        arrived.send(()).unwrap();
        let _ = gate.recv_timeout(Duration::from_secs(5));
        let _ = request.respond(tiny_http::Response::from_string(
            "{\"data\":[{\"index\":0,\"embedding\":[1,0]}]}",
        ));
    });
    e.search(
        987,
        "report".into(),
        s,
        "Both".into(),
        "All files".into(),
        String::new(),
        String::new(),
    );
    arrival.recv_timeout(Duration::from_secs(5)).unwrap();
    let begin = Instant::now();
    e.models.cancel_query();
    let actual = std::env::var("VECTORDASH_TEST_OPEN_FIXTURE").ok();
    let timeout = if actual.is_some() {
        Duration::from_secs(3)
    } else {
        Duration::from_millis(500)
    };
    crate::engine::dispatch_action(e.tx.clone(), move || {
        if let Some(path) = actual {
            crate::platform::open(&path)
        } else {
            Ok(())
        }
    });
    let mut opened = false;
    while begin.elapsed() < timeout {
        if let Ok(Event::OpenDone(result)) = rx.recv_timeout(Duration::from_millis(50)) {
            result.unwrap();
            opened = true;
            break;
        }
    }
    release.send(()).unwrap();
    http.join().unwrap();
    assert!(
        opened,
        "selected action must run while the model response is still blocked"
    );
}

#[test]
fn burst_statistics_refreshes_are_coalesced_without_repeated_global_counts() {
    let (_home, _root, s, e, rx) = fixture();
    for _ in 0..50 {
        e.refresh_stats(&s);
    }
    let start = Instant::now();
    let mut counts = 0;
    while start.elapsed() < Duration::from_secs(2) {
        if let Ok(Event::Stats(_, _)) = rx.recv_timeout(Duration::from_millis(50)) {
            counts += 1;
        }
    }
    assert_eq!(
        counts, 1,
        "burst requests must not each run database-wide statistics queries"
    );
}

#[test]
fn visual_queries_do_not_promote_infographics_for_exact_ocr_words() {
    let (_home, root, mut settings, engine, _rx) = fixture();
    let c = engine.db.connect(false).unwrap();
    for (name, text, similarity) in [
        ("dog-photo.jpg", "", 0.85_f32),
        ("infographic.jpg", "dogs dog statistics", 0.75_f32),
    ] {
        let path = root.path().join(name);
        std::fs::write(&path, b"fixture").unwrap();
        Database::touch_name(&c, &path, "file", 1, 7, "full", &settings.roots[0].path, 1).unwrap();
        let mut segment = crate::content::Segment::text("image", text.into(), 0, 0);
        segment.media = vec![1];
        Database::save_segments(
            &c,
            &path.to_string_lossy(),
            &[segment],
            &[vec![similarity, (1.0 - similarity * similarity).sqrt()]],
            "fixture",
        )
        .unwrap();
    }
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    settings.embedding_server = "Remote".into();
    settings.remote_url = format!("http://{}/v1", server.server_addr());
    let reply = std::thread::spawn(move || {
        while let Some(request) = server.recv_timeout(Duration::from_secs(2)).unwrap() {
            request
                .respond(
                    tiny_http::Response::from_string(r#"{"data":[{"embedding":[1.0,0.0]}]}"#)
                        .with_header(
                            tiny_http::Header::from_bytes("Content-Type", "application/json")
                                .unwrap(),
                        ),
                )
                .unwrap();
        }
    });
    for calibrated in [false, true] {
        settings.rank_calibration = calibrated;
        for query in ["photos of dogs", "photo of dog"] {
            let hits = engine
                .search_inner(query, &settings, "Semantic", "All files", "", "")
                .unwrap();
            assert_eq!(hits.len(), 2);
            assert!(
                hits[0].path.ends_with("dog-photo.jpg"),
                "{query} calibration={calibrated}: {hits:?}"
            );
        }
    }
    reply.join().unwrap();
}

#[test]
fn explicit_path_search_bypasses_embeddings_in_every_mode() {
    let (_home, root, mut s, e, _) = fixture();
    let path = root.path().join("RHI").join("nvngx_dlssnr.dll");
    add(&e, &s, &path, "fixture contents");
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    s.remote_url = format!("http://{}", server.server_addr());
    let started = Instant::now();
    for query in [r"\RHI\nvngx_dlssnr.dll", "*nr*", r"*\*nr*"] {
        for mode in ["Names", "Both", "Semantic"] {
            let hits = e
                .search_inner(query, &s, mode, "All files", &s.roots[0].path, "")
                .unwrap();
            assert_eq!(hits.len(), 1, "{mode}");
            assert_eq!(hits[0].path, path.to_string_lossy());
        }
    }
    assert!(
        server.try_recv().unwrap().is_none(),
        "Path searches must not request embeddings"
    );
    println!(
        "Three search modes: {:.2} ms; zero embedding requests",
        started.elapsed().as_secs_f64() * 1000.0
    );
}

#[test]
fn changing_embedding_deployment_does_not_reembed_completed_files() {
    let (_home, root, mut s, e, _) = fixture();
    let path = root.path().join("deployment.txt");
    std::fs::write(&path, "already indexed deployment fixture").unwrap();
    s.pause_for_game = false;
    s.pause_for_fullscreen = false;
    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    s.embedding_server = "Remote".into();
    s.remote_url = format!("http://{}", server.server_addr());
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let stopped = Arc::new(AtomicBool::new(false));
    let reply = server.clone();
    let requests = count.clone();
    let stop = stopped.clone();
    let worker = std::thread::spawn(move || {
        while !stop.load(Ordering::Acquire) {
            if let Ok(Some(request)) = reply.recv_timeout(Duration::from_millis(20)) {
                requests.fetch_add(1, Ordering::SeqCst);
                let _ = request.respond(tiny_http::Response::from_string(
                    "{\"data\":[{\"index\":0,\"embedding\":[1,0]}]}",
                ));
            }
        }
    });
    *e.control.schedule.lock().unwrap() = s.clone();
    e.index_inner(&s, false).unwrap();
    assert!(count.load(Ordering::SeqCst) > 0);
    let before = count.load(Ordering::SeqCst);
    let c = e.db.connect(true).unwrap();
    let blob: Vec<u8> = c
        .query_row("SELECT vector FROM segment_vectors LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    s.embedding_server = "Local".into();
    s.model = "renamed-local-model.gguf".into();
    s.projector = "renamed-encoder.gguf".into();
    *e.control.schedule.lock().unwrap() = s.clone();
    e.index_inner(&s, false).unwrap();
    s.embedding_server = "Remote".into();
    s.remote_model = "server-model-alias".into();
    *e.control.schedule.lock().unwrap() = s.clone();
    e.index_inner(&s, false).unwrap();
    assert_eq!(
        count.load(Ordering::SeqCst),
        before,
        "Provider/model aliases must not trigger inference"
    );
    assert_eq!(
        blob,
        c.query_row("SELECT vector FROM segment_vectors LIMIT 1", [], |r| r
            .get::<_, Vec<u8>>(0))
            .unwrap()
    );
    // Reproduce an old-version job left behind for already-completed content.
    let writer = e.db.connect(false).unwrap();
    let stamp: String = writer
        .query_row(
            "SELECT fingerprint FROM native_content WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |r| r.get(0),
        )
        .unwrap();
    let (token, _) = Database::prepare_job(&writer, &path.to_string_lossy(), &stamp).unwrap();
    Database::stage(
        &writer,
        &token,
        0,
        &[crate::content::Segment::text(
            "text",
            "redundant".into(),
            1,
            0,
        )],
        &[vec![1.0, 0.0]],
    )
    .unwrap();
    e.index_inner(&s, false).unwrap();
    assert_eq!(
        count.load(Ordering::SeqCst),
        before,
        "A redundant resume job must not override completed content"
    );
    assert_eq!(
        writer
            .query_row("SELECT count(*) FROM native_jobs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        writer
            .query_row("SELECT count(*) FROM native_staging", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        blob,
        c.query_row("SELECT vector FROM segment_vectors LIMIT 1", [], |r| r
            .get::<_, Vec<u8>>(0))
            .unwrap()
    );
    // Explicit queued rebuilds must survive the same completion check.
    Database::prepare_job(&writer, &path.to_string_lossy(), &stamp).unwrap();
    writer
        .execute(
            "INSERT INTO native_rebuild_requests VALUES(?1)",
            [path.to_string_lossy().as_ref()],
        )
        .unwrap();
    e.index_inner(&s, false).unwrap();
    let rebuilt = count.load(Ordering::SeqCst);
    assert!(rebuilt > before);
    assert_eq!(
        writer
            .query_row("SELECT count(*) FROM native_rebuild_requests", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    // A file edit still updates its vectors, and an explicit rebuild still runs.
    std::fs::write(&path, "file contents changed, requiring another embedding").unwrap();
    e.index_inner(&s, false).unwrap();
    let edited = count.load(Ordering::SeqCst);
    assert!(edited > rebuilt);
    let quick = root.path().join("quick-only");
    std::fs::create_dir(&quick).unwrap();
    s.roots.push(Source {
        path: quick.to_string_lossy().into(),
        mode: "quick".into(),
    });
    s.index_order = "Quick first".into();
    *e.control.schedule.lock().unwrap() = s.clone();
    e.index_inner(&s, true).unwrap();
    assert!(count.load(Ordering::SeqCst) > edited);
    stopped.store(true, Ordering::Release);
    worker.join().unwrap();
}

#[test]
fn quick_sources_find_hidden_appdata_names_with_cache_exclusion_enabled() {
    let (_h, root, mut s, e, _) = fixture();
    let appdata = root.path().join("AppData");
    let rhi = appdata.join("Local").join("RHI");
    std::fs::create_dir_all(rhi.join("DLSS-NR")).unwrap();
    let dll = rhi.join("DLSS-NR").join("nvngx_dlssnr.dll");
    std::fs::write(&dll, "fixture").unwrap();
    #[cfg(windows)]
    unsafe {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = appdata.as_os_str().encode_wide().chain(Some(0)).collect();
        windows::Win32::Storage::FileSystem::SetFileAttributesW(
            windows::core::PCWSTR(wide.as_ptr()),
            windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_HIDDEN,
        )
        .unwrap();
    }
    s.roots[0].mode = "quick".into();
    s.skip_windows_churn = true;
    s.pause_for_game = false;
    s.pause_for_fullscreen = false;
    *e.control.schedule.lock().unwrap() = s.clone();
    e.index_inner(&s, false).unwrap();
    for query in [r"\Local\RHI", r"\RHI\nvngx_dlssnr.dll"] {
        let hits = names(&e, &s, query);
        assert_eq!(hits.len(), 1, "{query}: {hits:?}");
    }
    assert_eq!(e.db.stats().unwrap().vectors, 0);
    s.roots[0].mode = "full".into();
    assert!(!s.searched(&dll));
    assert!(
        !crate::config::SearchPolicy::new(&s, "All files", "", "")
            .allowed(&dll.to_string_lossy(), "file")
    );
    s.roots[0].mode = "quick".into();
    s.exclusions.push(rhi.to_string_lossy().into());
    assert!(names(&e, &s, r"\RHI\nvngx_dlssnr.dll").is_empty());
}

#[test]
fn newer_filename_results_arrive_while_old_embedding_response_is_delayed() {
    let (_h, root, mut s, e, rx) = fixture();
    add(&e, &s, &root.path().join("first.txt"), "first passage");
    add(&e, &s, &root.path().join("second.txt"), "second passage");
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    s.remote_url = format!("http://{}", server.server_addr());
    let started = Arc::new(AtomicBool::new(false));
    let marker = started.clone();
    let reply = std::thread::spawn(move || {
        for i in 0..2 {
            let request = server
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            if i == 0 {
                marker.store(true, Ordering::Release);
                std::thread::sleep(Duration::from_millis(700));
            }
            let _ = request.respond(tiny_http::Response::from_string(
                "{\"data\":[{\"index\":0,\"embedding\":[1,0]}]}",
            ));
        }
    });
    e.search(
        1,
        "first".into(),
        s.clone(),
        "Both".into(),
        "All files".into(),
        String::new(),
        String::new(),
    );
    let wait = Instant::now();
    while !started.load(Ordering::Acquire) {
        assert!(wait.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(5));
    }
    e.models.cancel_query();
    let began = Instant::now();
    e.search(
        2,
        "second".into(),
        s,
        "Both".into(),
        "All files".into(),
        String::new(),
        String::new(),
    );
    loop {
        if let Event::SearchPartial(2, hits, _) = rx.recv_timeout(Duration::from_secs(2)).unwrap() {
            assert!(began.elapsed() < Duration::from_millis(500));
            assert!(hits.iter().any(|h| h.path.ends_with("second.txt")));
            break;
        }
    }
    reply.join().unwrap();
}
