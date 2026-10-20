#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod admin;
mod ai;
mod app;
mod appearance;
mod backdrop;
mod capture;
mod changes;
mod cli;
mod color_emoji;
mod computer;
mod config;
mod content;
mod db;
mod dictation;
mod engine;
#[cfg(test)]
mod feature_tests;
mod i18n;
mod icons;
mod image_search;
mod index_costs;
mod journal;
mod launcher;
mod mcp;
mod models;
mod native_video;
mod pdf;
mod platform;
mod previews;
mod raw;
mod remote;
mod resources;
mod scheduling;
mod search_filters;
mod setup;
mod shortcuts;
mod speech_ffi;
#[cfg(test)]
mod stability_tests;
mod tray;
mod tts;
mod updates;
mod vector_cache;
mod web_search;
mod worker;
fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
fn run() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|x| x == "--control-worker") {
        return computer::worker_main();
    }
    if args.iter().any(|x| x == "--speech-worker") {
        return speech_ffi::worker();
    }
    if args.iter().any(|x| x == "--extract-worker") {
        return worker::worker_main();
    }
    admin::prepare_handoff(&args)?;
    let home = config::home(args.iter().any(|x| x == "--portable"))?;
    unsafe {
        std::env::set_var("VECTORDASH_MODEL_LOG", home.join("model.log"));
    }
    if let Some(i) = args.iter().position(|a| a == "--preview-probe") {
        let original = args
            .get(i + 1)
            .ok_or_else(|| anyhow::anyhow!("Missing PDF path"))?;
        let resolved = platform::preview_path(original);
        let result = pdf::preview(&resolved, 1, "");
        let report = serde_json::json!({"original":original,"resolved":resolved,"bytes":result.as_ref().map(|v|v.len()).unwrap_or(0),"ok":result.is_ok(),"error":result.as_ref().err().map(|e|format!("{e:#}"))});
        if let Some(i) = args.iter().position(|a| a == "--report") {
            std::fs::write(&args[i + 1], serde_json::to_vec_pretty(&report)?)?;
        }
        return result.map(|_| ());
    }
    if let Some(i) = args.iter().position(|a| a == "--journal-probe") {
        let result=journal::probe(args.get(i+1).ok_or_else(||anyhow::anyhow!("Missing volume"))?).unwrap_or_else(|e|serde_json::json!({"accessible":false,"fallback":"folder notifications","error":e.to_string()}));
        if let Some(i) = args.iter().position(|a| a == "--report") {
            std::fs::write(&args[i + 1], serde_json::to_vec(&result)?)?;
        }
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--test-tts") {
        let mut settings = config::Settings::default();
        settings.tts_url = args
            .get(i + 1)
            .ok_or_else(|| anyhow::anyhow!("Missing TTS URL"))?
            .clone();
        settings.tts_model = args
            .get(i + 2)
            .ok_or_else(|| anyhow::anyhow!("Missing TTS model"))?
            .clone();
        settings.tts_voice = args
            .get(i + 3)
            .ok_or_else(|| anyhow::anyhow!("Missing TTS voice"))?
            .clone();
        let mut reports = vec![];
        let result = tts::speak_report(
            &settings,
            "VectorDash connection test.",
            &std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            &std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            |report| reports.push(report),
        );
        platform::stop_playback();
        if let Some(i) = args.iter().position(|a| a == "--report") {
            std::fs::write(
                &args[i + 1],
                serde_json::to_vec(
                    &serde_json::json!({"ok":result.is_ok(),"error":result.as_ref().err().map(|e|e.to_string()),"requests":reports}),
                )?,
            )?;
        }
        return result;
    }
    if let Some(i) = args.iter().position(|a| a == "--test-playback-pause") {
        let path = args
            .get(i + 1)
            .ok_or_else(|| anyhow::anyhow!("Missing audio path"))?;
        platform::play_at(path, 0.0)?;
        let start = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(500));
        platform::pause_playback(true)?;
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let active = platform::playback_active();
        platform::pause_playback(false)?;
        let mut errors = vec![];
        while platform::playback_active() && start.elapsed() < std::time::Duration::from_secs(12) {
            errors.extend(platform::playback_errors());
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let elapsed = start.elapsed().as_secs_f64();
        platform::stop_playback();
        if let Some(i) = args.iter().position(|a| a == "--report") {
            std::fs::write(
                &args[i + 1],
                serde_json::json!({"active_while_paused":active,"elapsed":elapsed,"errors":errors})
                    .to_string(),
            )?;
        }
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--test-audio-playback") {
        let path = args
            .get(i + 1)
            .ok_or_else(|| anyhow::anyhow!("Missing test audio path"))?;
        platform::play_at(path, 1.0)?;
        let start = std::time::Instant::now();
        let mut errors = vec![];
        while start.elapsed() < std::time::Duration::from_secs(4) {
            std::thread::sleep(std::time::Duration::from_millis(100));
            errors.extend(platform::playback_errors());
        }
        if platform::playback_active() {
            errors.push("Audio did not finish within the test deadline".into());
        }
        platform::stop_playback();
        if let Some(i) = args.iter().position(|a| a == "--report") {
            std::fs::write(
                &args[i + 1],
                serde_json::to_vec(&serde_json::json!({"ok":errors.is_empty(),"errors":errors}))?,
            )?;
        }
        anyhow::ensure!(errors.is_empty(), "Audio playback failed: {:?}", errors);
        return Ok(());
    }
    if cli::run(&home, &args)? {
        return Ok(());
    }
    if admin::should_elevate(
        config::Settings::load(&home)?.always_admin,
        admin::elevated(),
        &args,
    ) {
        match admin::relaunch(&home, &args) {
            Ok(()) => return Ok(()),
            Err(e) => {
                use std::io::Write;
                if let Ok(mut log) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(home.join("vectordash.log"))
                {
                    let _ = writeln!(
                        log,
                        "Warning {} {e}; continuing without administrator access",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                    );
                }
            }
        }
    }
    let smoke = args
        .iter()
        .position(|x| x == "--smoke-report")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from);
    let hidden =
        args.iter().any(|x| x == "--startup") && config::Settings::load(&home)?.start_hidden;
    let window_state = config::WindowState::load(&home);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_position(window_state.main.unwrap_or([80.0, 80.0]))
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([780.0, 540.0])
            .with_icon({
                let image =
                    image::load_from_memory(include_bytes!("../assets/vectordash.png"))?.to_rgba8();
                eframe::egui::IconData {
                    width: image.width(),
                    height: image.height(),
                    rgba: image.into_raw(),
                }
            })
            .with_decorations(false)
            .with_transparent(true)
            .with_has_shadow(false)
            .with_visible(!hidden),
        ..Default::default()
    };
    let _installation_guard = platform::installation_guard();
    eframe::run_native(
        "VectorDash",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, home, smoke)?))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
