// Game reports: the Lichii report site, read live, and the shared report site of NX2 Launcher (forkbay-compat),
// where a report is sent through a GitHub form that NX2 Launcher fills in.

use crate::net::encode;
use anyhow::Result;
use serde_json::{json, Value};

const LICHII: &str = "https://lichii.zortos.me";
pub const SITE: &str = "https://tom-tour.github.io/forkbay-compat/";
const FORM: &str = "https://github.com/tom-tour/forkbay-compat/issues/new";
/// Lichii's reports are asked again after this long.
const FRESH_SECS: u64 = 15 * 60;

fn fold(s: &str) -> String {
    s.to_lowercase().chars().map(|c| match c { 'é' | 'è' | 'ê' => 'e', c if c.is_alphanumeric() => c, _ => ' ' }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The Lichii game whose title holds ours, or that ours holds, letters only: "Fortunes Weave" is "Fortune's Weave".
fn pick_game<'a>(games: &'a [Value], name: &str) -> Option<&'a Value> {
    let ours = fold(name).replace(' ', "");
    if ours.is_empty() { return None; }
    games.iter().find(|g| {
        let theirs = fold(g["title"].as_str().unwrap_or("")).replace(' ', "");
        !theirs.is_empty() && (theirs.contains(&ours) || ours.contains(&theirs))
    })
}

/// The Lichii summary of a game and its latest reports. None when Lichii does not know the game.
fn lichii(name: &str) -> Result<Option<Value>> {
    // Lichii searches the exact text of its titles, punctuation included: the longest word of ours finds the game.
    // A word can be written another way there ("Fortune's"): the next longest one is tried.
    let folded = fold(name);
    let mut words: Vec<&str> = folded.split(' ').filter(|w| w.len() >= 3).collect();
    words.sort_by_key(|w| std::cmp::Reverse(w.len()));
    let mut hit = None;
    for word in words.into_iter().take(3) {
        let found = crate::net::get_json_cached(&format!("{LICHII}/api/games?q={}", encode(word)), FRESH_SECS)?;
        let games = found["games"].as_array().cloned().unwrap_or_default();
        if let Some(g) = pick_game(&games, name) { hit = Some((g.clone(), found["latestBuild"].clone())); break; }
    }
    let Some((g, latest)) = hit else { return Ok(None) };
    let id = g["sgdbId"].as_u64().unwrap_or(0);
    let list = if g["reportCount"].as_u64().unwrap_or(0) > 0 { crate::net::get_json_cached(&format!("{LICHII}/api/reports?gameId={id}&page=1"), FRESH_SECS)? } else { json!({}) };
    // Players' names are left out: the reports are shown for what they say about the game.
    let reports: Vec<Value> = list["reports"].as_array().cloned().unwrap_or_default().iter().map(|r| json!({
        "build": r["build"], "gpu": r["gpu"], "cpu": r["cpu"], "os": r["os"], "api": r["graphicsApi"], "outcome": r["outcome"],
        "summary": r["summary"], "settings": r["settings"], "gameVersion": r["gameVersion"], "date": r["createdAt"],
        "url": format!("{LICHII}/reports/{}", encode(r["id"].as_str().unwrap_or(""))),
    })).collect();
    Ok(Some(json!({
        "title": g["title"], "status": g["status"], "count": g["reportCount"], "best": g["best"], "latestBuild": latest,
        "url": format!("{LICHII}/games/{}", encode(g["slug"].as_str().unwrap_or(""))), "reports": reports,
    })))
}

/// The graphics card a report names: the dedicated one when there are several.
fn main_gpu(all: &str) -> String {
    let cards: Vec<String> = all.split("; ").map(|c| c.split(" (driver").next().unwrap_or("").trim().to_string()).filter(|c| !c.is_empty()).collect();
    cards.iter().find(|c| { let l = c.to_lowercase(); l.contains("nvidia") || l.contains("radeon") || l.contains(" arc") }).or(cards.first()).cloned().unwrap_or_default()
}

fn duration(seconds: u64) -> String {
    let m = seconds.div_ceil(60);
    if m < 60 { format!("{m} min") } else { format!("{} h {:02}", m / 60, m % 60) }
}

pub struct Setup<'a> {
    pub name: &'a str,
    pub id: Option<String>,
    pub emulator: &'a str,
    pub build: &'a str,
    pub played: u64,
}

/// The address of the report form, filled in with what NX2 Launcher knows. GitHub fills each field from its id.
pub fn form_url(s: &Setup, gpu: &str, cpu: &str, os: &str) -> String {
    let mut fields = vec![("template", "report.yml".to_string()), ("title", format!("[Report] {}", s.name)), ("game", s.name.to_string())];
    if let Some(id) = &s.id { fields.push(("title_id", id.clone())); }
    fields.extend([("emulator", s.emulator.to_string()), ("build", s.build.to_string()), ("gpu", main_gpu(gpu)), ("cpu", cpu.to_string()), ("os", os.to_string())]);
    if s.played > 0 { fields.push(("played", duration(s.played))); }
    fields.push(("forkbay", format!("NX2 Launcher {}", env!("CARGO_PKG_VERSION"))));
    let query: Vec<String> = fields.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}={}", encode(v))).collect();
    format!("{FORM}?{}", query.join("&"))
}

/// What the game sheet shows: Lichii's reports, and where to see or send the others.
pub fn game(s: &Setup) -> Value {
    let (os, cpu, gpu, _) = crate::sys::system_info();
    let site = match &s.id { Some(id) => format!("{SITE}#/find/{id}/{}", encode(s.name)), None => format!("{SITE}#/find/-/{}", encode(s.name)) };
    let lichii = match lichii(s.name) {
        Ok(v) => json!({ "game": v }),
        Err(e) => json!({ "error": format!("Le site de rapports de Lichii ne répond pas : {e}") }),
    };
    json!({ "lichii": lichii, "gpu": crate::compat::gpu_vendor(), "site": site, "form": form_url(s, &gpu, &cpu, &os) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_lichii_game_by_its_words() {
        let games = vec![json!({ "title": "Mario Kart World" }), json!({ "title": "The Legend of Zelda: Ocarina of Time" }), json!({ "title": "Fire Emblem: Fortune's Weave" })];
        assert_eq!(pick_game(&games, "The Legend of Zelda Ocarina of Time").unwrap()["title"], "The Legend of Zelda: Ocarina of Time");
        assert_eq!(pick_game(&games, "Mario Kart World (v1.2)").unwrap()["title"], "Mario Kart World");
        assert!(pick_game(&games, "Star Fox").is_none());
        assert_eq!(pick_game(&games, "Fire Emblem Fortunes Weave").unwrap()["title"], "Fire Emblem: Fortune's Weave");
    }

    #[test]
    fn report_form_is_filled_in() {
        let s = Setup { name: "Mario Kart World", id: Some("0400C3F00006E000".into()), emulator: "Eglantine", build: "v0.1.0-rc3", played: 3700 };
        let url = form_url(&s, "Intel(R) UHD Graphics (driver 31.0); NVIDIA GeForce RTX 4070 (driver 32.0.15.6094)", "AMD Ryzen 7 5800X3D", "Windows 11 Pro (build 26100)");
        assert!(url.starts_with("https://github.com/tom-tour/forkbay-compat/issues/new?template=report.yml&"));
        assert!(url.contains("&title_id=0400C3F00006E000&"));
        assert!(url.contains("&gpu=NVIDIA%20GeForce%20RTX%204070&"));
        assert!(url.contains("&played=1%20h%2002&"));
        assert!(url.contains("&forkbay=NX2%20Launcher%20"));
    }
}
