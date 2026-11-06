// Sync of saves, profile and settings. Keys and firmware are never sent.
use crate::core::*;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const SYNC_FILE: &str = "forkbay-sync.zip";
const GOOGLE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

#[derive(Clone)]
pub struct Meta { pub hash: String, pub saved_at: String, pub machine: String, pub id: String, pub token: String }

/// Files synced, relative to Data, with forward slashes.
pub fn sync_files(data: &Path) -> Vec<String> {
    let mut abs = vec![];
    for d in ["nand/user/save", "nand/system/save/8000000000000010"] { walk_files(&data.join(d), &mut abs); }
    for f in ["config/qt-config.ini", "config/sdl2-config.ini"] { let p = data.join(f); if p.is_file() { abs.push(p); } }
    if let Ok(rd) = fs::read_dir(data.join("config/custom")) {
        abs.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ini")));
    }
    let mut rel: Vec<String> = abs.iter().filter_map(|p| p.strip_prefix(data).ok()).map(slash).collect();
    rel.sort();
    rel
}

// Removes what depends on the PC or changes on every launch.
fn normalized_config(path: &Path) -> Result<String> {
    let t = fs::read_to_string(path)?;
    let t = Regex::new(r"(?m)^(nand|sdmc|load|dump|tas)_directory=[^\r\n]*").unwrap().replace_all(&t, "${1}_directory=__LOCAL__").into_owned();
    let t = Regex::new(r"(?m)^(Screenshots\\screenshot_path)=[^\r\n]*").unwrap().replace_all(&t, "${1}=__LOCAL__").into_owned();
    let t = Regex::new(r"(?m)^(Paths\\gamedirs\\\d+\\path)=([^\r\n]*)").unwrap().replace_all(&t, |c: &regex::Captures| if c[2].contains(":/") || c[2].starts_with('/') { format!("{}=__LOCAL__", &c[1]) } else { c[0].to_string() }).into_owned();
    Ok(Regex::new(r"(?m)^(Paths\\recentFiles)=[^\r\n]*").unwrap().replace_all(&t, "${1}=").into_owned())
}

pub fn local_hash(data: &Path) -> Result<String> {
    let mut parts = vec![];
    for rel in sync_files(data) {
        let full = data.join(&rel);
        let h = if rel == "config/qt-config.ini" { sha256_hex(normalized_config(&full)?.as_bytes()) } else { sha256_file(&full)? };
        parts.push(format!("{}|{h}", rel.to_lowercase()));
    }
    parts.sort();
    Ok(sha256_hex(parts.join("\n").as_bytes()))
}

fn local_date(data: &Path) -> u64 {
    let mut files = vec![];
    walk_files(&data.join("nand/user/save"), &mut files);
    files.iter().filter_map(|f| f.metadata().ok()?.modified().ok()).filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).max().unwrap_or(0)
}

