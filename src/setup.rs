// The shared Data folder, the links that point each emulator at it, keys and firmware.
use crate::core::*;
use crate::files::*;
use crate::sys::*;
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

pub fn set_emulator_link(folder: &Path, data: &Path) -> Result<()> {
    let link = folder.join("user");
    if let Ok(meta) = fs::symlink_metadata(&link) {
        if !is_link(&meta) {
            bail!("{} est un dossier normal alors qu'un lien est attendu : déplace son contenu dans Data, puis supprime-le.", link.display());
        }
        if fs::canonicalize(&link).ok() == fs::canonicalize(data).ok() && fs::canonicalize(data).is_ok() {
            return Ok(());
        }
        remove_link(&link)?;
    }
    make_link(&link, data)
}

/// Points the emulator's folders at Data and its game folders at Roms.
fn point_paths(text: &str, data: &str, roms: &str) -> String {
    let new = Regex::new(r"(?m)^(nand|sdmc|load|dump|tas)_directory=[^\r\n]*").unwrap().replace_all(text, |c: &regex::Captures| format!("{0}_directory={data}/{0}", &c[1]));
    let new = Regex::new(r"(?m)^(Screenshots\\screenshot_path)=[^\r\n]*").unwrap().replace_all(&new, |c: &regex::Captures| format!("{}={data}/screenshots", &c[1]));
    Regex::new(r"(?m)^(Paths\\gamedirs\\\d+\\path)=([^\r\n]*)").unwrap().replace_all(&new, |c: &regex::Captures| {
        if c[2].contains(":/") || c[2].starts_with('/') || &c[2] == "__LOCAL__" { format!("{}={roms}", &c[1]) } else { c[0].to_string() }
    }).into_owned()
}

pub fn set_config_paths(data: &Path) -> Result<()> {
    let ini = data.join("config").join("qt-config.ini");
    let Ok(old) = fs::read_to_string(&ini) else { return Ok(()) };
    let new = point_paths(&old, &slash(data), &slash(&root().join("Roms")));
    if new != old {
        fs::write(&ini, new.as_bytes())?;
    }
    Ok(())
}

pub fn repair_setup() -> Result<PathBuf> {
    let data = data_path();
    for sub in ["config", "keys", "log", "load", "sdmc", "dump", "tas", "screenshots", "nand/user/save", "nand/system/Contents/registered"] {
        fs::create_dir_all(data.join(sub))?;
    }
    fs::create_dir_all(root().join("Roms"))?;
    fs::create_dir_all(root().join("Covers"))?;
    for folder in crate::emu::emulator_folders() {
        set_emulator_link(&folder, &data)?;
    }
    set_config_paths(&data)?;
    Ok(data)
}

// --- Keys and firmware ---
pub fn has_keys(data: &Path) -> bool {
    data.join("keys").join("prod.keys").is_file()
}

pub fn firmware_count(data: &Path) -> usize {
    fs::read_dir(data.join("nand/system/Contents/registered"))
        .map(|rd| rd.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".nca")).count())
        .unwrap_or(0)
}

/// Copies prod.keys (and title.keys when it sits beside it). Refuses a file that is not a key file.
pub fn install_keys(path: &Path) -> Result<String> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    if !name.ends_with(".keys") {
        bail!("Choisis ton fichier prod.keys.");
    }
    let text = fs::read_to_string(path).map_err(|_| anyhow!("Ce fichier n'est pas un fichier de clés lisible."))?;
    if !text.lines().any(|l| l.contains('=') && l.to_lowercase().contains("key")) {
        bail!("Ce fichier ne ressemble pas à un fichier de clés.");
    }
    let data = repair_setup()?;
    let dir = data.join("keys");
    fs::copy(path, dir.join("prod.keys"))?;
    let title = path.with_file_name("title.keys");
    let mut done = "prod.keys installé.";
    if title.is_file() {
        fs::copy(&title, dir.join("title.keys"))?;
        done = "prod.keys et title.keys installés.";
    }
    Ok(done.to_string())
}

