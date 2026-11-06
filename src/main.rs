// Switch 2 Manager: one window for Switch 2 emulators, on Windows and Linux.
#![cfg_attr(not(test), windows_subsystem = "windows")]
mod art;
mod cloud;
mod compat;
mod core;
mod emu;
mod i18n;
mod import;
mod report;
mod steam;
mod update;

use crate::core::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::{Icon, WindowBuilder};
use wry::{WebContext, WebViewBuilder};

const HTML: &str = include_str!("../ui/index.html");

static PROXY: OnceLock<EventLoopProxy<String>> = OnceLock::new();

fn to_js(script: String) {
    if let Some(p) = PROXY.get() {
        let _ = p.send_event(script);
    }
}

pub fn emit_status(text: &str) {
    to_js(format!("window.onStatus({})", json!(i18n::t(text))));
}

fn s_arg(a: &Value, k: &str) -> String {
    a[k].as_str().unwrap_or("").to_string()
}

fn state() -> Result<Value> {
    let s = load_settings();
    let data = data_path();
    let games: Vec<Value> = rom_files().iter().map(|f| json!({ "file": f, "name": display_name(f), "emu": game_emulator(&s, f), "cover": art::cover_uri(f) })).collect();
    let emus: Vec<Value> = emu::list().iter().map(|e| json!({ "name": e.name, "present": emu::find_exe(&root().join(&e.name)).is_some() })).collect();
    Ok(json!({
        "setupDone": s.setup_done, "theme": s.theme, "language": i18n::current(), "languageSetting": s.language, "languages": i18n::languages(), "termsAccepted": s.terms_accepted >= 1, "autoUpdateCheck": s.auto_update_check, "version": update::version(),
        "dataLocation": s.data_location, "customDataPath": s.custom_data_path,
        "defaultEmulator": s.default_emulator, "dataPath": data.to_string_lossy(), "games": games, "emulators": emus,
        "keys": has_keys(&data), "firmware": firmware_count(&data),
        "keysFound": find_local_keys().map(|p| p.to_string_lossy().into_owned()), "firmwareFound": find_local_firmware().map(|p| p.to_string_lossy().into_owned()),
        "romsPath": root().join("Roms").to_string_lossy(), "hasGridKey": !unprotect(&s.steam_grid_key).is_empty(),
        "platform": if cfg!(windows) { "windows" } else { "linux" },
        "sync": { "enabled": s.sync.enabled, "provider": s.sync.provider, "folderPath": s.sync.folder_path, "googleClientId": s.sync.google_client_id,
                  "hasSecret": !unprotect(&s.sync.google_client_secret).is_empty(), "connected": !unprotect(&s.sync.google_refresh_token).is_empty(), "keepBackups": s.sync.keep_backups },
    }))
}

fn pick(a: &Value) -> Option<PathBuf> {
    let title = s_arg(a, "title");
    if s_arg(a, "kind") == "folder" {
        return rfd::FileDialog::new().set_title(&title).pick_folder();
    }
    let mut d = rfd::FileDialog::new().set_title(&title);
    if let Some(ext) = a["ext"].as_array() {
        let list: Vec<&str> = ext.iter().filter_map(|x| x.as_str()).collect();
        if !list.is_empty() { d = d.add_filter(&s_arg(a, "label"), &list); }
    }
    d.pick_file()
}

