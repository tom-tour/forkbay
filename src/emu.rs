// Emulator list, version checks, installation and launching.
use crate::core::*;
use crate::files::*;
use crate::setup::*;
use crate::sys::*;
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Emu {
    pub name: String,
    pub source: String,
    pub repo: String,
    pub base: String,
    pub tag: String,
    pub asset: String,
    pub asset_exclude: String,
    pub linux_asset: String,
    pub about: String,
    pub user: bool,
    /// Repositories this emulator was published under before.
    pub old_repos: Vec<String>,
    /// Names Forkbay gave this emulator before.
    pub old_names: Vec<String>,
}

pub struct Release {
    pub tag: String,
    pub date: String,
    pub url: String,
    pub size: u64,
    pub digest: String,
    pub page: String,
    pub asset: String,
    pub notes: String,
}

fn user_file() -> PathBuf {
    manager_dir().join("user-emulators.json")
}

fn user_list() -> Vec<Emu> {
    fs::read_to_string(user_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_user_list(list: &[Emu]) -> Result<()> {
    fs::create_dir_all(manager_dir())?;
    fs::write(user_file(), serde_json::to_vec_pretty(list)?)?;
    Ok(())
}

pub fn list() -> Vec<Emu> {
    let mut all: Vec<Emu> = serde_json::from_str(include_str!("../emulators.json")).expect("emulators.json");
    for mut e in user_list() {
        if !all.iter().any(|x| x.name == e.name) {
            e.user = true;
            all.push(e);
        }
    }
    all
}

/// The name an emulator has now, for a name saved by an older Forkbay ("Lichii" is now "Fritchii").
/// A folder still holding an emulator under the old name keeps it.
pub fn current_name(name: &str) -> String {
    let all = list();
    if all.iter().any(|e| e.name == name) || find_exe(&root().join(name)).is_some() {
        return name.to_string();
    }
    all.into_iter().find(|e| e.old_names.iter().any(|o| o == name)).map(|e| e.name).unwrap_or_else(|| name.to_string())
}

/// Gives a renamed emulator its new name on disk: its folder, its kept versions and its shader cache.
/// Only a folder holding a version of that emulator (or of no known version) moves.
pub fn migrate_renamed() {
    for e in list() {
        let Ok(tag) = Regex::new(if e.tag.is_empty() { "." } else { &e.tag }) else { continue };
        for old in &e.old_names {
            let (from, to) = (root().join(old), root().join(&e.name));
            if find_exe(&from).is_none() || to.exists() || !{ let v = installed(old); v.is_empty() || tag.is_match(&v) } {
                continue;
            }
            if any_running() {
                crate::log::info(&format!("{old} not renamed to {}: an emulator is running", e.name));
                return;
            }
            if let Err(err) = fs::rename(&from, &to) {
                crate::log::error(&format!("{old} not renamed to {}: {err}", e.name));
                continue;
            }
            crate::log::info(&format!("{old} renamed to {}", e.name));
            let shaders = data_path().join("cache").join("shaders");
            if shaders.join(old).is_dir() && !shaders.join(&e.name).exists() {
                let _ = fs::rename(shaders.join(old), shaders.join(&e.name));
            }
            let kept = root().join("Old Versions");
            for d in fs::read_dir(&kept).map(|rd| rd.flatten().collect::<Vec<_>>()).unwrap_or_default() {
                let n = d.file_name().to_string_lossy().into_owned();
                if let Some(label) = n.strip_prefix(&format!("{old} ")) {
                    if tag.is_match(label) && !kept.join(format!("{} {label}", e.name)).exists() {
                        let _ = fs::rename(d.path(), kept.join(format!("{} {label}", e.name)));
                    }
                }
            }
        }
    }
}

pub fn find_exe(folder: &Path) -> Option<PathBuf> {
    if let Ok(name) = fs::read_to_string(folder.join("exe.txt")) {
        let p = folder.join(name.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    for name in ["lichii.exe", "eglantine.exe"] {
        let p = folder.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub fn emulator_folders() -> Vec<PathBuf> {
    fs::read_dir(root()).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && find_exe(p).is_some()).collect()).unwrap_or_default()
}

pub fn any_running() -> bool {
    let mut names: Vec<String> = vec!["lichii".into(), "eglantine".into(), "eden-cli".into()];
    for f in emulator_folders() {
        if let Some(n) = find_exe(&f).and_then(|e| e.file_stem().map(|s| s.to_string_lossy().to_lowercase())) {
            names.push(n);
        }
    }
    running_processes().iter().any(|p| names.iter().any(|n| p == n || (n.len() > 15 && n.starts_with(p.as_str()))))
}

fn installed(name: &str) -> String {
    fs::read_to_string(root().join(name).join("version.txt")).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn api_url(e: &Emu) -> String {
    if e.source == "forgejo" { format!("{}/api/v1/repos/{}/releases?limit=40", e.base, e.repo) } else { format!("https://api.github.com/repos/{}/releases?per_page=40", e.repo) }
}

fn page_url(e: &Emu) -> String {
    if e.source == "forgejo" { format!("{}/{}/releases", e.base, e.repo) } else { format!("https://github.com/{}/releases", e.repo) }
}

/// Releases usable on this system, newest first.
pub fn releases(e: &Emu) -> Result<Vec<Release>> {
    let wanted = if cfg!(windows) { &e.asset } else { &e.linux_asset };
    if wanted.is_empty() {
        bail!("Pas de version pour ce système.");
    }
    let wanted = Regex::new(wanted)?;
    let exclude = if e.asset_exclude.is_empty() { None } else { Some(Regex::new(&e.asset_exclude)?) };
    let tag = Regex::new(if e.tag.is_empty() { "." } else { &e.tag })?;
    let list = crate::net::get_json_cached(&api_url(e), 300).map_err(|err| if err.to_string().starts_with("GitHub") { err } else { anyhow!("Réseau : {err}") })?;
    let mut found: Vec<Release> = vec![];
    for r in list.as_array().into_iter().flatten() {
        let t = r["tag_name"].as_str().unwrap_or("");
        if r["draft"].as_bool().unwrap_or(false) || !tag.is_match(t) || found.iter().any(|f| f.tag == t) {
            continue;
        }
        let Some(a) = r["assets"].as_array().into_iter().flatten().find(|a| {
            let n = a["name"].as_str().unwrap_or("");
            wanted.is_match(n) && !exclude.as_ref().is_some_and(|x| x.is_match(n))
        }) else { continue };
        found.push(Release {
            tag: t.into(),
            date: r["published_at"].as_str().unwrap_or("").to_string(),
            url: a["browser_download_url"].as_str().unwrap_or("").into(),
            size: a["size"].as_u64().unwrap_or(0),
            digest: a["digest"].as_str().unwrap_or("").into(),
            page: r["html_url"].as_str().unwrap_or("").into(),
            asset: a["name"].as_str().unwrap_or("").into(),
            notes: r["body"].as_str().unwrap_or("").chars().take(4000).collect(),
        });
    }
    found.sort_by(|a, b| b.date.cmp(&a.date));
    if found.is_empty() {
        bail!("Aucune version trouvée pour ce système.");
    }
    Ok(found)
}

pub fn latest(e: &Emu) -> Result<Release> {
    Ok(releases(e)?.remove(0))
}

/// The versions one can switch to: every release, and the ones already kept in Old Versions.
pub fn versions(name: &str) -> Result<Value> {
    let e = list().into_iter().find(|x| x.name == name).ok_or_else(|| anyhow!("Émulateur inconnu : {name}"))?;
    let inst = installed(name);
    let mut old = old_versions(name);
    let (rels, error) = if e.source == "local" { (vec![], String::new()) } else {
        match releases(&e) { Ok(r) => (r, String::new()), Err(err) => (vec![], err.to_string()) }
    };
    let mut rows: Vec<Value> = rels.iter().map(|r| {
        // A kept version is named "<emulator> <tag>": switching to it needs no download.
        let kept = old.iter().position(|o| o == &format!("{name} {}", r.tag)).map(|i| old.remove(i)).unwrap_or_default();
        json!({ "tag": r.tag, "date": r.date.get(..10).unwrap_or(""), "size": r.size, "old": kept, "current": r.tag == inst, "notes": r.notes })
    }).collect();
    rows.extend(old.into_iter().map(|o| json!({ "tag": o.strip_prefix(&format!("{name} ")).unwrap_or(&o), "date": "", "size": 0, "old": o, "current": false, "notes": "" })));
    Ok(json!({ "versions": rows, "error": error }))
}

pub fn status() -> Value {
    // An emulator with no build for this system is only shown once installed.
    let rows: Vec<Value> = list().iter().filter(|e| e.user || find_exe(&root().join(&e.name)).is_some() || !(if cfg!(windows) { &e.asset } else { &e.linux_asset }).is_empty()).map(|e| {
        let folder = root().join(&e.name);
        let present = find_exe(&folder).is_some();
        let local = e.source == "local";
        let rels = if local { Ok(vec![]) } else { releases(e) };
        if let (true, Ok(r)) = (present, rels.as_ref()) {
            check_outside(&e.name, r);
        }
        // Stays until Forkbay installs a version again: the one before was not kept.
        let outside = folder.join("outside.txt").exists();
        let inst = installed(&e.name);
        let mut row = json!({ "name": e.name, "about": e.about, "present": present, "installed": inst, "latest": "", "date": "", "size": 0, "page": page_url(e), "upToDate": false, "error": "", "user": e.user, "local": local, "notes": "", "old": old_versions(&e.name), "shaders": shader_cache_size(&data_path(), &e.name), "outside": outside });
        if !local {
            match rels {
                Ok(r) => {
                    let l = &r[0];
                    row["latest"] = json!(l.tag);
                    row["date"] = json!(l.date.get(..10).unwrap_or(""));
                    row["size"] = json!(l.size);
                    row["notes"] = json!(l.notes);
                    if !l.page.is_empty() { row["page"] = json!(l.page); }
                    row["upToDate"] = json!(present && inst == l.tag);
                }
                Err(err) => row["error"] = json!(err.to_string()),
            }
        }
        row
    }).collect();
    json!(rows)
}

/// Size and date of an emulator's program. A change means it was replaced, maybe by the
/// emulator's own updater.
fn program_stamp(folder: &Path) -> String {
    let Some(m) = find_exe(folder).and_then(|p| fs::metadata(p).ok()) else { return String::new() };
    let secs = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    format!("{} {secs}", m.len())
}

/// Which of these releases is written inside the program: the emulators keep their version
/// name in it ("daily-20261107", "Fritchii-v5.5"). The longest name wins, so "v5.5" beats "v5".
fn tag_in_program(bytes: &[u8], tags: &[String]) -> Option<String> {
    let mut sorted: Vec<&String> = tags.iter().filter(|t| t.len() >= 4).collect();
    sorted.sort_by_key(|t| std::cmp::Reverse(t.len()));
    if sorted.is_empty() {
        return None;
    }
    let alts: Vec<String> = sorted.iter().map(|t| regex::escape(&t.replace('-', " ")).replace(' ', "[- ]")).collect();
    let re = regex::bytes::Regex::new(&format!("(?i)({})", alts.join("|"))).ok()?;
    let mut best: Option<String> = None;
    for m in re.find_iter(bytes) {
        // "fritchii-v5" inside "fritchii-v5.5" is not a match.
        let next = bytes.get(m.end()).copied().unwrap_or(0);
        let after = bytes.get(m.end() + 1).copied().unwrap_or(0);
        let prev = if m.start() > 0 { bytes[m.start() - 1] } else { 0 };
        if next.is_ascii_alphanumeric() || (next == b'.' && after.is_ascii_digit()) || prev.is_ascii_alphanumeric() {
            continue;
        }
        let found = String::from_utf8_lossy(m.as_bytes()).to_lowercase();
        let tag = sorted.iter().find(|t| t.to_lowercase().replace('-', " ") == found.replace('-', " ")).map(|t| t.to_string());
        if tag.as_ref().is_some_and(|t| best.as_ref().is_none_or(|b| t.len() > b.len())) {
            best = tag;
        }
    }
    best
}

/// Notices an emulator replaced outside Forkbay, and writes the version it now has.
/// Returns that version when it changed.
fn check_outside(name: &str, releases: &[Release]) -> Option<String> {
    let folder = root().join(name);
    let now = program_stamp(&folder);
    let saved = fs::read_to_string(folder.join("stamp.txt")).unwrap_or_default();
    if now.is_empty() || saved.trim() == now {
        return None;
    }
    let tags: Vec<String> = releases.iter().map(|r| r.tag.clone()).collect();
    let found = find_exe(&folder).and_then(|exe| fs::read(exe).ok()).and_then(|b| tag_in_program(&b, &tags));
    let _ = fs::write(folder.join("stamp.txt"), &now);
    let inst = installed(name);
    match found {
        // Without a stamp yet, this is only the first look at a version Forkbay installed itself.
        Some(t) if t != inst => {
            let _ = fs::write(folder.join("version.txt"), &t);
            crate::log::info(&format!("{name}: replaced outside Forkbay, {inst} -> {t}"));
            if !saved.is_empty() || !inst.is_empty() {
                let _ = fs::write(folder.join("outside.txt"), format!("{inst} -> {t}"));
            }
            (!saved.is_empty() || !inst.is_empty()).then_some(t)
        }
        _ => None,
    }
}

pub fn install(name: &str, tag: &str) -> Result<String> {
    let e = list().into_iter().find(|x| x.name == name).ok_or_else(|| anyhow!("Émulateur inconnu : {name}"))?;
    if e.source == "local" {
        bail!("{name} est une copie locale : elle se met à jour à la main.");
    }
    if any_running() {
        bail!("Ferme les émulateurs avant d'installer.");
    }
    let all = releases(&e)?;
    check_outside(&e.name, &all);
    let rel = if tag.is_empty() { all.into_iter().next().ok_or_else(|| anyhow!("Aucune version trouvée pour ce système."))? } else {
        all.into_iter().find(|r| r.tag == tag).ok_or_else(|| anyhow!("Cette version n'existe plus."))?
    };
    crate::log::info(&format!("install {name} {}", rel.tag));
    let folder = root().join(&e.name);
    let present = find_exe(&folder).is_some();
    let inst = installed(&e.name);
    if present && inst == rel.tag {
        return Ok(if tag.is_empty() { format!("{name} est déjà à jour ({inst}).") } else { format!("{name} est déjà en version {inst}.") });
    }
    if !rel.url.starts_with("https://") {
        bail!("Adresse refusée.");
    }
    if present {
        crate::backup::auto()?;
    }
    let parent = root().join("_update");
    let work = parent.join(&e.name);
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work)?;
    let file = work.join(if rel.asset.to_lowercase().ends_with(".appimage") { rel.asset.as_str() } else { "download.zip" });
    crate::net::download(&rel.url, None, &file, rel.size, &|p| crate::emit_status(&format!("{name} : téléchargement {p} %"))).map_err(|e| anyhow!("Téléchargement : {e}"))?;
    crate::emit_status(&format!("{name} : vérification et installation..."));
    if rel.size > 0 && fs::metadata(&file)?.len() != rel.size {
        bail!("Téléchargement incomplet : taille différente.");
    }
    let mut checked = "taille";
    if let Some(d) = rel.digest.strip_prefix("sha256:") {
        if !sha256_file(&file)?.eq_ignore_ascii_case(d) {
            bail!("Somme de contrôle incorrecte : fichier refusé.");
        }
        checked = "somme de contrôle SHA-256";
    }
    let (new_dir, exe_name) = if file.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")) {
        let extract = work.join("x");
        zip::ZipArchive::new(fs::File::open(&file)?)?.extract(&extract)?;
        let mut exes = vec![];
        walk_files(&extract, &mut exes);
        let tool = Regex::new(r"(?i)(cli|redist|room|tests?|setup|unins|crash)").unwrap();
        let best = exes.into_iter().filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")))
            .filter(|p| !tool.is_match(&p.file_stem().unwrap_or_default().to_string_lossy()))
            .max_by_key(|p| p.metadata().map(|m| m.len()).unwrap_or(0))
            .ok_or_else(|| anyhow!("Aucun programme d'émulateur dans l'archive."))?;
        (best.parent().unwrap().to_path_buf(), best.file_name().unwrap().to_string_lossy().into_owned())
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o755))?;
        }
        (work.clone(), rel.asset.clone())
    };
    crate::sys::virus_scan(&new_dir)?;
    let label = if inst.is_empty() { "inconnue".to_string() } else { inst };
    if folder.exists() {
        let old = retire(&folder, &e.name, &label)?;
        if let Err(err) = move_dir(&new_dir, &folder, &work) {
            fs::rename(&old, &folder)?;
            set_emulator_link(&folder, &data_path())?;
            return Err(err);
        }
    } else {
        move_dir(&new_dir, &folder, &work)?;
    }
    fs::write(folder.join("version.txt"), &rel.tag)?;
    fs::write(folder.join("exe.txt"), &exe_name)?;
    let _ = fs::write(folder.join("stamp.txt"), program_stamp(&folder));
    set_emulator_link(&folder, &data_path())?;
    let _ = fs::remove_dir_all(&work);
    let _ = fs::remove_dir(&parent);
    let pruned = prune_old(&root().join("Old Versions"), &old_versions(&e.name), KEEP_OLD);
    if pruned > 0 {
        crate::log::info(&format!("{}: {pruned} old version(s) removed, the {KEEP_OLD} newest are kept", e.name));
    }
    Ok(if present { format!("{name} : {label} -> {} (vérifié : {checked}). Ancienne version gardée dans Old Versions.", rel.tag) } else { format!("{name} {} installé (vérifié : {checked}).", rel.tag) })
}

