#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod ai;
mod app;
mod appearance;
mod capture;
mod changes;
mod cli;
mod color_emoji;
mod config;
mod content;
mod db;
mod dictation;
mod engine;
mod icons;
mod journal;
mod launcher;
mod mcp;
mod models;
mod pdf;
mod platform;
mod previews;
mod resources;
mod scheduling;
mod search_filters;
mod setup;
mod speech_ffi;
mod updates;
mod vector_cache;
mod worker;
fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
fn run() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|x| x == "--speech-worker") {
        return speech_ffi::worker();
    }
    if args.iter().any(|x| x == "--extract-worker") {
        return worker::worker_main();
    }
    let home = config::home(args.iter().any(|x| x == "--portable"))?;
    unsafe {
        std::env::set_var("VECTORDASH_MODEL_LOG", home.join("model.log"));
    }
    if let Some(i) = args.iter().position(|a| a == "--journal-probe") {
        let result=journal::probe(args.get(i+1).ok_or_else(||anyhow::anyhow!("Missing volume"))?).unwrap_or_else(|e|serde_json::json!({"accessible":false,"fallback":"folder notifications","error":e.to_string()}));
        if let Some(i) = args.iter().position(|a| a == "--report") {
            std::fs::write(&args[i + 1], serde_json::to_vec(&result)?)?;
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
            .with_visible(!hidden),
        ..Default::default()
    };
    eframe::run_native(
        "VectorDash",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, home, smoke)?))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
