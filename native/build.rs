fn main() {
    println!("cargo:rerun-if-changed=assets/vectordash.ico");
    println!("cargo:rerun-if-changed=assets/app.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            // Media Foundation is optional on Windows N/Server. Check availability
            // before calling it; missing media features must not prevent app startup.
            println!("cargo:rustc-link-lib=delayimp");
            println!("cargo:rustc-link-arg=/DELAYLOAD:mfplat.dll");
            println!("cargo:rustc-link-arg=/DELAYLOAD:mfreadwrite.dll");
        }
        let version = std::env::var("CARGO_PKG_VERSION").unwrap();
        let commas = format!("{},0", version.split('-').next().unwrap().replace('.', ","));
        let icon = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("assets/vectordash.ico")
            .to_string_lossy()
            .replace('\\', "/");
        let source = std::fs::read_to_string("assets/app.rc")
            .unwrap()
            .replace("@ICON@", &icon)
            .replace("@VERSION_COMMAS@", &commas)
            .replace("@VERSION@", &version);
        let rc = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("vectordash.rc");
        std::fs::write(&rc, source).unwrap();
        embed_resource::compile(rc, embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
