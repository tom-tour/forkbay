// Updates of the tool itself, from the project's GitHub releases.
use crate::core::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

pub const REPO_URL: &str = "https://github.com/tom-tour/forkbay";
pub const KOFI_URL: &str = "https://ko-fi.com/tom_t";
const API: &str = "https://api.github.com/repos/tom-tour/forkbay/releases/latest";
const ASSET: &str = if cfg!(windows) { "forkbay-windows-x86_64.exe" } else { "forkbay-linux-x86_64" };

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn parts(v: &str) -> Vec<u32> {
    v.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

fn newer(latest: &str, current: &str) -> bool {
    parts(latest) > parts(current)
}

fn release() -> Result<Option<Value>> {
    let url = std::env::var("FORKBAY_UPDATE_API").unwrap_or_else(|_| API.to_string());
    match agent().get(&url).call() {
        Ok(r) => Ok(Some(r.into_json()?)),
        // Nothing published yet, or a private repository.
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(e) => Err(anyhow!("Réseau : {e}")),
    }
}

pub fn check() -> Result<Value> {
    let current = version();
    let Some(r) = release()? else { return Ok(json!({ "current": current, "latest": "", "available": false, "published": false })) };
    let tag = r["tag_name"].as_str().unwrap_or("");
    Ok(json!({ "current": current, "latest": tag, "available": newer(tag, current), "published": true, "page": r["html_url"] }))
}

/// Old copy of the program left by the last update.
fn old_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(PathBuf::from(format!("{}.old", exe.display())))
}

pub fn cleanup() {
    if let Ok(p) = old_exe() {
        let _ = fs::remove_file(p);
    }
}

/// Downloads, verifies and swaps the program. The running copy can be renamed but not overwritten.
pub fn apply() -> Result<String> {
    let current = version();
    let r = release()?.ok_or_else(|| anyhow!("Aucune version publiée pour l'instant."))?;
    let tag = r["tag_name"].as_str().unwrap_or("").to_string();
    if !newer(&tag, current) {
        return Ok(format!("Déjà à jour ({current})."));
    }
    let asset = r["assets"].as_array().into_iter().flatten().find(|a| a["name"].as_str() == Some(ASSET)).ok_or_else(|| anyhow!("Version de l'outil introuvable pour ce système."))?;
    let url = asset["browser_download_url"].as_str().unwrap_or("");
    let size = asset["size"].as_u64().unwrap_or(0);
    let exe = std::env::current_exe()?;
    let new = PathBuf::from(format!("{}.new", exe.display()));
    let result = (|| -> Result<()> {
        let mut reader = agent().get(url).call().map_err(|e| anyhow!("Téléchargement : {e}"))?.into_reader();
        let mut file = fs::File::create(&new)?;
        let mut buf = vec![0u8; 256 * 1024];
        let (mut done, mut last) = (0u64, 0u64);
        loop {
            let n = std::io::Read::read(&mut reader, &mut buf)?;
            if n == 0 { break; }
            std::io::Write::write_all(&mut file, &buf[..n])?;
            done += n as u64;
            if size > 0 && done * 100 / size != last {
                last = done * 100 / size;
                crate::emit_status(&crate::i18n::t(&format!("Mise à jour : téléchargement {last} %")));
            }
        }
        drop(file);
        if size > 0 && fs::metadata(&new)?.len() != size {
            bail!("Téléchargement incomplet : taille différente.");
        }
        if let Some(d) = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")) {
            if !sha256_file(&new)?.eq_ignore_ascii_case(d) {
                bail!("Condensat différent : fichier refusé.");
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&new, fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&new);
        return Err(e);
    }
    let old = old_exe()?;
    let _ = fs::remove_file(&old);
    fs::rename(&exe, &old)?;
    if let Err(e) = fs::rename(&new, &exe) {
        fs::rename(&old, &exe)?;
        return Err(e.into());
    }
    Ok("Mise à jour installée : redémarrage...".into())
}
