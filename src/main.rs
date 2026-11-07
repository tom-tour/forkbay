// Forkbay: one window for Switch 2 emulators, on Windows and Linux.
#![cfg_attr(not(test), windows_subsystem = "windows")]
#![recursion_limit = "256"]
mod art;
mod backup;
mod cloud;
mod compat;
mod config;
mod core;
mod emu;
mod i18n;
mod import;
mod install;
mod log;
mod report;
mod steam;
mod theme;
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
        "setupDone": s.setup_done, "theme": s.theme, "mode": s.mode, "gradient": s.gradient, "customSurfaces": s.custom_surfaces, "userThemes": s.user_themes, "customColors": s.custom_colors, "fullscreen": FULLSCREEN.load(std::sync::atomic::Ordering::Relaxed), "padTest": std::env::var("FORKBAY_PADTEST").unwrap_or_default(), "secTest": std::env::var("FORKBAY_SECTEST").is_ok(), "uiTest": std::env::var("FORKBAY_UITEST").unwrap_or_default(), "installPrompt": install::needed(), "fullscreenGames": s.fullscreen_games, "gameDirs": s.game_dirs, "coverSources": art::source_names(), "artSources": art::offered(&s), "artChoice": { "kinds": s.art_kinds, "source": s.art_source, "replace": s.art_replace }, "language": i18n::current(), "languageSetting": s.language, "languages": i18n::languages(), "termsAccepted": s.terms_accepted >= 1, "autoUpdateCheck": s.auto_update_check, "version": update::version(),
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
        if !list.is_empty() { d = d.add_filter(s_arg(a, "label"), &list); }
    }
    d.pick_file()
}

static FULLSCREEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const FULLSCREEN_EVENT: &str = "\u{1}fullscreen";

fn dispatch(cmd: &str, a: &Value) -> Result<Value> {
    let r = run_command(cmd, a);
    match &r {
        Err(e) => log::error(&format!("{cmd}: {e}")),
        Ok(_) if ["emu_install", "import", "move_data", "cloud_sync", "steam_save", "update_apply", "install_keys", "install_firmware", "emu_rollback", "backup_restore"].contains(&cmd) => log::info(&format!("{cmd}: ok")),
        _ => {}
    }
    r
}