/// Installs firmware from a zip or a folder of .nca files. Refuses anything else.
pub fn install_firmware(path: &Path) -> Result<String> {
    let data = repair_setup()?;
    let reg = data.join("nand/system/Contents/registered");
    let mut count = 0;
    if path.is_dir() {
        let mut files = vec![];
        walk_files(path, &mut files);
        for f in files.iter().filter(|f| f.to_string_lossy().ends_with(".nca")) {
            fs::copy(f, reg.join(f.file_name().unwrap()))?;
            count += 1;
        }
    } else {
        let mut zip = zip::ZipArchive::new(fs::File::open(path)?).map_err(|_| anyhow!("Ce fichier n'est pas une archive zip de firmware."))?;
        for i in 0..zip.len() {
            let mut e = zip.by_index(i)?;
            let Some(name) = e.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_owned())) else { continue };
            if !name.to_string_lossy().ends_with(".nca") {
                continue;
            }
            let mut out = fs::File::create(reg.join(name))?;
            std::io::copy(&mut e, &mut out)?;
            count += 1;
        }
    }
    if count == 0 {
        bail!("Aucun fichier .nca trouvé : ce n'est pas un firmware.");
    }
    Ok(format!("Firmware installé : {count} fichiers."))
}

/// Looks for keys and firmware already placed in Files.
pub fn find_local_keys() -> Option<PathBuf> {
    let rd = fs::read_dir(root()).ok()?;
    let dirs = std::iter::once(root()).chain(rd.flatten().filter(|e| e.path().is_dir()).filter(|e| !["Data", "Roms", "Roms Dumps", "Old Versions", "Save Backups", "Covers"].contains(&&*e.file_name().to_string_lossy())).map(|e| e.path()));
    dirs.map(|d| d.join("prod.keys")).find(|p| p.is_file())
}

pub fn find_local_firmware() -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(root()).ok()?.flatten().map(|e| e.path()).filter(|p| {
        p.is_file() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_lowercase().starts_with("firmware") && n.to_lowercase().ends_with(".zip"))
    }).collect();
    v.sort();
    v.pop()
}

/// Moves Data. The old copy stays until the user deletes it.
pub fn move_data(location: &str, custom: &str) -> Result<String> {
    if crate::emu::any_running() {
        bail!("Ferme les émulateurs avant de déplacer les données.");
    }
    let mut s = load_settings();
    let from = data_path_for(&s);
    s.data_location = location.into();
    s.custom_data_path = custom.into();
    let to = data_path_for(&s);
    if from == to {
        return Ok("Les données sont déjà à cet endroit.".into());
    }
    if to.exists() && fs::read_dir(&to)?.next().is_some() {
        bail!("Le dossier cible n'est pas vide : {}", to.display());
    }
    copy_tree(&from, &to, true)?;
    if tree_stats(&from) != tree_stats(&to) {
        bail!("La copie est incomplète : rien n'a changé.");
    }
    save_settings(&s)?;
    repair_setup()?;
    let mut stamp = 0;
    let old = loop {
        let name = format!("{}.old{}", from.file_name().unwrap().to_string_lossy(), if stamp == 0 { String::new() } else { format!("-{stamp}") });
        let p = from.with_file_name(name);
        if !p.exists() {
            break p;
        }
        stamp += 1;
    };
    fs::rename(&from, &old)?;
    Ok(format!("Données déplacées vers {}. L'ancienne copie reste dans {} : supprime-la quand tout fonctionne.", to.display(), old.display()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn emulator_paths_point_at_data_and_roms() {
        let old = "[Data Storage]\r\nnand_directory=C:/old/nand\r\nsdmc_directory=/home/x/sdmc\r\n[UI]\r\nScreenshots\\screenshot_path=C:/pics\r\nPaths\\gamedirs\\4\\path=D:/games\r\nPaths\\gamedirs\\1\\path=SDMC\r\n";
        let new = super::point_paths(old, "E:/F/Data", "E:/F/Roms");
        assert!(new.contains("nand_directory=E:/F/Data/nand\r\n"));
        assert!(new.contains("sdmc_directory=E:/F/Data/sdmc\r\n"));
        assert!(new.contains("Screenshots\\screenshot_path=E:/F/Data/screenshots\r\n"));
        assert!(new.contains("Paths\\gamedirs\\4\\path=E:/F/Roms\r\n"));
        assert!(new.contains("Paths\\gamedirs\\1\\path=SDMC\r\n"));
        assert_eq!(super::point_paths(&new, "E:/F/Data", "E:/F/Roms"), new);
    }
}