/// Moves an emulator to Old Versions, and returns where it went.
fn retire(folder: &Path, name: &str, label: &str) -> Result<PathBuf> {
    // The link goes first: an old version must never point at the data.
    let link = folder.join("user");
    if let Ok(m) = fs::symlink_metadata(&link) {
        if !is_link(&m) {
            bail!("{} est un dossier normal alors qu'un lien est attendu : installation annulée.", link.display());
        }
        remove_link(&link)?;
    }
    let old_root = root().join("Old Versions");
    fs::create_dir_all(&old_root)?;
    let mut old = old_root.join(format!("{name} {label}"));
    let mut n = 2;
    while old.exists() {
        old = old_root.join(format!("{name} {label} ({n})"));
        n += 1;
    }
    fs::rename(folder, &old)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let _ = fs::write(old.join("retired.txt"), now.to_string());
    Ok(old)
}

/// How many replaced versions of each emulator stay in Old Versions.
const KEEP_OLD: usize = 2;

/// When a version was put aside: written at that moment, or the folder's date for older ones.
fn retired_at(dir: &Path) -> u64 {
    fs::read_to_string(dir.join("retired.txt")).ok().and_then(|s| s.trim().parse().ok())
        .or_else(|| fs::metadata(dir).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()))
        .unwrap_or(0)
}

