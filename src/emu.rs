// Emulator list, version checks, installation and launching.
use crate::core::*;
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
}

pub struct Release {
    pub tag: String,
    pub date: String,
    pub url: String,
    pub size: u64,
    pub digest: String,
    pub page: String,
    pub asset: String,
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
    if e.source == "forgejo" { format!("{}/api/v1/repos/{}/releases?limit=10", e.base, e.repo) } else { format!("https://api.github.com/repos/{}/releases?per_page=40", e.repo) }
}

fn page_url(e: &Emu) -> String {
    if e.source == "forgejo" { format!("{}/{}/releases", e.base, e.repo) } else { format!("https://github.com/{}/releases", e.repo) }
}

pub fn latest(e: &Emu) -> Result<Release> {
    let wanted = if cfg!(windows) { &e.asset } else { &e.linux_asset };
    if wanted.is_empty() {
        bail!("Pas de version pour ce système.");
    }
    let wanted = Regex::new(wanted)?;
    let exclude = if e.asset_exclude.is_empty() { None } else { Some(Regex::new(&e.asset_exclude)?) };
    let tag = Regex::new(if e.tag.is_empty() { "." } else { &e.tag })?;
    let releases: Value = agent().get(&api_url(e)).call().map_err(|err| anyhow!("Réseau : {err}"))?.into_json()?;
    let mut best: Option<Release> = None;
    for r in releases.as_array().into_iter().flatten() {
        let t = r["tag_name"].as_str().unwrap_or("");
        if r["draft"].as_bool().unwrap_or(false) || !tag.is_match(t) {
            continue;
        }
        let Some(a) = r["assets"].as_array().into_iter().flatten().find(|a| {
            let n = a["name"].as_str().unwrap_or("");
            wanted.is_match(n) && !exclude.as_ref().is_some_and(|x| x.is_match(n))
        }) else { continue };
        let date = r["published_at"].as_str().unwrap_or("").to_string();
        if best.as_ref().is_none_or(|b| date > b.date) {
            best = Some(Release {
                tag: t.into(),
                date,
                url: a["browser_download_url"].as_str().unwrap_or("").into(),
                size: a["size"].as_u64().unwrap_or(0),
                digest: a["digest"].as_str().unwrap_or("").into(),
                page: r["html_url"].as_str().unwrap_or("").into(),
                asset: a["name"].as_str().unwrap_or("").into(),
            });
        }
    }
    best.ok_or_else(|| anyhow!("Aucune version trouvée pour ce système."))
}

