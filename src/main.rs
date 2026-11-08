// Forkbay: one window for Switch 2 emulators, on Windows and Linux.
#![cfg_attr(not(test), windows_subsystem = "windows")]
#![recursion_limit = "256"]
mod art;
mod commands;
mod backup;
mod cloud;
mod compat;
mod config;
mod core;
mod emu;
mod files;
mod games;
mod i18n;
mod import;
mod install;
mod log;
mod net;
mod report;
mod setup;
mod steam;
mod sys;
mod theme;
mod update;

use crate::core::*;
use crate::games::*;
use crate::setup::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::OnceLock;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::{Icon, WindowBuilder};
use wry::{WebContext, WebViewBuilder};

// The page from ui/, put in one piece by build.rs.
const HTML: &str = include_str!(concat!(env!("OUT_DIR"), "/page.html"));

static PROXY: OnceLock<EventLoopProxy<String>> = OnceLock::new();

pub fn to_js(script: String) {
    if let Some(p) = PROXY.get() {
        let _ = p.send_event(script);
    }
}

pub fn emit_status(text: &str) {
    to_js(format!("window.onStatus({})", json!(i18n::t(text))));
}

pub static FULLSCREEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub const FULLSCREEN_EVENT: &str = "\u{1}fullscreen";

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
            let reply = match commands::dispatch(&cmd, &v["args"]) {
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
        let out = match commands::dispatch(args.get(1).map(String::as_str).unwrap_or(""), &a) { Ok(v) => json!({ "ok": true, "value": v }), Err(e) => json!({ "ok": false, "error": i18n::t(&e.to_string()) }) };
        if let Some(f) = args.get(3) { let _ = std::fs::write(f, out.to_string()); }
        return;
    }
    if args.first().map(String::as_str) == Some("--version") { if let Some(f) = args.get(1) { let _ = std::fs::write(f, env!("CARGO_PKG_VERSION")); } return; }
    let result = if let Some(g) = get("--launch") { launch(Some(g), get("--emu")) }
        else if let Some(e) = get("--open") { launch(None, Some(e)) }
        else { log::info(&format!("start {}", update::version())); run_gui(args.iter().any(|a| a == "--fullscreen") || (args.iter().any(|a| a == "--steam-ui") && steam::big_picture())).map_err(|e| anyhow!(e)) };
    if let Err(e) = result { log::error(&e.to_string()); notify(&e.to_string()); }
}