fn state_get(data: &Path) -> String {
    fs::read_to_string(data.join("sync-state.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v["lastHash"].as_str().map(str::to_string)).unwrap_or_default()
}

fn state_set(data: &Path, hash: &str) -> Result<()> {
    fs::write(data.join("sync-state.json"), json!({ "lastHash": hash }).to_string())?;
    Ok(())
}

fn work_dir() -> Result<PathBuf> {
    let w = manager_dir().join("_work");
    let _ = fs::remove_dir_all(&w);
    fs::create_dir_all(&w)?;
    Ok(w)
}

fn make_zip(data: &Path, out: &Path) -> Result<()> {
    let mut z = zip::ZipWriter::new(fs::File::create(out)?);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for rel in sync_files(data) {
        z.start_file(rel.as_str(), opts)?;
        if rel == "config/qt-config.ini" { z.write_all(normalized_config(&data.join(&rel))?.as_bytes())?; } else { std::io::copy(&mut fs::File::open(data.join(&rel))?, &mut z)?; }
    }
    z.finish()?;
    Ok(())
}

pub fn copy_synced(data: &Path, dest: &Path) -> Result<()> {
    for rel in sync_files(data) {
        let to = dest.join(&rel);
        fs::create_dir_all(to.parent().unwrap())?;
        fs::copy(data.join(&rel), to)?;
    }
    fs::create_dir_all(dest)?;
    Ok(())
}

fn backup_local(data: &Path, keep: u32) -> Result<PathBuf> {
    let root_dir = root().join("Save Backups");
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
    let dest = root_dir.join(format!("auto-{stamp}"));
    copy_synced(data, &dest)?;
    let mut old: Vec<PathBuf> = fs::read_dir(&root_dir)?.flatten().map(|e| e.path()).filter(|p| p.is_dir() && p.file_name().unwrap().to_string_lossy().starts_with("auto-")).collect();
    old.sort();
    let keep = keep.max(1) as usize;
    if old.len() > keep { for p in &old[..old.len() - keep] { let _ = fs::remove_dir_all(p); } }
    Ok(dest)
}

pub fn clear_synced(data: &Path) -> Result<()> {
    for rel in sync_files(data) { fs::remove_file(data.join(rel))?; }
    let _ = fs::remove_dir_all(data.join("nand/user/save"));
    fs::create_dir_all(data.join("nand/user/save"))?;
    Ok(())
}

/// Replaces the local version: backup first, roll back if the result does not match.
fn install_zip(zip_path: &Path, data: &Path, expected: &str, keep: u32) -> Result<()> {
    let backup = backup_local(data, keep)?;
    let attempt = (|| -> Result<()> {
        clear_synced(data)?;
        zip::ZipArchive::new(fs::File::open(zip_path)?)?.extract(data)?;
        set_config_paths(data)?;
        if !expected.is_empty() && local_hash(data)? != expected { bail!("contenu différent de la version du cloud"); }
        Ok(())
    })();
    if let Err(e) = attempt {
        let _ = clear_synced(data);
        copy_tree(&backup, data, true)?;
        set_config_paths(data)?;
        bail!("Remplacement annulé, version locale restaurée : {e}");
    }
    Ok(())
}

// --- Synced folder ---
fn folder_dir(s: &Settings) -> Result<PathBuf> {
    if s.sync.folder_path.is_empty() { bail!("Aucun dossier de synchronisation choisi."); }
    let base = PathBuf::from(&s.sync.folder_path);
    if !base.is_dir() { bail!("Dossier de synchronisation introuvable : {}", base.display()); }
    Ok(base.join("ForkbaySync"))
}

// --- Google Drive API ---
fn google_token(s: &Settings) -> Result<String> {
    let refresh = unprotect(&s.sync.google_refresh_token);
    if refresh.is_empty() { bail!("Compte Google non connecté sur ce PC : clique sur Connecter mon compte Google dans la page Cloud."); }
    let v: Value = agent().post("https://oauth2.googleapis.com/token").send_form(&[("client_id", &s.sync.google_client_id), ("client_secret", &unprotect(&s.sync.google_client_secret)), ("refresh_token", &refresh), ("grant_type", "refresh_token")])?.into_json()?;
    v["access_token"].as_str().map(str::to_string).ok_or_else(|| anyhow!("Google n'a pas renvoyé de jeton."))
}

fn google_find(token: &str) -> Result<Option<Value>> {
    let v: Value = agent().get("https://www.googleapis.com/drive/v3/files").query("q", &format!("name='{SYNC_FILE}' and trashed=false")).query("fields", "files(id,appProperties,modifiedTime)").query("spaces", "drive").set("Authorization", &format!("Bearer {token}")).call()?.into_json()?;
    Ok(v["files"].as_array().and_then(|a| a.first().cloned()))
}

pub fn google_connect() -> Result<String> {
    let mut s = load_settings();
    let secret = unprotect(&s.sync.google_client_secret);
    if s.sync.google_client_id.is_empty() || secret.is_empty() { bail!("Renseigne d'abord l'identifiant et le secret du client OAuth."); }
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| anyhow!("{e}"))?;
    let nonce = sha256_hex(&seed);
    let verifier = nonce.clone() + &nonce[..20];
    use sha2::{Digest, Sha256};
    let challenge = b64.encode(Sha256::digest(verifier.as_bytes()));
    let state = &nonce[..24];
    let server = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = server.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}/");
    let url = format!("https://accounts.google.com/o/oauth2/v2/auth?response_type=code&access_type=offline&prompt=consent&client_id={}&redirect_uri={}&scope={}&state={state}&code_challenge={challenge}&code_challenge_method=S256", encode(&s.sync.google_client_id), encode(&redirect), encode(GOOGLE_SCOPE));
    open_external(&url);
    server.set_nonblocking(true)?;
    let start = std::time::Instant::now();
    let mut stream = loop {
        match server.accept() {
            Ok((st, _)) => break st,
            Err(_) if start.elapsed().as_secs() < 300 => std::thread::sleep(std::time::Duration::from_millis(200)),
            Err(_) => bail!("Connexion Google non terminée après 5 minutes."),
        }
    };
    stream.set_nonblocking(false)?;
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf)?;
    let head = String::from_utf8_lossy(&buf[..n]).into_owned();
    let line = head.lines().next().unwrap_or("");
    let param = |k: &str| Regex::new(&format!(r"[?&]{k}=([^& ]+)")).unwrap().captures(line).map(|c| decode(&c[1]));
    let page = "<html><body style='font-family:sans-serif;background:#12141c;color:#eee;padding:40px'><h2>Forkbay</h2><p>Tu peux fermer cet onglet et revenir à l'outil.</p></body></html>";
    let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
    if param("state").as_deref() != Some(state) { bail!("Réponse Google non reconnue : connexion refusée."); }
    let Some(code) = param("code") else { bail!("Connexion refusée par Google."); };
    let v: Value = agent().post("https://oauth2.googleapis.com/token").send_form(&[("client_id", &s.sync.google_client_id), ("client_secret", &secret), ("code", &code), ("code_verifier", &verifier), ("grant_type", "authorization_code"), ("redirect_uri", &redirect)])?.into_json()?;
    let refresh = v["refresh_token"].as_str().ok_or_else(|| anyhow!("Google n'a pas renvoyé de jeton durable."))?;
    s.sync.google_refresh_token = protect(refresh);
    save_settings(&s)?;
    Ok("Compte Google connecté.".into())
}

fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) { out.push(v); i += 3; continue; }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// --- Common operations ---
fn remote_meta(s: &Settings) -> Result<Option<Meta>> {
    if s.sync.provider == "google" {
        let token = google_token(s)?;
        let Some(f) = google_find(&token)? else { return Ok(None) };
        let g = |k: &str| f["appProperties"][k].as_str().unwrap_or("").to_string();
        return Ok(Some(Meta { hash: g("hash"), saved_at: g("savedAt"), machine: g("machine"), id: f["id"].as_str().unwrap_or("").into(), token }));
    }
    let meta = folder_dir(s)?.join("meta.json");
    let Ok(t) = fs::read_to_string(meta) else { return Ok(None) };
    let v: Value = serde_json::from_str(&t)?;
    let g = |k: &str| v[k].as_str().unwrap_or("").to_string();
    Ok(Some(Meta { hash: g("hash"), saved_at: g("savedAt"), machine: g("machine"), id: String::new(), token: String::new() }))
}

fn machine() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "PC".into())
}

fn send_remote(s: &Settings, zip_path: &Path, hash: &str, date: u64) -> Result<()> {
    let saved = date.to_string();
    if s.sync.provider == "google" {
        let token = google_token(s)?;
        let existing = google_find(&token)?;
        let mut meta = json!({ "appProperties": { "hash": hash, "savedAt": saved, "machine": machine() } });
        if existing.is_none() { meta["name"] = json!(SYNC_FILE); }
        let boundary = format!("forkbay{}", &sha256_hex(hash.as_bytes())[..24]);
        let mut body = format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/zip\r\n\r\n").into_bytes();
        body.extend(fs::read(zip_path)?);
        body.extend(format!("\r\n--{boundary}--\r\n").into_bytes());
        let ct = format!("multipart/related; boundary={boundary}");
        let auth = format!("Bearer {token}");
        match existing {
            Some(f) => agent().request("PATCH", &format!("https://www.googleapis.com/upload/drive/v3/files/{}?uploadType=multipart", f["id"].as_str().unwrap_or(""))).set("Authorization", &auth).set("Content-Type", &ct).send_bytes(&body)?,
            None => agent().post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart").set("Authorization", &auth).set("Content-Type", &ct).send_bytes(&body)?,
        };
        return Ok(());
    }
    let dir = folder_dir(s)?;
    fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("{SYNC_FILE}.tmp"));
    fs::copy(zip_path, &tmp)?;
    fs::rename(&tmp, dir.join(SYNC_FILE))?;
    fs::write(dir.join("meta.json"), json!({ "hash": hash, "savedAt": saved, "machine": machine() }).to_string())?;
    Ok(())
}

fn receive_remote(s: &Settings, meta: &Meta, out: &Path) -> Result<()> {
    if s.sync.provider == "google" {
        let mut r = agent().get(&format!("https://www.googleapis.com/drive/v3/files/{}?alt=media", meta.id)).set("Authorization", &format!("Bearer {}", meta.token)).call()?.into_reader();
        std::io::copy(&mut r, &mut fs::File::create(out)?)?;
        return Ok(());
    }
    fs::copy(folder_dir(s)?.join(SYNC_FILE), out).context("Fichier introuvable dans le cloud")?;
    Ok(())
}

fn upload(s: &Settings, data: &Path, hash: &str) -> Result<()> {
    let w = work_dir()?;
    let z = w.join(SYNC_FILE);
    let r = make_zip(data, &z).and_then(|_| send_remote(s, &z, hash, local_date(data))).and_then(|_| state_set(data, hash));
    let _ = fs::remove_dir_all(&w);
    r
}

