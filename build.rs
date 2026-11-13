use std::fmt::Write;

fn main() {
    for f in ["assets/app.rc", "assets/icon.ico", "assets/app.manifest", "assets/THIRD-PARTY.txt", "ui", "emulators.json", "locales"] {
        println!("cargo:rerun-if-changed={f}");
    }
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = std::env::var("OUT_DIR").unwrap();
    std::fs::write(format!("{out}/page.html"), page(&format!("{root}/ui"))).unwrap();

    // Every file in locales/ is built in: adding a language needs no code change.
    let mut list = String::from("pub const EMBEDDED: &[(&str, &str)] = &[\n");
    let mut files: Vec<_> = std::fs::read_dir(format!("{root}/locales")).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    for p in files {
        let code = p.file_stem().unwrap().to_string_lossy().into_owned();
        writeln!(list, "    ({code:?}, include_str!({:?})),", p.to_string_lossy()).unwrap();
    }
    list.push_str("];\n");
    std::fs::write(format!("{out}/locales.rs"), list).unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/app.rc", embed_resource::NONE).manifest_required().unwrap();
        quiet_dll(&root, &out);
    }
}

/// tools/nx2-quiet, built in: NX2 Launcher writes it next to its settings when it first applies a setting
/// during a game. Without the MSVC compiler the file is empty, and settings wait for the emulator to close.
fn quiet_dll(root: &str, out: &str) {
    println!("cargo:rerun-if-changed=tools/nx2-quiet/quiet.c");
    let dll = format!("{out}/nx2-quiet.dll");
    let target = std::env::var("TARGET").unwrap();
    let built = target.ends_with("-msvc")
        && cc::windows_registry::find(&target, "cl.exe").is_some_and(|mut cl| {
            cl.current_dir(out)
                .args(["/nologo", "/O2", "/MT", "/LD", "/W3"])
                .arg(format!("{root}/tools/nx2-quiet/quiet.c"))
                .arg(format!("/Fe{dll}"))
                .args(["/link", "user32.lib", "psapi.lib", "kernel32.lib", "dwmapi.lib"])
                .status()
                .is_ok_and(|s| s.success())
        });
    if !built {
        println!("cargo:warning=nx2-quiet.dll not built (needs the MSVC compiler): settings changed during a game wait for the emulator to close");
        std::fs::write(&dll, b"").unwrap();
    }
}

/// The page in one piece. Its security policy only allows code written inside it, so the style
/// sheet and the scripts of ui/ are put back in, the scripts in their order and in one block.
fn page(ui: &str) -> String {
    let read = |f: &str| std::fs::read_to_string(format!("{ui}/{f}")).unwrap_or_else(|e| panic!("ui/{f}: {e}"));
    let mut page = String::new();
    let mut scripts: Option<String> = None;
    for line in read("index.html").lines() {
        if let Some(src) = line.strip_prefix("<script src=\"").and_then(|l| l.strip_suffix("\"></script>")) {
            scripts.get_or_insert_with(String::new).push_str(&read(src));
            continue;
        }
        if let Some(s) = scripts.take() {
            write!(page, "<script>\n{s}</script>").unwrap();
        }
        if line == "<link rel=\"stylesheet\" href=\"style.css\">" {
            write!(page, "<style>\n{}</style>", read("style.css")).unwrap();
        } else {
            page.push_str(line);
            page.push('\n');
        }
    }
    page
}