pub fn status() -> Value {
    let rows: Vec<Value> = list().iter().map(|e| {
        let folder = root().join(&e.name);
        let present = find_exe(&folder).is_some();
        let inst = installed(&e.name);
        let local = e.source == "local";
        let mut row = json!({ "name": e.name, "about": e.about, "present": present, "installed": inst, "latest": "", "date": "", "size": 0, "page": page_url(e), "upToDate": false, "error": "", "user": e.user, "local": local });
        if !local {
            match latest(e) {
                Ok(l) => {
                    row["latest"] = json!(l.tag);
                    row["date"] = json!(l.date.get(..10).unwrap_or(""));
                    row["size"] = json!(l.size);
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

fn download(url: &str, out: &Path, size: u64, label: &str) -> Result<()> {
    let mut reader = agent().get(url).call().map_err(|e| anyhow!("Téléchargement : {e}"))?.into_reader();
    let mut file = fs::File::create(out)?;
    let mut buf = vec![0u8; 256 * 1024];
    let (mut done, mut last) = (0u64, 0u64);
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf)?;
        if n == 0 { break; }
        std::io::Write::write_all(&mut file, &buf[..n])?;
        done += n as u64;
        if size > 0 && done * 100 / size != last {
            last = done * 100 / size;
            crate::emit_status(&format!("{label} : téléchargement {last} %"));
        }
    }
    Ok(())
}

pub fn install(name: &str) -> Result<String> {
    let e = list().into_iter().find(|x| x.name == name).ok_or_else(|| anyhow!("Émulateur inconnu : {name}"))?;
    if e.source == "local" {
        bail!("{name} est une copie locale : elle se met à jour à la main.");
    }
    if any_running() {
        bail!("Ferme les émulateurs avant d'installer.");
    }
    let rel = latest(&e)?;
    crate::log::info(&format!("install {name} {}", rel.tag));
    let folder = root().join(&e.name);
    let present = find_exe(&folder).is_some();
    let inst = installed(&e.name);
    if present && inst == rel.tag {
        return Ok(format!("{name} est déjà à jour ({inst})."));
    }
    let parent = root().join("_update");
    let work = parent.join(&e.name);
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work)?;
    let file = work.join(if rel.asset.to_lowercase().ends_with(".appimage") { rel.asset.as_str() } else { "download.zip" });
    download(&rel.url, &file, rel.size, name)?;
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
    #[cfg(windows)]
    {
        let defender = PathBuf::from(std::env::var_os("ProgramFiles").unwrap_or_default()).join("Windows Defender").join("MpCmdRun.exe");
        if defender.is_file() {
            let st = command(&defender.to_string_lossy()).args(["-Scan", "-ScanType", "3", "-DisableRemediation", "-File"]).arg(&new_dir).output()?;
            if st.status.code() == Some(2) {
                bail!("Defender signale une menace : installation annulée.");
            }
        }
    }
    let label = if inst.is_empty() { "inconnue".to_string() } else { inst };
    if folder.exists() {
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
        let mut old = old_root.join(format!("{} {label}", e.name));
        let mut n = 2;
        while old.exists() {
            old = old_root.join(format!("{} {label} ({n})", e.name));
            n += 1;
        }
        fs::rename(&folder, &old)?;
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
    set_emulator_link(&folder, &data_path())?;
    let _ = fs::remove_dir_all(&work);
    let _ = fs::remove_dir(&parent);
    Ok(if present { format!("{name} : {label} -> {} (vérifié : {checked}). Ancienne version gardée dans Old Versions.", rel.tag) } else { format!("{name} {} installé (vérifié : {checked}).", rel.tag) })
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
    if ["Data", "Roms", "Roms Dumps", "Manager", "Save Backups", "Old Versions", "Covers", "Unverified"].iter().any(|r| r.eq_ignore_ascii_case(name)) {
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
    if exe.extension().is_some_and(|x| x.eq_ignore_ascii_case("appimage")) {
        fs::create_dir_all(&folder)?;
        fs::copy(exe, folder.join(exe.file_name().unwrap()))?;
    } else {
        copy_tree(exe.parent().unwrap(), &folder, true)?;
        let _ = fs::remove_dir_all(folder.join("user"));
    }
    fs::write(folder.join("exe.txt"), exe.file_name().unwrap().to_string_lossy().as_bytes())?;
    set_emulator_link(&folder, &data_path())?;
    let mut all = user_list();
    all.push(Emu { name: name.into(), source: "local".into(), about: "Copie locale".into(), user: true, ..Default::default() });
    save_user_list(&all)?;
    Ok(format!("{name} ajouté. Ses mises à jour restent à faire à la main."))
}

/// Only the list entry goes: the folder stays.
pub fn remove(name: &str) -> Result<String> {
    let all: Vec<Emu> = user_list().into_iter().filter(|e| e.name != name).collect();
    save_user_list(&all)?;
    Ok(format!("{name} retiré de la liste. Son dossier est conservé."))
}

/// Starts an emulator and waits until it closes.
pub fn run(emu: &str, game: Option<&Path>) -> Result<()> {
    let dir = root().join(emu);
    let exe = find_exe(&dir).ok_or_else(|| anyhow!("{emu} n'est pas installé."))?;
    let mut c = std::process::Command::new(&exe);
    c.current_dir(&dir);
    if let Some(g) = game {
        c.arg("-g").arg(g);
    }
    #[cfg(not(windows))]
    linux_portable(&mut c, &dir)?;
    crate::log::info(&format!("start {emu}"));
    let status = c.spawn()?.wait()?;
    if !status.success() { crate::log::error(&format!("{emu} stopped with {status}")); } else { crate::log::info(&format!("{emu} closed")); }
    Ok(())
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
