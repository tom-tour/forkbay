// The game list: Files/Roms and the folders the user added, names and chosen emulators.
use crate::core::*;
use anyhow::{bail, Result};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

pub const GAME_EXTENSIONS: [&str; 3] = ["nup", "nsp", "xci"];

// --- Games ---
fn is_game(p: &Path) -> bool {
    p.is_file() && p.extension().and_then(|x| x.to_str()).is_some_and(|x| GAME_EXTENSIONS.contains(&x.to_lowercase().as_str()))
}

/// A folder on the program's own disk still works after its drive letter changes.
pub fn resolve_moved(p: &Path) -> PathBuf {
    #[cfg(windows)]
    if !p.exists() {
        let (old, exe) = (p.to_string_lossy().into_owned(), exe_dir().to_string_lossy().into_owned());
        if old.get(1..2) == Some(":") && exe.get(1..2) == Some(":") {
            let moved = PathBuf::from(format!("{}{}", &exe[..2], &old[2..]));
            if moved.exists() { return moved; }
        }
    }
    p.to_path_buf()
}

// Bounded, so that choosing a whole disk stays quick.
fn scan_games(dir: &Path, depth: u32, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if out.len() >= 5000 { return; }
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_symlink() { continue; }
        let p = e.path();
        if kind.is_dir() {
            if depth > 0 { scan_games(&p, depth - 1, out); }
        } else if is_game(&p) {
            out.push(p.to_string_lossy().into_owned());
        }
    }
}

/// Games in Files/Roms are known by their file name, those in other folders by their full path.
pub fn rom_files() -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(root().join("Roms"))
        .map(|rd| rd.flatten().filter(|e| is_game(&e.path())).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    for dir in load_settings().game_dirs {
        scan_games(&resolve_moved(Path::new(&dir)), 3, &mut v);
    }
    v.sort_by_key(|f| display_name(f).to_lowercase());
    v.dedup();
    v
}

/// The argument of --launch for a game.
pub fn game_arg(file: &str) -> String {
    if Path::new(file).is_absolute() { file.to_string() } else { format!("Roms/{file}") }
}

fn game_key(arg: &str) -> &str {
    arg.strip_prefix("Roms/").or_else(|| arg.strip_prefix("Roms\\")).unwrap_or(arg)
}

pub fn game_key_for(arg: &str) -> String {
    game_key(arg).to_string()
}

pub fn add_game_dir(path: &Path) -> Result<String> {
    if !path.is_dir() { bail!("Choisis un dossier."); }
    let same = |a: &Path, b: &Path| fs::canonicalize(a).ok().is_some_and(|x| fs::canonicalize(b).ok() == Some(x));
    if same(path, &root().join("Roms")) { bail!("Ce dossier est déjà utilisé."); }
    let mut s = load_settings();
    if s.game_dirs.iter().any(|d| same(Path::new(d), path)) { bail!("Ce dossier est déjà utilisé."); }
    s.game_dirs.push(path.to_string_lossy().into_owned());
    save_settings(&s)?;
    let mut found = vec![];
    scan_games(path, 3, &mut found);
    Ok(format!("Dossier ajouté : {} jeu(x) trouvé(s). Rien n'a été déplacé.", found.len()))
}

pub fn remove_game_dir(path: &str) -> Result<()> {
    let mut s = load_settings();
    s.game_dirs.retain(|d| d != path);
    save_settings(&s)
}

pub fn display_name(file: &str) -> String {
    let stem = Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or(file);
    let clean = Regex::new(r"\s*[\[(][^\])]*[\])]").unwrap().replace_all(stem, "").trim().to_string();
    if clean.is_empty() { stem.to_string() } else { clean }
}

pub fn game_emulator(s: &Settings, file: &str) -> String {
    let exact = s.games.get(file).filter(|e| !e.is_empty());
    // A game in another folder keeps its emulator when its drive letter changes.
    let name = Path::new(file).file_name();
    let same_name = || s.games.iter().find(|(k, e)| !e.is_empty() && Path::new(k.as_str()).is_absolute() && Path::new(k.as_str()).file_name() == name).map(|(_, e)| e);
    exact.or_else(same_name).cloned().unwrap_or_else(|| s.default_emulator.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_drops_tags() {
        assert_eq!(display_name("Super Game [0100000000010000][v0] (EU).nsp"), "Super Game");
        assert_eq!(display_name("[0100000000010000].xci"), "[0100000000010000]");
        assert_eq!(display_name("Plain.nup"), "Plain");
    }

    #[test]
    fn games_in_roms_are_known_by_name() {
        assert_eq!(game_arg("A.nsp"), "Roms/A.nsp");
        assert_eq!(game_key_for("Roms/A.nsp"), "A.nsp");
        assert_eq!(game_key_for("Roms\\A.nsp"), "A.nsp");
        let abs = std::env::temp_dir().join("A.nsp").to_string_lossy().into_owned();
        assert_eq!(game_arg(&abs), abs);
        assert_eq!(game_key_for(&abs), abs);
    }

    #[test]
    fn emulator_follows_a_game_moved_to_another_drive() {
        let mut s = Settings::default();
        let old = std::env::temp_dir().join("old").join("B [0100000000010000].nsp");
        let new = std::env::temp_dir().join("new").join("B [0100000000010000].nsp");
        s.games.insert(old.to_string_lossy().into_owned(), "Eden".into());
        s.games.insert("C.nsp".into(), String::new());
        assert_eq!(game_emulator(&s, &new.to_string_lossy()), "Eden");
        assert_eq!(game_emulator(&s, "C.nsp"), s.default_emulator);
    }
}