fn run_command(cmd: &str, a: &Value) -> Result<Value> {
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
            if message.contains("redémarrage") && std::env::var("FORKBAY_SELFTEST").is_err() {
                std::process::Command::new(std::env::current_exe()?).spawn()?;
                std::thread::spawn(|| { std::thread::sleep(std::time::Duration::from_millis(600)); std::process::exit(0); });
            }
            msg(message)
        }
        "state" => {
            // Automated tests: proves that the page ran its script and reached the program.
            if let Ok(f) = std::env::var("FORKBAY_SELFTEST") { let _ = std::fs::write(f, "page-loaded"); }
            // Nothing is created next to a copy that may move on its first start.
            if !install::needed() { repair_setup()?; }
            state()?
        }
        "emu_status" => emu::status(),
        "emu_install" => msg(emu::install(&s_arg(a, "name"))?),
        "emu_add_github" => msg(emu::add_github(s_arg(a, "name").trim(), &s_arg(a, "repo"), s_arg(a, "filter").trim())?),
        "emu_add_local" => msg(emu::add_local(s_arg(a, "name").trim(), Path::new(&s_arg(a, "exe")))?),
        "emu_rollback" => msg(emu::rollback(&s_arg(a, "name"), &s_arg(a, "version"))?),
        "backup_list" => backup::list(),
        "backup_now" => msg(backup::now()?),
        "backup_restore" => msg(backup::restore(&s_arg(a, "name"))?),
        "backup_set" => { backup::set(a["enabled"].as_bool(), a["keep"].as_u64())?; json!({}) }
        "emu_remove" => msg(emu::remove(&s_arg(a, "name"))?),
        "set_game_emu" => { let mut s = load_settings(); s.games.insert(s_arg(a, "file"), s_arg(a, "emu")); save_settings(&s)?; json!({}) }
        "set_default_emu" => { let mut s = load_settings(); s.default_emulator = s_arg(a, "emu"); save_settings(&s)?; json!({}) }
        "set_theme" => { theme::set(a)?; json!({}) }
        "theme_import" => {
            let p = if a["path"].is_string() { Some(PathBuf::from(s_arg(a, "path"))) } else { rfd::FileDialog::new().add_filter("Forkbay", &["json"]).pick_file() };
            match p { Some(p) => json!({ "theme": theme::import(&p)? }), None => json!({}) }
        }
        "theme_export" => {
            let safe: String = a["theme"]["name"].as_str().unwrap_or("theme").chars().filter(|c| c.is_alphanumeric() || " _-".contains(*c)).collect();
            let p = if a["path"].is_string() { Some(PathBuf::from(s_arg(a, "path"))) } else { rfd::FileDialog::new().add_filter("Forkbay", &["json"]).set_file_name(format!("{safe}.forkbay-theme.json")).save_file() };
            match p { Some(p) => msg(theme::export(&p, &a["theme"])?), None => json!({}) }
        }
        "theme_remove" => { theme::remove(&s_arg(a, "name"))?; json!({}) }
        "log_js" => { log::error(&format!("page: {}", s_arg(a, "text"))); json!({}) }
        "log_open" => { let p = log::path(); std::fs::create_dir_all(p.parent().unwrap())?; if !p.exists() { std::fs::write(&p, "")?; } open_external(&p.to_string_lossy()); json!({}) }
        "selftest" => { if let Ok(f) = std::env::var("FORKBAY_SELFTEST") { std::fs::write(f, s_arg(a, "text"))?; } json!({}) }
        "selftest_crash" => { if std::env::var("FORKBAY_SELFTEST").is_ok() { panic!("test crash in {}", data_path().display()); } json!({}) }
        "quit" => std::process::exit(0),
        "finish_setup" => { let mut s = load_settings(); s.setup_done = true; save_settings(&s)?; json!({}) }
        "play" => { std::process::Command::new(std::env::current_exe()?).arg("--launch").arg(game_arg(&s_arg(a, "file"))).spawn()?; json!({}) }
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
            msg(steam::save(&s_arg(a, "account"), &games, a["tool"].as_bool().unwrap_or(false))?)
        }
        "fetch_art" => {
            let s = load_settings();
            let kinds = a["kinds"].as_array().map_or(s.art_kinds.clone(), |v| v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect());
            let source = a["source"].as_str().map_or(s.art_source.clone(), str::to_string);
            let replace = a["replace"].as_bool().unwrap_or(s.art_replace);
            msg(art::fetch(art::Choice { kinds, source, replace, file: s_arg(a, "file") })?)
        }
        "game_images" => art::images(&s_arg(a, "file")),
        "remove_image" => { art::remove_image(&s_arg(a, "file"), &s_arg(a, "kind"))?; json!({}) }
        "emu_settings_get" => config::get(),
        "emu_settings_set" => { config::set(&s_arg(a, "key"), a["value"].as_u64().ok_or_else(|| anyhow!("Valeur refusée."))?)?; json!({}) }
        "game_session" => json!({ "session": emu::session(), "other": a["other"].as_bool().unwrap_or(false).then(emu::any_running) }),
        "game_close" => msg(emu::close(a["force"].as_bool().unwrap_or(false))?),
        "set_image" => { art::set_image(&s_arg(a, "file"), &s_arg(a, "kind"), Path::new(&s_arg(a, "path")))?; json!({}) }
        "set_fullscreen_games" => { let mut s = load_settings(); s.fullscreen_games = a["enabled"].as_bool().unwrap_or(true); save_settings(&s)?; json!({}) }
        "game_dir_add" => msg(add_game_dir(Path::new(&s_arg(a, "path")))?),
        "game_dir_remove" => { remove_game_dir(&s_arg(a, "path"))?; json!({}) }
        "steam_ui_detect" => json!({ "active": steam::big_picture() }),
        "toggle_fullscreen" => {
            let on = !FULLSCREEN.load(std::sync::atomic::Ordering::Relaxed);
            FULLSCREEN.store(on, std::sync::atomic::Ordering::Relaxed);
            to_js(FULLSCREEN_EVENT.to_string());
            json!({ "fullscreen": on })
        }
        "install_options" => install::options(),
        "install_stay" => { install::stay()?; repair_setup()?; json!({}) }
        "install_to" => {
            let target = install::install(&s_arg(a, "kind"), &s_arg(a, "custom"), a["startMenu"].as_bool().unwrap_or(false), a["desktop"].as_bool().unwrap_or(false))?;
            if target != std::env::current_exe()? { install::restart_at(&target)?; }
            json!({ "path": target.to_string_lossy() })
        }
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
                "downloaded" => "Version du cloud installée. Une copie de la version locale est dans Save Backups.",
                _ => "Synchronisation annulée : rien n'a changé.",
            }.to_string())
        }
        "google_connect" => msg(cloud::google_connect()?),
        "open_path" => {
            let p = match s_arg(a, "which").as_str() { "roms" => root().join("Roms"), "covers" => art::cover_dir(), "sources" => art::sources_dir(), "data" => data_path(), _ => root() };
            std::fs::create_dir_all(&p)?;
            open_external(&p.to_string_lossy());
            json!({})
        }
        "open_url" => { let u = s_arg(a, "url"); if u.starts_with("https://") { open_external(&u); } json!({}) }
        _ => bail!("Commande inconnue : {cmd}"),
    })
}

fn notify(text: &str) {
    rfd::MessageDialog::new().set_title("Forkbay").set_description(i18n::t(text)).set_level(rfd::MessageLevel::Warning).show();
}

fn confirm(text: &str) -> bool {
    matches!(rfd::MessageDialog::new().set_title("Forkbay").set_description(i18n::t(text)).set_buttons(rfd::MessageButtons::YesNo).show(), rfd::MessageDialogResult::Yes)
}