fn download(s: &Settings, data: &Path, meta: &Meta) -> Result<()> {
    let w = work_dir()?;
    let z = w.join("remote.zip");
    let r = receive_remote(s, meta, &z).and_then(|_| install_zip(&z, data, &meta.hash, s.sync.keep_backups)).and_then(|_| state_set(data, &meta.hash));
    let _ = fs::remove_dir_all(&w);
    r
}

pub enum Choice { Local, Remote, Cancel }

/// Compares local, remote and the state known at the last exchange.
pub fn sync(s: &Settings, resolve: &dyn Fn(u64, u64, &str) -> Choice) -> Result<&'static str> {
    if crate::emu::any_running() { bail!("Un émulateur est ouvert : synchronisation impossible pour l'instant."); }
    let data = data_path();
    let meta = remote_meta(s)?;
    let local = local_hash(&data)?;
    let base = state_get(&data);
    let Some(meta) = meta else { upload(s, &data, &local)?; return Ok("uploaded") };
    if meta.hash == local { if base != local { state_set(&data, &local)?; } return Ok("none"); }
    if base == local { download(s, &data, &meta)?; return Ok("downloaded"); }
    if base == meta.hash { upload(s, &data, &local)?; return Ok("uploaded"); }
    match resolve(local_date(&data), meta.saved_at.parse().unwrap_or(0), &meta.machine) {
        Choice::Local => { upload(s, &data, &local)?; Ok("uploaded") }
        Choice::Remote => { download(s, &data, &meta)?; Ok("downloaded") }
        Choice::Cancel => Ok("cancelled"),
    }
}

pub fn format_date(secs: u64) -> String {
    let en = !crate::i18n::is_fr();
    if secs == 0 { return crate::i18n::t("aucune sauvegarde"); }
    // Civil date from days since 1970 (UTC offset from the system when available).
    let offset = local_offset();
    let t = secs as i64 + offset;
    let (days, rem) = (t.div_euclid(86400), t.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    if en { return format!("{y}-{m:02}-{d:02} at {:02}:{:02}", rem / 3600, rem % 3600 / 60); }
    format!("{d:02}/{m:02}/{y} à {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}

#[cfg(windows)]
fn local_offset() -> i64 {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    let mut z: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    let bias = match unsafe { GetTimeZoneInformation(&mut z) } {
        1 => z.Bias + z.StandardBias,
        2 => z.Bias + z.DaylightBias,
        _ => z.Bias,
    };
    -(bias as i64) * 60
}

#[cfg(not(windows))]
fn local_offset() -> i64 {
    let out = command("date").arg("+%z").output();
    out.ok().and_then(|o| {
        let t = String::from_utf8_lossy(&o.stdout).trim().to_string();
        let sign = if t.starts_with('-') { -1 } else { 1 };
        let h: i64 = t.get(1..3)?.parse().ok()?;
        let m: i64 = t.get(3..5)?.parse().ok()?;
        Some(sign * (h * 3600 + m * 60))
    }).unwrap_or(0)
}

/// Conflict question: both dates, and the user chooses.
pub fn ask_conflict(local: u64, remote: u64, machine: &str, can_cancel: bool) -> Choice {
    // Scripted answer for the automated tests.
    match std::env::var("FORKBAY_CONFLICT").as_deref() {
        Ok("local") => return Choice::Local,
        Ok("remote") => return Choice::Remote,
        Ok("cancel") => return Choice::Cancel,
        _ => {}
    }
    use crate::i18n::t;
    let (a, b, c) = (t("Garder la version locale"), t("Garder la version du cloud"), t("Annuler"));
    let text = t(&format!("Les deux versions ont changé depuis la dernière synchronisation.\n\nVersion locale (ce PC) : {}\nVersion du cloud ({machine}) : {}\n\nCelle que tu ne gardes pas est remplacée. Si c'est la version locale, une copie reste dans Save Backups.", format_date(local), format_date(remote)));
    let buttons = if can_cancel { rfd::MessageButtons::YesNoCancelCustom(a.clone(), b.clone(), c) } else { rfd::MessageButtons::OkCancelCustom(a.clone(), b.clone()) };
    let r = rfd::MessageDialog::new().set_title(t("Forkbay : conflit de sauvegarde")).set_description(text).set_level(rfd::MessageLevel::Warning).set_buttons(buttons).show();
    match r {
        rfd::MessageDialogResult::Custom(t) if t == a => Choice::Local,
        rfd::MessageDialogResult::Custom(t) if t == b => Choice::Remote,
        _ => Choice::Cancel,
    }
}