fn dispatch(cmd: &str, a: &Value) -> Result<Value> {
    let msg = |m: String| json!({ "message": i18n::t(&m) });
    Ok(match cmd {
        "set_language" => { let mut s = load_settings(); s.language = s_arg(a, "language"); save_settings(&s)?; i18n::page_locale() }
        "locale" => i18n::page_locale(),
        "accept_terms" => { let mut s = load_settings(); s.terms_accepted = 1; save_settings(&s)?; json!({}) }
        "compat" => compat::recommendations()?,
        "notices" => json!({ "text": include_str!("../assets/THIRD-PARTY.txt") }),
        "set_auto_update" => { let mut s = load_settings(); s.auto_update_check = a["enabled"].as_bool().unwrap_or(true); save_settings(&s)?; json!({}) }
        "about" => json!({ "version": update::version(), "license": include_str!("../LICENSE"), "repo": update::REPO_URL, "kofi": update::KOFI_URL }),
        "report_collect" => report::collect(&s_arg(a, "game"), a["log"].as_bool().unwrap_or(true))?,
        "report_save" => { report::save(&s_arg(a, "text"))?; json!({}) }
        "report_open_file" => { open_external(&manager_dir().join("bug-report.txt").to_string_lossy()); json!({}) }
        "update_check" => update::check()?,
        "update_apply" => {
            let message = update::apply()?;
            if message.contains("redémarrage") && std::env::var("S2M_SELFTEST").is_err() {
                std::process::Command::new(std::env::current_exe()?).spawn()?;
                std::thread::spawn(|| { std::thread::sleep(std::time::Duration::from_millis(600)); std::process::exit(0); });
            }
            msg(message)
        }
        "state" => {
            // Automated tests: proves that the page ran its script and reached the program.
            if let Ok(f) = std::env::var("S2M_SELFTEST") { let _ = std::fs::write(f, "page-loaded"); }
            repair_setup()?;
            state()?
        }
        "emu_status" => emu::status(),
        "emu_install" => msg(emu::install(&s_arg(a, "name"))?),
        "emu_add_github" => msg(emu::add_github(s_arg(a, "name").trim(), &s_arg(a, "repo"), s_arg(a, "filter").trim())?),
        "emu_add_local" => msg(emu::add_local(s_arg(a, "name").trim(), Path::new(&s_arg(a, "exe")))?),
        "emu_remove" => msg(emu::remove(&s_arg(a, "name"))?),
        "set_game_emu" => { let mut s = load_settings(); s.games.insert(s_arg(a, "file"), s_arg(a, "emu")); save_settings(&s)?; json!({}) }
        "set_default_emu" => { let mut s = load_settings(); s.default_emulator = s_arg(a, "emu"); save_settings(&s)?; json!({}) }
        "set_theme" => { let mut s = load_settings(); s.theme = s_arg(a, "theme"); save_settings(&s)?; json!({}) }
        "finish_setup" => { let mut s = load_settings(); s.setup_done = true; save_settings(&s)?; json!({}) }
        "reopen_setup" => { let mut s = load_settings(); s.setup_done = false; save_settings(&s)?; json!({}) }
        "play" => { std::process::Command::new(std::env::current_exe()?).arg("--launch").arg(format!("Roms/{}", s_arg(a, "file"))).spawn()?; json!({}) }
        "open_emulator" => { std::process::Command::new(std::env::current_exe()?).arg("--open").arg(s_arg(a, "name")).spawn()?; json!({}) }
        "sources" => import::sources(),
        "inspect" => import::inspect(Path::new(&s_arg(a, "path")))?,
        "import" => {
            let cats: Vec<String> = a["cats"].as_array().map(|v| v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            msg(import::import(Path::new(&s_arg(a, "path")), &cats)?)
        }
        "install_keys" => msg(install_keys(Path::new(&s_arg(a, "path")))?),
        "install_firmware" => msg(install_firmware(Path::new(&s_arg(a, "path")))?),
        "pick" => json!({ "path": pick(a).map(|p| p.to_string_lossy().into_owned()) }),
        "move_data" => msg(move_data(&s_arg(a, "location"), &s_arg(a, "custom"))?),
        "steam_accounts" => json!({ "found": steam::steam_dir().is_some(), "accounts": steam::steam_dir().map(|d| steam::accounts(&d)).unwrap_or_default() }),
        "steam_load" => steam::load(&s_arg(a, "account"))?,
        "steam_save" => {
            let games: Vec<String> = a["games"].as_array().map(|v| v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            msg(steam::save(&s_arg(a, "account"), &games)?)
        }
        "fetch_art" => msg(art::fetch_all()?),
        "set_grid_key" => { let mut s = load_settings(); s.steam_grid_key = protect(s_arg(a, "key").trim()); save_settings(&s)?; json!({}) }
        "cloud_save" => {
            let mut s = load_settings();
            s.sync.enabled = a["enabled"].as_bool().unwrap_or(false);
            s.sync.provider = s_arg(a, "provider");
            s.sync.folder_path = s_arg(a, "folderPath").trim().to_string();
            s.sync.google_client_id = s_arg(a, "googleClientId").trim().to_string();
            s.sync.keep_backups = a["keepBackups"].as_u64().unwrap_or(10).max(1) as u32;
            let secret = s_arg(a, "googleSecret");
            if !secret.is_empty() { s.sync.google_client_secret = protect(secret.trim()); }
            if s.sync.enabled && s.sync.provider == "folder" && !Path::new(&s.sync.folder_path).is_dir() { bail!("Choisis un dossier de synchronisation existant."); }
            save_settings(&s)?;
            json!({})
        }
        "cloud_sync" => {
            let s = load_settings();
            let r = cloud::sync(&s, &|l, d, m| cloud::ask_conflict(l, d, m, true))?;
            msg(match r {
                "none" => "Déjà synchronisé : les deux versions sont identiques.",
                "uploaded" => "Version locale envoyée dans le cloud.",
                "downloaded" => "Version distante installée. Une copie de la version locale est dans Save Backups.",
                _ => "Synchronisation annulée : rien n'a changé.",
            }.to_string())
        }
        "google_connect" => msg(cloud::google_connect()?),
        "open_path" => {
            let p = match s_arg(a, "which").as_str() { "roms" => root().join("Roms"), "covers" => art::cover_dir(), "data" => data_path(), _ => root() };
            std::fs::create_dir_all(&p)?;
            open_external(&p.to_string_lossy());
            json!({})
        }
        "open_url" => { let u = s_arg(a, "url"); if u.starts_with("https://") { open_external(&u); } json!({}) }
        _ => bail!("Commande inconnue : {cmd}"),
    })
}

fn notify(text: &str) {
    rfd::MessageDialog::new().set_title("Switch 2 Manager").set_description(i18n::t(text)).set_level(rfd::MessageLevel::Warning).show();
}

fn confirm(text: &str) -> bool {
    matches!(rfd::MessageDialog::new().set_title("Switch 2 Manager").set_description(i18n::t(text)).set_buttons(rfd::MessageButtons::YesNo).show(), rfd::MessageDialogResult::Yes)
}

/// Launches a game on its emulator, with cloud sync when enabled. No window of its own.
fn launch(game: Option<&str>, forced: Option<&str>) -> Result<()> {
    repair_setup()?;
    let s = load_settings();
    let emu_name = match (forced, game) {
        (Some(e), _) => e.to_string(),
        (None, Some(g)) => game_emulator(&s, Path::new(g).file_name().and_then(|n| n.to_str()).unwrap_or(g)),
        _ => s.default_emulator.clone(),
    };
    if emu::find_exe(&root().join(&emu_name)).is_none() {
        if !confirm(&format!("{emu_name} n'est pas installé. L'installer maintenant ?")) { return Ok(()); }
        emu::install(&emu_name)?;
    }
    let path = match game {
        Some(g) => {
            let p = root().join(g.replace(['\\', '/'], std::path::MAIN_SEPARATOR_STR));
            if !p.is_file() { bail!("Jeu introuvable : {}", p.display()); }
            Some(p)
        }
        None => None,
    };
    let do_sync = s.sync.enabled && !emu::any_running();
    if do_sync {
        match cloud::sync(&s, &|l, d, m| cloud::ask_conflict(l, d, m, true)) {
            Ok("cancelled") => return Ok(()),
            Ok(_) => {}
            Err(e) => {
                if !confirm(&format!("Vérification du cloud impossible :\n{e}\n\nJouer quand même ? Les versions seront comparées au prochain échange.")) { return Ok(()); }
            }
        }
    }
    emu::run(&emu_name, path.as_deref())?;
    if do_sync {
        if let Err(e) = cloud::sync(&load_settings(), &|l, d, m| cloud::ask_conflict(l, d, m, false)) {
            notify(&format!("Envoi vers le cloud impossible :\n{e}\n\nTa partie est bien gardée sur ce PC et sera envoyée au prochain échange."));
        }
    }
    Ok(())
}

fn window_icon() -> Option<Icon> {
    #[cfg(windows)]
    {
        use tao::platform::windows::IconExtWindows;
        Icon::from_resource(1, None).ok()
    }
    #[cfg(not(windows))]
    {
        let dec = png::Decoder::new(std::io::Cursor::new(include_bytes!("../assets/icon.png").as_slice()));
        let mut reader = dec.read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        buf.truncate(info.buffer_size());
        Icon::from_rgba(buf, info.width, info.height).ok()
    }
}

fn run_gui() -> Result<()> {
    let event_loop = EventLoopBuilder::<String>::with_user_event().build();
    let _ = PROXY.set(event_loop.create_proxy());
    let window = WindowBuilder::new().with_title("Switch 2 Manager").with_inner_size(LogicalSize::new(1120.0, 760.0)).with_min_inner_size(LogicalSize::new(900.0, 600.0)).with_window_icon(window_icon()).build(&event_loop)?;
    let mut context = WebContext::new(Some(manager_dir().join("webview")));
    let builder = WebViewBuilder::new_with_web_context(&mut context).with_html(HTML).with_devtools(false).with_ipc_handler(|req: wry::http::Request<String>| {
        let body = req.body().clone();
        std::thread::spawn(move || {
            let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let id = v["id"].as_u64().unwrap_or(0);
            let cmd = v["cmd"].as_str().unwrap_or("").to_string();
            let reply = match dispatch(&cmd, &v["args"]) {
                Ok(r) => json!({ "ok": true, "value": r }),
                Err(e) => json!({ "ok": false, "error": i18n::t(&e.to_string()) }),
            };
            to_js(format!("window.onReply({id},{reply})"));
        });
    });
    #[cfg(windows)]
    let webview = builder.build(&window)?;
    #[cfg(not(windows))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().ok_or_else(|| anyhow!("Conteneur GTK introuvable"))?)?
    };
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(js) => { let _ = webview.evaluate_script(&js); }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *flow = ControlFlow::Exit,
            _ => {}
        }
    })
}

fn main() {
    update::cleanup();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let get = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str);
    if args.first().map(String::as_str) == Some("--cli") {
        // Test mode: --cli <command> <json> <output file>
        let a: Value = serde_json::from_str(args.get(2).map(String::as_str).unwrap_or("{}")).unwrap_or(Value::Null);
        let out = match dispatch(args.get(1).map(String::as_str).unwrap_or(""), &a) { Ok(v) => json!({ "ok": true, "value": v }), Err(e) => json!({ "ok": false, "error": i18n::t(&e.to_string()) }) };
        if let Some(f) = args.get(3) { let _ = std::fs::write(f, out.to_string()); }
        return;
    }
    if args.first().map(String::as_str) == Some("--version") { if let Some(f) = args.get(1) { let _ = std::fs::write(f, env!("CARGO_PKG_VERSION")); } return; }
    let result = if let Some(g) = get("--launch") { launch(Some(g), get("--emu")) }
        else if let Some(e) = get("--open") { launch(None, Some(e)) }
        else { run_gui().map_err(|e| anyhow!(e)) };
    if let Err(e) = result { notify(&e.to_string()); }
}
