// The tool's own journal. Paths, names and secrets are cleaned before anything is written.
use crate::core::*;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

const MAX_SIZE: u64 = 256 * 1024;

pub fn path() -> PathBuf {
    manager_dir().join("logs").join("nx2-launcher.log")
}

fn stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Writes without cleaning, for messages known to hold no path or name.
pub fn raw(level: &str, message: &str) {
    let file = path();
    let Some(dir) = file.parent() else { return };
    if fs::create_dir_all(dir).is_err() { return; }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = writeln!(f, "{} {level} {message}", stamp());
    }
}

pub fn write(level: &str, message: &str) {
    let file = path();
    let Some(dir) = file.parent() else { return };
    if fs::create_dir_all(dir).is_err() { return; }
    if fs::metadata(&file).map(|m| m.len() > MAX_SIZE).unwrap_or(false) {
        let _ = fs::rename(&file, file.with_extension("old.log"));
    }
    let clean = crate::report::sanitize(message).replace('\n', " | ");
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = writeln!(f, "{} {level} {clean}", stamp());
    }
}

pub fn info(m: &str) { write("INFO", m); }
pub fn error(m: &str) { write("ERROR", m); }

pub fn tail(lines: usize) -> String {
    let mut all: Vec<String> = vec![];
    for f in [path().with_extension("old.log"), path()] {
        if let Ok(t) = fs::read_to_string(f) { all.extend(t.lines().map(str::to_string)); }
    }
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// A crash still leaves a trace, even though the program stops right after.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let text = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        write("CRASH", &format!("{text} at {place}"));
    }));
}
