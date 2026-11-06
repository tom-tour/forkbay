use std::fmt::Write;

fn main() {
    for f in ["assets/app.rc", "assets/icon.ico", "assets/app.manifest", "assets/THIRD-PARTY.txt", "ui/index.html", "emulators.json", "locales"] {
        println!("cargo:rerun-if-changed={f}");
    }
    // Every file in locales/ is built in: adding a language needs no code change.
    let mut list = String::from("pub const EMBEDDED: &[(&str, &str)] = &[\n");
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut files: Vec<_> = std::fs::read_dir(format!("{root}/locales")).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    for p in files {
        let code = p.file_stem().unwrap().to_string_lossy().into_owned();
        writeln!(list, "    ({code:?}, include_str!({:?})),", p.to_string_lossy()).unwrap();
    }
    list.push_str("];\n");
    std::fs::write(format!("{}/locales.rs", std::env::var("OUT_DIR").unwrap()), list).unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/app.rc", embed_resource::NONE).manifest_required().unwrap();
    }
}
