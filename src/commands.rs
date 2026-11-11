// Everything the page can ask for: one name per command, and the state it draws from.
use crate::core::*;
use crate::games::*;
use crate::setup::*;
use crate::sys::*;
use crate::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

fn p_arg(a: &Value) -> Result<usize> {
    a["player"].as_u64().map(|p| p as usize).ok_or_else(|| anyhow!("Joueur inconnu."))
}

fn s_arg(a: &Value, k: &str) -> String {
    a[k].as_str().unwrap_or("").to_string()
}

fn state() -> Result<Value> {
    let s = load_settings();
    let data = data_path();
    let times = playtime::Times::read();
    let games: Vec<Value> = rom_files().iter().map(|f| { let (played, last) = times.of(f); json!({ "file": f, "name": display_name(f), "emu": game_emulator(&s, f), "cover": art::cover_uri(f), "own": config::has_own(f), "hasId": config::game_id(f).is_some(), "played": played, "last": last }) }).collect();
    let emus: Vec<Value> = emu::list().iter().map(|e| json!({ "name": e.name, "present": emu::find_exe(&root().join(&e.name)).is_some(), "available": e.user || !(if cfg!(windows) { &e.asset } else { &e.linux_asset }).is_empty() })).collect();
    Ok(json!({
        "setupDone": s.setup_done, "theme": s.theme, "mode": s.mode, "gradient": s.gradient, "customSurfaces": s.custom_surfaces, "userThemes": s.user_themes, "customColors": s.custom_colors, "fullscreen": FULLSCREEN.load(Ordering::Relaxed), "padTest": std::env::var("FORKBAY_PADTEST").unwrap_or_default(), "secTest": std::env::var("FORKBAY_SECTEST").is_ok(), "uiTest": std::env::var("FORKBAY_UITEST").unwrap_or_default(), "installPrompt": install::needed(), "fullscreenGames": s.fullscreen_games, "gameDirs": s.game_dirs, "coverSources": art::source_names(), "artSources": art::offered(&s), "artChoice": { "kinds": s.art_kinds, "source": s.art_source, "replace": s.art_replace }, "language": i18n::current(), "languageSetting": s.language, "languages": i18n::languages(), "termsAccepted": s.terms_accepted >= 1, "autoUpdateCheck": s.auto_update_check, "version": update::version(),
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

pub fn dispatch(cmd: &str, a: &Value) -> Result<Value> {
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
        "emu_install" => msg(emu::install(&s_arg(a, "name"), &s_arg(a, "tag"))?),
        "emu_versions" => emu::versions(&s_arg(a, "name"))?,
        "emu_add_github" => msg(emu::add_github(s_arg(a, "name").trim(), &s_arg(a, "repo"), s_arg(a, "filter").trim())?),
        "emu_add_local" => msg(emu::add_local(s_arg(a, "name").trim(), Path::new(&s_arg(a, "exe")))?),
        "emu_rollback" => msg(emu::rollback(&s_arg(a, "name"), &s_arg(a, "version"))?),
        "backup_list" => backup::list(),
        "backup_now" => msg(backup::now()?),
        "backup_restore" => msg(backup::restore(&s_arg(a, "name"))?),
        "backup_set" => { backup::set(a["enabled"].as_bool(), a["keep"].as_u64())?; json!({}) }
        "emu_clear_shaders" => msg(emu::clear_shaders(&s_arg(a, "name"))?),
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
        "play" => { crate::sdl::stop(); std::process::Command::new(std::env::current_exe()?).arg("--launch").arg(game_arg(&s_arg(a, "file"))).spawn()?; json!({}) }
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
        "game_reports" => {
            let file = s_arg(a, "file");
            let emulator = game_emulator(&load_settings(), &file);
            let (played, _) = crate::playtime::Times::read().of(&file);
            let name = display_name(&file);
            reports::game(&reports::Setup { name: &name, id: config::game_id(&file), emulator: &emulator, build: &emu::installed(&emulator), played })
        }
        "remove_image" => { art::remove_image(&s_arg(a, "file"), &s_arg(a, "kind"))?; json!({}) }
        "emu_settings_get" => config::get(),
        "emu_settings_set" => { config::set(&s_arg(a, "key"), a["value"].as_u64().ok_or_else(|| anyhow!("Valeur refusée."))?)?; json!({}) }
        "game_settings_get" => config::game_get(&s_arg(a, "file"))?,
        "game_settings_set" => {
            let value = if a["value"].is_null() { None } else { Some(a["value"].as_u64().ok_or_else(|| anyhow!("Valeur refusée."))?) };
            config::game_set(&s_arg(a, "file"), &s_arg(a, "key"), value)?;
            json!({})
        }
        "game_settings_other_common" => { config::game_other_common(&s_arg(a, "file"), &s_arg(a, "section"), &s_arg(a, "key"))?; json!({}) }
        "controls_get" => controls::get(),
        "controls_devices" => controls::devices()?,
        "controls_states" => controls::states()?,
        "controls_set" => { controls::set_control(p_arg(a)?, &s_arg(a, "name"), &s_arg(a, "value"))?; json!({}) }
        "controls_connected" => { controls::set_connected(p_arg(a)?, a["on"].as_bool().unwrap_or(true))?; json!({}) }
        "controls_type" => { controls::set_type(p_arg(a)?, a["type"].as_u64().ok_or_else(|| anyhow!("Valeur refusée."))? as u32)?; json!({}) }
        "controls_vibration" => { controls::set_vibration(p_arg(a)?, a["on"].as_bool(), a["strength"].as_u64().map(|v| v as u32))?; json!({}) }
        "controls_advanced" => { controls::set_advanced(&s_arg(a, "key"), a["on"].as_bool().unwrap_or(false))?; json!({}) }
        "controls_docked" => { controls::set_docked(a["on"].as_bool().unwrap_or(true))?; json!({}) }
        "hotkeys_get" => hotkeys::get(),
        "hotkeys_set" => { hotkeys::set(&s_arg(a, "id"), &s_arg(a, "field"), &s_arg(a, "value"))?; json!({}) }
        "hotkeys_reset" => { hotkeys::reset()?; json!({}) }
        "controls_clear" => { controls::clear(p_arg(a)?)?; json!({}) }
        "controls_device" => { controls::use_device(p_arg(a)?, &a["device"])?; json!({}) }
        "controls_capture" => {
            let only = a["guid"].as_str().map(|g| (g.to_string(), a["port"].as_u64().unwrap_or(0) as usize));
            controls::capture_begin(&s_arg(a, "kind"), only)?;
            json!({})
        }
        "controls_poll" => controls::capture_poll()?,
        "controls_nav_device" => { controls::nav_device(&a["device"])?; json!({}) }
        "controls_nav" => { controls::set_nav(&a["controls"])?; json!({}) }
        "controls_release" => { crate::sdl::stop(); json!({}) }
        "controls_stop" => { controls::capture_stop()?; json!({}) }
        "profile_save" => { controls::save_profile(&s_arg(a, "name"), p_arg(a)?, a["new"].as_bool().unwrap_or(false))?; json!({}) }
        "profile_load" => { controls::load_profile(&s_arg(a, "name"), p_arg(a)?)?; json!({}) }
        "profile_delete" => { controls::delete_profile(&s_arg(a, "name"))?; json!({}) }
        "game_profiles" => controls::game_profiles(&s_arg(a, "file"))?,
        "game_profile_set" => { controls::set_game_profile(&s_arg(a, "file"), p_arg(a)?, &s_arg(a, "name"))?; json!({}) }
        "custom_games" => config::custom_games(),
        "art_sources" => art::source_list(&load_settings()),
        "art_source_enable" => { art::set_enabled(&s_arg(a, "name"), a["enabled"].as_bool().unwrap_or(true))?; json!({}) }
        "art_source_add" => {
            let p = if a["path"].is_string() { Some(PathBuf::from(s_arg(a, "path"))) } else { rfd::FileDialog::new().set_title(i18n::t("Choisis une source d'images")).add_filter("JSON", &["json"]).pick_file() };
            match p { Some(p) => msg(art::add_source(&p)?), None => json!({}) }
        }
        "grid_key_test" => msg(art::test_key(&s_arg(a, "key"))?),
        "grid_key_remove" => { let mut s = load_settings(); s.steam_grid_key.clear(); save_settings(&s)?; json!({}) }
        "game_session" => json!({ "session": emu::session(), "other": a["other"].as_bool().unwrap_or(false).then(emu::any_running) }),
        "game_close" => msg(emu::close(a["force"].as_bool().unwrap_or(false))?),
        "set_image" => { art::set_image(&s_arg(a, "file"), &s_arg(a, "kind"), Path::new(&s_arg(a, "path")))?; json!({}) }
        "set_fullscreen_games" => { let mut s = load_settings(); s.fullscreen_games = a["enabled"].as_bool().unwrap_or(true); save_settings(&s)?; json!({}) }
        "game_dir_add" => msg(add_game_dir(Path::new(&s_arg(a, "path")))?),
        "game_dir_remove" => { remove_game_dir(&s_arg(a, "path"))?; json!({}) }
        "steam_ui_detect" => json!({ "active": steam::big_picture() }),
        "toggle_fullscreen" => {
            let on = !FULLSCREEN.load(Ordering::Relaxed);
            FULLSCREEN.store(on, Ordering::Relaxed);
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