/// Launches a game on its emulator, with cloud sync when enabled. No window of its own.
fn launch(game: Option<&str>, forced: Option<&str>) -> Result<()> {
    repair_setup()?;
    let s = load_settings();
    let emu_name = match (forced, game) {
        (Some(e), _) => e.to_string(),
        (None, Some(g)) => game_emulator(&s, &game_key_for(g)),
        _ => s.default_emulator.clone(),
    };
    if emu::find_exe(&root().join(&emu_name)).is_none() {
        if !confirm(&format!("{emu_name} n'est pas installé. L'installer maintenant ?")) { return Ok(()); }
        emu::install(&emu_name)?;
    }
    let path = match game {
        Some(g) => {
            let p = resolve_moved(&root().join(g.replace(['\\', '/'], std::path::MAIN_SEPARATOR_STR)));
            if !p.is_file() { bail!("Jeu introuvable : {}", p.display()); }
            Some(p)
        }
        None => None,
    };
    let idle = !emu::any_running();
    if idle {
        if let Err(e) = backup::auto() {
            log::error(&format!("automatic copy: {e}"));
            if !confirm(&format!("Copie automatique impossible :\n{e}\n\nJouer quand même ?")) { return Ok(()); }
        }
    }
    let do_sync = s.sync.enabled && idle;
    if do_sync {
        match cloud::sync(&s, &|l, d, m| cloud::ask_conflict(l, d, m, true)) {
            Ok("cancelled") => return Ok(()),
            Ok(_) => {}
            Err(e) => {
                if !confirm(&format!("Vérification du cloud impossible :\n{e}\n\nJouer quand même ? Les versions seront comparées à la prochaine synchronisation.")) { return Ok(()); }
            }
        }
    }
    emu::run(&emu_name, path.as_deref(), &game.map(game_key_for).unwrap_or_default())?;
    if do_sync {
        if let Err(e) = cloud::sync(&load_settings(), &|l, d, m| cloud::ask_conflict(l, d, m, false)) {
            notify(&format!("Envoi vers le cloud impossible :\n{e}\n\nTa sauvegarde reste bien sur ce PC et sera envoyée à la prochaine synchronisation."));
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

fn run_gui(fullscreen: bool) -> Result<()> {
    FULLSCREEN.store(fullscreen, std::sync::atomic::Ordering::Relaxed);
    let event_loop = EventLoopBuilder::<String>::with_user_event().build();
    let _ = PROXY.set(event_loop.create_proxy());
    let window = WindowBuilder::new().with_title("Forkbay").with_inner_size(LogicalSize::new(1120.0, 760.0)).with_min_inner_size(LogicalSize::new(900.0, 600.0)).with_window_icon(window_icon()).with_fullscreen(fullscreen.then_some(tao::window::Fullscreen::Borderless(None))).build(&event_loop)?;
    let mut context = WebContext::new(Some(manager_dir().join("webview")));
    let builder = WebViewBuilder::new_with_web_context(&mut context).with_html(HTML).with_devtools(false).with_navigation_handler(|url| {
        // The page is the only thing this window ever shows: links open in the browser.
        let ok = url == "about:blank" || url.starts_with("data:text/html");
        if !ok { log::error(&format!("navigation blocked: {url}")); }
        ok
    }).with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny).with_drag_drop_handler(|e| {
        if let wry::DragDropEvent::Drop { paths, position } = e {
            let list: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
            to_js(format!("window.onDropFiles({},{},{})", json!(list), position.0, position.1));
        }
        true
    }).with_ipc_handler(|req: wry::http::Request<String>| {
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
            Event::UserEvent(js) if js == FULLSCREEN_EVENT => {
                let on = FULLSCREEN.load(std::sync::atomic::Ordering::Relaxed);
                window.set_fullscreen(on.then_some(tao::window::Fullscreen::Borderless(None)));
            }
            Event::UserEvent(js) => { let _ = webview.evaluate_script(&js); }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *flow = ControlFlow::Exit,
            _ => {}
        }
    })
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--find") {
        // Steam launcher copy: it has no Files folder and must not create one.
        steam::find_and_run();
        return;
    }
    log::install_panic_hook();
    update::cleanup();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--cleanup-install") {
        if let Some(old) = args.get(1).cloned() {
            let old = PathBuf::from(old);
            // Waits for the old copy to close, without delaying this window. Tests need the result.
            if std::env::var("FORKBAY_INSTALLTEST").is_ok() { install::cleanup_old(&old); } else { std::thread::spawn(move || install::cleanup_old(&old)); }
        }
        args.drain(..args.len().min(2));
    }
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
        else { log::info(&format!("start {}", update::version())); run_gui(args.iter().any(|a| a == "--fullscreen") || (args.iter().any(|a| a == "--steam-ui") && steam::big_picture())).map_err(|e| anyhow!(e)) };
    if let Err(e) = result { log::error(&e.to_string()); notify(&e.to_string()); }
}
