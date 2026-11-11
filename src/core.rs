// Settings and paths. Everything is relative to the executable.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Sync {
    pub enabled: bool,
    pub provider: String,
    pub folder_path: String,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub google_refresh_token: String,
    pub keep_backups: u32,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub setup_done: bool,
    pub theme: String,
    pub language: String,
    pub auto_update_check: bool,
    pub terms_accepted: u32,
    pub custom_colors: BTreeMap<String, String>,
    pub mode: String,
    pub gradient: bool,
    pub custom_surfaces: bool,
    pub user_themes: Vec<serde_json::Value>,
    pub steam_grid_key: String,
    pub data_location: String,
    pub custom_data_path: String,
    pub default_emulator: String,
    pub games: BTreeMap<String, String>,
    pub sync: Sync,
    pub auto_backup: bool,
    pub backup_keep: u32,
    pub fullscreen_games: bool,
    pub game_dirs: Vec<String>,
    pub install_asked: bool,
    pub art_kinds: Vec<String>,
    pub art_source: String,
    pub art_replace: bool,
    pub art_disabled: Vec<String>,
    /// The controls that move around NX2 Launcher itself. None: those of player 1.
    pub nav_controls: Option<BTreeMap<String, String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            setup_done: false,
            theme: "aurora".into(),
            language: "auto".into(),
            auto_update_check: true,
            terms_accepted: 0,
            custom_colors: BTreeMap::new(),
            mode: "dark".into(),
            gradient: true,
            custom_surfaces: false,
            user_themes: vec![],
            steam_grid_key: String::new(),
            data_location: "portable".into(),
            custom_data_path: String::new(),
            default_emulator: "Fritchii".into(),
            games: BTreeMap::new(),
            sync: Sync { provider: "folder".into(), keep_backups: 10, ..Default::default() },
            auto_backup: true,
            backup_keep: 10,
            fullscreen_games: true,
            game_dirs: vec![],
            install_asked: false,
            art_kinds: ["grid", "wide", "hero", "logo", "icon"].map(String::from).to_vec(),
            art_source: "all".into(),
            art_replace: false,
            art_disabled: vec![],
            nav_controls: None,
        }
    }
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| ".".into())
}

/// The Files folder next to the executable (FORKBAY_ROOT overrides it for tests).
pub fn root() -> PathBuf {
    if let Ok(r) = std::env::var("FORKBAY_ROOT") {
        return r.into();
    }
    exe_dir().join("Files")
}

pub fn manager_dir() -> PathBuf {
    root().join("Manager")
}

pub fn load_settings() -> Settings {
    let path = manager_dir().join("settings.json");
    let mut s: Settings = match fs::read_to_string(&path) {
        // Text editors may add a byte order mark: it must not reset the settings.
        Ok(t) => serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap_or_else(|e| {
            // A damaged file is kept aside instead of being overwritten by the next save.
            let _ = fs::copy(&path, path.with_extension("json.damaged"));
            // Written directly: the normal journal reads the settings, which would loop here.
            crate::log::raw("ERROR", &format!("settings.json unreadable: {e}"));
            Settings::default()
        }),
        Err(_) => Settings::default(),
    };
    crate::theme::migrate(&mut s);
    // Emulators renamed since these settings were saved.
    s.default_emulator = crate::emu::current_name(&s.default_emulator);
    for e in s.games.values_mut() {
        if !e.is_empty() { *e = crate::emu::current_name(e); }
    }
    s
}

pub fn save_settings(s: &Settings) -> Result<()> {
    fs::create_dir_all(manager_dir())?;
    let path = manager_dir().join("settings.json");
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn local_data_base() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| ".".into())
    } else {
        home().join(".local/share")
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| ".".into())
}

pub fn data_path_for(s: &Settings) -> PathBuf {
    match s.data_location.as_str() {
        "local" => {
            // Kept where it was when the program was still called Forkbay.
            let old = local_data_base().join("Forkbay").join("Data");
            if old.is_dir() { old } else { local_data_base().join("NX2 Launcher").join("Data") }
        }
        "custom" if !s.custom_data_path.is_empty() => PathBuf::from(&s.custom_data_path),
        _ => root().join("Data"),
    }
}

pub fn data_path() -> PathBuf {
    data_path_for(&load_settings())
}

pub fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

