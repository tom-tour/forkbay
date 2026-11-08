// Automatic copies of saves, profile and settings, with or without the cloud.
use crate::cloud;
use crate::core::*;
use crate::files::*;
use crate::setup::*;
use anyhow::{bail, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

const PREFIX: &str = "save-";

fn dir() -> PathBuf {
    root().join("Save Backups")
}

fn hash_file() -> PathBuf {
    dir().join("last-backup.txt")
}

fn stamp_of(name: &str) -> u64 {
    name.rsplit('-').next().and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn own_copies() -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = fs::read_dir(dir()).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(PREFIX))).collect()).unwrap_or_default();
    list.sort_by_key(|p| stamp_of(&p.file_name().unwrap_or_default().to_string_lossy()));
    list
}

fn rotate(keep: u32) {
    let list = own_copies();
    let keep = keep.max(1) as usize;
    if list.len() > keep {
        for p in &list[..list.len() - keep] {
            let _ = fs::remove_dir_all(p);
        }
    }
}

fn new_copy() -> Result<PathBuf> {
    let mut stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
    while dir().join(format!("{PREFIX}{stamp}")).exists() {
        stamp += 1;
    }
    let dest = dir().join(format!("{PREFIX}{stamp}"));
    cloud::copy_synced(&data_path(), &dest)?;
    Ok(dest)
}

/// Makes a copy when something changed since the last one. Returns true when it did.
fn make_if_changed(keep: u32) -> Result<bool> {
    let data = data_path();
    if cloud::sync_files(&data).is_empty() {
        return Ok(false);
    }
    let hash = cloud::local_hash(&data)?;
    let same = fs::read_to_string(hash_file()).is_ok_and(|h| h.trim() == hash);
    if same && !own_copies().is_empty() {
        return Ok(false);
    }
    new_copy()?;
    fs::write(hash_file(), &hash)?;
    rotate(keep);
    Ok(true)
}

/// Called before playing and before changing an emulator version.
pub fn auto() -> Result<()> {
    let s = load_settings();
    if s.auto_backup {
        make_if_changed(s.backup_keep)?;
    }
    Ok(())
}

pub fn now() -> Result<String> {
    Ok(if make_if_changed(load_settings().backup_keep)? { "Copie enregistrée dans Save Backups." } else { "Rien n'a changé depuis la dernière copie." }.to_string())
}

pub fn list() -> Value {
    let s = load_settings();
    let valid = Regex::new(r"^(save|auto)-\d+$").unwrap();
    let mut rows: Vec<(u64, Value)> = fs::read_dir(dir()).map(|rd| rd.flatten().filter(|e| e.path().is_dir()).filter_map(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        if !valid.is_match(&name) {
            return None;
        }
        let mut files = vec![];
        walk_files(&e.path(), &mut files);
        let stamp = stamp_of(&name);
        Some((stamp, json!({ "name": name, "date": cloud::format_date(stamp), "files": files.len(), "cloud": name.starts_with("auto-") })))
    }).collect()).unwrap_or_default();
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    json!({ "enabled": s.auto_backup, "keep": s.backup_keep, "copies": rows.into_iter().map(|r| r.1).collect::<Vec<_>>() })
}

pub fn set(enabled: Option<bool>, keep: Option<u64>) -> Result<()> {
    let mut s = load_settings();
    if let Some(e) = enabled {
        s.auto_backup = e;
    }
    if let Some(k) = keep {
        s.backup_keep = k.clamp(1, 100) as u32;
    }
    save_settings(&s)
}

/// Puts a copy back. The current state is copied first, and comes back if anything fails.
pub fn restore(name: &str) -> Result<String> {
    if crate::emu::any_running() {
        bail!("Ferme les émulateurs avant de restaurer.");
    }
    let from = dir().join(name);
    if !Regex::new(r"^(save|auto)-\d+$").unwrap().is_match(name) || !from.is_dir() {
        bail!("Cette copie n'existe plus.");
    }
    let data = data_path();
    let safety = new_copy()?;
    let attempt = (|| -> Result<()> {
        cloud::clear_synced(&data)?;
        copy_tree(&from, &data, true)?;
        set_config_paths(&data)
    })();
    if let Err(e) = attempt {
        let _ = cloud::clear_synced(&data);
        copy_tree(&safety, &data, true)?;
        set_config_paths(&data)?;
        return Err(e);
    }
    rotate(load_settings().backup_keep);
    Ok("Copie restaurée. L'état précédent est gardé dans Save Backups.".to_string())
}