/// Sorts versions kept in this folder, the last one put aside first.
fn newest_first(old_root: &Path, mut found: Vec<String>) -> Vec<String> {
    found.sort_by_cached_key(|v| (std::cmp::Reverse(retired_at(&old_root.join(v))), std::cmp::Reverse(v.clone())));
    found
}

/// Deletes the versions beyond the newest `keep` ones, and returns how many went.
fn prune_old(old_root: &Path, versions: &[String], keep: usize) -> usize {
    versions.iter().skip(keep).filter(|v| {
        let dir = old_root.join(v);
        // A kept version never has its own data: a real folder there means something unexpected, so it stays.
        match fs::symlink_metadata(dir.join("user")) {
            Ok(m) if !is_link(&m) => return false,
            Ok(_) => if remove_link(&dir.join("user")).is_err() { return false },
            Err(_) => {}
        }
        fs::remove_dir_all(&dir).is_ok()
    }).count()
}

/// Versions of this emulator kept in Old Versions, the last one put aside first.
pub fn old_versions(name: &str) -> Vec<String> {
    let names: Vec<String> = list().into_iter().map(|e| e.name).collect();
    let old_root = root().join("Old Versions");
    let mut found: Vec<String> = fs::read_dir(&old_root).map(|rd| rd.flatten().filter(|e| find_exe(&e.path()).is_some()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    // "Lichii Upstream 1.0" belongs to Lichii Upstream, not to Lichii.
    found.retain(|d| names.iter().filter(|n| d.starts_with(&format!("{n} "))).max_by_key(|n| n.len()).is_some_and(|n| n == name));
    newest_first(&old_root, found)
}

/// Puts a version from Old Versions back, and keeps the current one there.
pub fn rollback(name: &str, version: &str) -> Result<String> {
    if any_running() {
        bail!("Ferme les émulateurs avant de changer de version.");
    }
    if !old_versions(name).iter().any(|v| v == version) {
        bail!("Cette version n'existe plus.");
    }
    let old = root().join("Old Versions").join(version);
    if fs::symlink_metadata(old.join("user")).is_ok_and(|m| !is_link(&m)) {
        bail!("{} est un dossier normal alors qu'un lien est attendu : installation annulée.", old.join("user").display());
    }
    crate::backup::auto()?;
    let folder = root().join(name);
    let label = match installed(name).as_str() { "" => "inconnue".to_string(), v => v.to_string() };
    let parked = if folder.exists() { Some(retire(&folder, name, &label)?) } else { None };
    if let Err(err) = fs::rename(&old, &folder) {
        if let Some(p) = parked {
            fs::rename(p, &folder)?;
            set_emulator_link(&folder, &data_path())?;
        }
        return Err(err.into());
    }
    set_emulator_link(&folder, &data_path())?;
    let now = match installed(name).as_str() { "" => "inconnue".to_string(), v => v.to_string() };
    Ok(format!("{name} : version précédente rétablie ({now}). L'autre version reste dans Old Versions."))
}

fn move_dir(from: &Path, to: &Path, work: &Path) -> Result<()> {
    if from == work {
        // AppImage: only the file moves.
        fs::create_dir_all(to)?;
        for f in fs::read_dir(from)?.flatten() {
            if f.path().is_file() {
                fs::rename(f.path(), to.join(f.file_name()))?;
            }
        }
        return Ok(());
    }
    fs::rename(from, to)?;
    Ok(())
}

fn check_name(name: &str) -> Result<()> {
    let ok = Regex::new(r"^[A-Za-z0-9][A-Za-z0-9 ._-]{0,39}$").unwrap().is_match(name) && !name.ends_with('.');
    if !ok {
        bail!("Nom invalide : lettres, chiffres, espaces, points et tirets seulement.");
    }
    if list().iter().any(|e| e.name.eq_ignore_ascii_case(name)) {
        bail!("Un émulateur s'appelle déjà {name}.");
    }
    if ["Data", "Roms", "Roms Dumps", "Manager", "Save Backups", "Old Versions", "Covers", "Unverified"].iter().any(|r| r.eq_ignore_ascii_case(name)) || list().iter().any(|e| e.old_names.iter().any(|o| o.eq_ignore_ascii_case(name))) {
        bail!("Ce nom est réservé : {name}.");
    }
    Ok(())
}

/// The source is checked before it is kept.
pub fn add_github(name: &str, repo: &str, filter: &str) -> Result<String> {
    check_name(name)?;
    let repo = repo.trim().trim_start_matches("https://github.com/").trim_matches('/').to_string();
    if !Regex::new(r"^[\w.-]+/[\w.-]+$").unwrap().is_match(&repo) {
        bail!("Dépôt invalide : écris propriétaire/projet, ou colle son adresse GitHub.");
    }
    let (asset, exclude, linux) = if filter.is_empty() {
        (r"(?i)win.*\.zip$".to_string(), r"(?i)symbols|debug|source|pgo".to_string(), r"(?i)linux.*x86_64.*\.AppImage$".to_string())
    } else {
        Regex::new(filter).map_err(|_| anyhow!("Filtre invalide."))?;
        (filter.to_string(), String::new(), filter.to_string())
    };
    let e = Emu { name: name.into(), source: "github".into(), repo: repo.clone(), tag: ".".into(), asset, asset_exclude: exclude, linux_asset: linux, about: format!("Ajouté depuis {repo}"), user: true, ..Default::default() };
    let rel = latest(&e).map_err(|err| anyhow!("Aucune version utilisable sur {repo} : {err}"))?;
    let mut all = user_list();
    all.push(e);
    save_user_list(&all)?;
    Ok(format!("{name} ajouté : dernière version {}. Clique sur Installer.", rel.tag))
}

pub fn add_local(name: &str, exe: &Path) -> Result<String> {
    check_name(name)?;
    if !exe.is_file() {
        bail!("Choisis le programme de l'émulateur.");
    }
    let folder = root().join(name);
    if folder.exists() {
        bail!("Le dossier existe déjà : {}", folder.display());
    }
    let appimage = exe.extension().is_some_and(|x| x.eq_ignore_ascii_case("appimage"));
    let in_place = !appimage && installed_in_system(exe, &home(), cfg!(windows));
    if !appimage && !in_place {
        // The whole folder is copied: refuse one that is clearly not an emulator's own folder.
        let (files, size) = tree_stats(exe.parent().unwrap());
        if files > 20_000 || size > 20 * 1024 * 1024 * 1024 {
            bail!("Ce dossier est trop gros pour être copié ({files} fichiers, {} Mo) : choisis le programme d'un émulateur portable.", size / 1_048_576);
        }
    }
    if appimage {
        fs::create_dir_all(&folder)?;
        fs::copy(exe, folder.join(exe.file_name().unwrap()))?;
    } else if in_place {
        fs::create_dir_all(&folder)?;
    } else {
        copy_tree(exe.parent().unwrap(), &folder, true)?;
        let _ = fs::remove_dir_all(folder.join("user"));
    }
    // An emulator installed in the system is started where it is, by its full path.
    let target = if in_place { exe.to_string_lossy().into_owned() } else { exe.file_name().unwrap().to_string_lossy().into_owned() };
    fs::write(folder.join("exe.txt"), target.as_bytes())?;
    set_emulator_link(&folder, &data_path())?;
    let mut all = user_list();
    all.push(Emu { name: name.into(), source: "local".into(), about: if in_place { "Installé dans le système" } else { "Copie locale" }.into(), user: true, ..Default::default() });
    save_user_list(&all)?;
    Ok(if in_place { format!("{name} ajouté, utilisé sur place. Ses mises à jour passent par ton gestionnaire de paquets.") } else { format!("{name} ajouté. Ses mises à jour restent à faire à la main.") })
}

/// On Linux, a program outside the home folder (/usr/bin from a package, /opt…) belongs to the
/// system: it is used where it is. Copying its folder would copy /usr/bin.
fn installed_in_system(exe: &Path, home: &Path, windows: bool) -> bool {
    !windows && !exe.starts_with(home)
}

pub fn clear_shaders(name: &str) -> Result<String> {
    if !list().iter().any(|e| e.name == name) { bail!("Émulateur inconnu : {name}"); }
    if any_running() { bail!("Ferme l'émulateur avant de vider son cache de shaders."); }
    clear_shader_cache(&data_path(), name)?;
    Ok(format!("Cache de shaders de {name} vidé. Les shaders seront recompilés en jouant."))
}

/// Only the list entry goes: the folder stays.
pub fn remove(name: &str) -> Result<String> {
    let all: Vec<Emu> = user_list().into_iter().filter(|e| e.name != name).collect();
    save_user_list(&all)?;
    Ok(format!("{name} retiré de la liste. Son dossier est conservé."))
}

/// Starts an emulator and waits until it closes.
pub fn run(emu: &str, game: Option<&Path>, key: &str) -> Result<()> {
    let dir = root().join(emu);
    let exe = find_exe(&dir).ok_or_else(|| anyhow!("{emu} n'est pas installé."))?;
    let mut c = std::process::Command::new(&exe);
    c.current_dir(&dir);
    // These emulators leave full screen when given a game, unless they are told otherwise.
    let full = game.is_some() && load_settings().fullscreen_games && list().iter().any(|e| e.name == emu && !e.user);
    if full { c.arg("-f"); }
    if let Some(g) = game {
        c.arg("-g").arg(g);
    }
    #[cfg(not(windows))]
    linux_portable(&mut c, &dir)?;
    // Not while another emulator runs: its cache and settings would change under it.
    let idle = !any_running();
    if !idle {
        crate::log::info("shader cache unchanged: another emulator is running");
    } else {
        if let Err(e) = point_shader_cache(&data_path(), emu) {
            crate::log::error(&format!("shader cache: {e}"));
        }
        if let Err(e) = crate::config::turn_off_emulator_updates() {
            crate::log::error(&format!("emulator update prompt: {e}"));
        }
        if let Err(e) = crate::controls::follow_plugged() {
            crate::log::error(&format!("controllers: {e}"));
        }
    }
    crate::log::info(&format!("start {emu}{}", if full { " full screen" } else { "" }));
    let mut child = c.spawn()?;
    let started = std::time::Instant::now();
    let exe_name = exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let _ = fs::create_dir_all(manager_dir());
    let _ = fs::write(session_file(), json!({ "file": key, "emu": emu, "exe": exe_name, "pid": child.id() }).to_string());
    let status = child.wait();
    // Counted before the session ends, so that the list read when it ends already has it.
    if let Err(e) = crate::playtime::add(key, started.elapsed().as_secs()) {
        crate::log::error(&format!("play time: {e}"));
    }
    let _ = fs::remove_file(session_file());
    let status = status?;
    if !status.success() { crate::log::error(&format!("{emu} stopped with {status}")); } else { crate::log::info(&format!("{emu} closed")); }
    Ok(())
}

// --- The game being played ---
fn session_file() -> PathBuf {
    manager_dir().join("session.json")
}

/// What Forkbay started and is still running, on this PC.
pub fn session() -> Option<Value> {
    let v: Value = serde_json::from_str(&fs::read_to_string(session_file()).ok()?).ok()?;
    let pid = v["pid"].as_u64()?;
    if crate::sys::is_process(pid,v["exe"].as_str().unwrap_or("")) {
        Some(json!({ "file": v["file"], "emu": v["emu"] }))
    } else {
        let _ = fs::remove_file(session_file());
        None
    }
}

/// Asks the emulator to close like its own close button would. Forcing skips anything it had left to write.
pub fn close(force: bool) -> Result<String> {
    if session().is_none() { bail!("Aucun jeu en cours."); }
    let v: Value = serde_json::from_str(&fs::read_to_string(session_file())?)?;
    let pid = v["pid"].as_u64().ok_or_else(|| anyhow!("Aucun jeu en cours."))?.to_string();
    let mut c = if cfg!(windows) { command("taskkill") } else { command("kill") };
    if cfg!(windows) {
        c.args(["/PID", pid.as_str()]);
        if force { c.arg("/F"); }
    } else {
        c.args([if force { "-KILL" } else { "-TERM" }, pid.as_str()]);
    }
    if !c.output()?.status.success() { bail!("Fermeture impossible : ferme le jeu depuis sa fenêtre."); }
    crate::log::info(if force { "game closed by force" } else { "game close requested" });
    Ok(if force { "Jeu fermé.".to_string() } else { "Fermeture demandée.".to_string() })
}

// Emulators built on yuzu read the XDG folders: they are pointed at the shared data.
#[cfg(not(windows))]
fn linux_portable(c: &mut std::process::Command, dir: &Path) -> Result<()> {
    let data = data_path();
    let xdg = dir.join(".xdg");
    for (kind, target) in [("share", data.clone()), ("config", data.join("config")), ("cache", data.join("cache"))] {
        let base = xdg.join(kind);
        fs::create_dir_all(&base)?;
        for name in ["eden", "eglantine", "lichii", "yuzu", "orchid", "citron", "suyu", "sudachi"] {
            let link = base.join(name);
            let _ = fs::remove_file(&link);
            let _ = std::os::unix::fs::symlink(&target, &link);
        }
    }
    c.env("XDG_DATA_HOME", xdg.join("share")).env("XDG_CONFIG_HOME", xdg.join("config")).env("XDG_CACHE_HOME", xdg.join("cache"));
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn system_installs_are_used_in_place_on_linux_only() {
        use std::path::Path;
        let home = Path::new("/home/sam");
        assert!(super::installed_in_system(Path::new("/usr/bin/eglantine"), home, false));
        assert!(super::installed_in_system(Path::new("/opt/lichii/lichii"), home, false));
        assert!(!super::installed_in_system(Path::new("/home/sam/Emus/Eglantine/eglantine"), home, false));
        assert!(!super::installed_in_system(Path::new("C:/Emus/eglantine.exe"), Path::new("C:/Users/sam"), true));
    }

    #[test]
    fn the_version_written_in_a_program_is_found() {
        let tags: Vec<String> = ["fritchii-v5", "fritchii-v5.5", "frizti-v3", "daily-20261107"].map(String::from).to_vec();
        let find = |text: &str| super::tag_in_program(text.as_bytes(), &tags);
        assert_eq!(find("\0Fritchii-v5.5\0").as_deref(), Some("fritchii-v5.5"));
        assert_eq!(find("\0Fritchii-v5\0 about").as_deref(), Some("fritchii-v5"));
        assert_eq!(find("Frizti v3\0").as_deref(), Some("frizti-v3"));
        assert_eq!(find("build daily-20261107 ok").as_deref(), Some("daily-20261107"));
        assert_eq!(find("Fritchii-v5.6 and xdaily-20261107"), None);
    }

    #[test]
    fn only_the_newest_old_versions_are_kept() {
        let root = std::env::temp_dir().join(format!("forkbay-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (v, at) in [("Emu a", "300"), ("Emu b", "100"), ("Emu c", "200")] {
            std::fs::create_dir_all(root.join(v)).unwrap();
            std::fs::write(root.join(v).join("retired.txt"), at).unwrap();
        }
        std::fs::create_dir_all(root.join("Emu b").join("user")).unwrap();
        let order = super::newest_first(&root, vec!["Emu b".into(), "Emu a".into(), "Emu c".into()]);
        assert_eq!(order, ["Emu a", "Emu c", "Emu b"]);
        // "Emu b" is beyond the limit but holds a real data folder: it is left alone.
        assert_eq!(super::prune_old(&root, &order, 2), 0);
        std::fs::remove_dir(root.join("Emu b").join("user")).unwrap();
        assert_eq!(super::prune_old(&root, &order, 2), 1);
        assert!(!root.join("Emu b").exists() && root.join("Emu a").exists() && root.join("Emu c").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
