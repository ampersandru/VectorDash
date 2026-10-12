use crate::{
    config::Settings,
    db::Database,
    engine::{Control, Engine, Event},
    models::Models,
};
use anyhow::Result;
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};
pub fn run(home: &Path, args: &[String]) -> Result<bool> {
    let get = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if !args
        .iter()
        .any(|a| ["--index", "--query", "--import", "--inspect-image"].contains(&a.as_str()))
    {
        return Ok(false);
    }
    let mut s = Settings::load(home)?;
    if let Some(url) = get("--server-url") {
        s.remote_url = url;
    }
    if let Some(profile) = get("--profile") {
        s.performance = profile;
    }
    if args.iter().any(|a| a == "--unencrypted") {
        s.encrypt_index = false;
    }
    s.include_programs = false;
    s.count_files = args.iter().any(|a| a == "--count");
    if args.iter().any(|a| a == "--include-cache-folders") {
        s.skip_windows_churn = false;
    }
    let db = Database::new(home, &s)?;
    let (tx, rx) = crossbeam_channel::bounded(4096);
    let engine = Engine {
        db: db.clone(),
        models: Models::default(),
        control: Control::default(),
        tx: tx.into(),
        searching: Arc::new(AtomicBool::new(false)),
        vectors: crate::vector_cache::Cache::default(),
    };
    let start = Instant::now();
    let mut report = serde_json::json!({"native_rust":true});
    if let Some(image) = get("--inspect-image") {
        let image = crate::content::decode(Path::new(&image), &s)?;
        report["image"] = serde_json::json!({"width":image.width(),"height":image.height()});
    }
    if let Some(source) = get("--import") {
        let old = Settings::legacy(Path::new(&source))?;
        db.import(Path::new(&source), old.encrypt_index, |_, _| {})?;
        s = old;
        s.encrypt_index = db.key.is_some();
        s.save(home)?;
        Database::new(home, &s)?;
        report["import_seconds"] = serde_json::json!(start.elapsed().as_secs_f64());
    }
    if let Some(root) = get("--index") {
        let mode = if args.iter().any(|a| a == "--quick") {
            "quick"
        } else {
            "full"
        };
        s.roots = vec![crate::config::Source {
            path: root,
            mode: mode.into(),
        }];
        s.save(home)?;
        engine.index(s.clone(), args.iter().any(|a| a == "--force"));
        let mut first = None;
        let mut progress = serde_json::Value::Null;
        let mut logs = vec![];
        loop {
            match rx.recv_timeout(Duration::from_secs(120)) {
                Ok(Event::IndexDone(result)) => {
                    report["index_result"] =
                        serde_json::json!(result.map_err(|e| anyhow::anyhow!(e))?);
                    break;
                }
                Ok(Event::Progress(p)) => {
                    if p.completed > 0 && first.is_none() {
                        first = Some(start.elapsed().as_secs_f64());
                    }
                    progress = serde_json::to_value(p)?;
                }
                Ok(Event::Log(level, text)) => {
                    logs.push(serde_json::json!({"level":level,"text":text}));
                }
                Err(_) => anyhow::bail!("Index produced no events for 120 seconds"),
                _ => {}
            }
        }
        report["index_seconds"] = serde_json::json!(start.elapsed().as_secs_f64());
        report["first_completed_seconds"] = serde_json::json!(first);
        report["progress"] = progress;
        report["logs"] = serde_json::json!(logs);
    }
    if let Some(q) = get("--query") {
        let mut timings = vec![];
        let mut hits = vec![];
        for _ in 0..get("--repeat")
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(1)
            .clamp(1, 20)
        {
            let start = Instant::now();
            hits = engine.search_inner(
                &q,
                &s,
                get("--mode").as_deref().unwrap_or("Both"),
                get("--category").as_deref().unwrap_or("All files"),
                "",
                "",
            )?;
            timings.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        report["search_ms"] = serde_json::json!(timings[0]);
        report["search_times_ms"] = serde_json::json!(timings);
        report["results"] = serde_json::to_value(hits)?;
    }
    report["stats"] = serde_json::to_value(db.stats()?)?;
    if let Some(path) = get("--report") {
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    println!("{report}");
    engine.models.stop()?;
    Ok(true)
}
