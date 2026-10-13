fn main() {
    println!("cargo:rerun-if-changed=assets/vectordash.ico");
    println!("cargo:rerun-if-changed=assets/app.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let version = std::env::var("CARGO_PKG_VERSION").unwrap();
        let commas = format!("{},0", version.replace('.', ","));
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
