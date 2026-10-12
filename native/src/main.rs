#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod cli;
mod config;
mod content;
mod db;
mod engine;
mod mcp;
mod models;
mod pdf;
mod platform;
mod previews;
mod setup;
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
    if args.iter().any(|x| x == "--extract-worker") {
        return worker::worker_main();
    }
    let home = config::home(args.iter().any(|x| x == "--portable"))?;
    unsafe {
        std::env::set_var("VECTORDASH_MODEL_LOG", home.join("model.log"));
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
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([780.0, 540.0])
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
