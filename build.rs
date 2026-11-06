fn main() {
    for f in ["assets/app.rc", "assets/icon.ico", "assets/app.manifest", "ui/index.html", "emulators.json"] {
        println!("cargo:rerun-if-changed={f}");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/app.rc", embed_resource::NONE).manifest_required().unwrap();
    }
}
