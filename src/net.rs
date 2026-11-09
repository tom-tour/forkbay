// Every network call goes through here: one agent, size limits, and the same errors everywhere.
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::Path;

/// A server answered with an error code. Callers can test for it with `status`.
#[derive(Debug)]
pub struct HttpStatus(pub u16);

impl std::fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "HTTP {}", self.0)
    }
}

impl std::error::Error for HttpStatus {}

/// The error code of a failed call, if the server answered.
pub fn status(e: &anyhow::Error) -> Option<u16> {
    e.downcast_ref::<HttpStatus>().map(|s| s.0)
}

type Response = ureq::http::Response<ureq::Body>;

fn agent() -> ureq::Agent {
    let secs = |s| Some(std::time::Duration::from_secs(s));
    ureq::Agent::new_with_config(ureq::Agent::config_builder().user_agent("Forkbay").timeout_connect(secs(20)).timeout_per_read(secs(120)).build())
}

fn run(r: std::result::Result<Response, ureq::Error>) -> Result<Response> {
    match r {
        Ok(r) => Ok(r),
        Err(ureq::Error::StatusCode(code)) => Err(HttpStatus(code).into()),
        Err(e) => Err(anyhow!("{e}")),
    }
}

fn get(url: &str, auth: Option<&str>) -> Result<Response> {
    let mut r = agent().get(url);
    if let Some(token) = auth { r = r.header("Authorization", format!("Bearer {token}")); }
    run(r.call())
}

/// A JSON answer. `auth` is a bearer token.
pub fn get_json(url: &str, auth: Option<&str>) -> Result<Value> {
    Ok(get(url, auth)?.body_mut().read_json()?)
}

/// A JSON answer kept on disk: reused for `fresh_secs`, then asked again with its ETag,
/// which GitHub answers with 304 without counting it in its hourly limit.
/// When the server refuses or cannot be reached, the last copy is used.
pub fn get_json_cached(url: &str, fresh_secs: u64) -> Result<Value> {
    use sha2::{Digest, Sha256};
    let dir = crate::core::manager_dir().join("cache");
    let key: String = Sha256::digest(url.as_bytes()).iter().take(12).map(|b| format!("{b:02x}")).collect();
    let file = dir.join(format!("{key}.json"));
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let saved: Option<Value> = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).filter(|v: &Value| v["url"] == url);
    if let Some(s) = &saved {
        if now.saturating_sub(s["time"].as_u64().unwrap_or(0)) < fresh_secs {
            return Ok(s["body"].clone());
        }
    }
    let save = |etag: &str, body: &Value| {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(&file, serde_json::json!({ "url": url, "time": now, "etag": etag, "body": body }).to_string());
    };
    let mut r = agent().get(url);
    if let Some(tag) = saved.as_ref().and_then(|s| s["etag"].as_str()).filter(|t| !t.is_empty()) {
        r = r.header("If-None-Match", tag);
    }
    match run(r.call()) {
        Ok(resp) if resp.status().as_u16() == 304 && saved.is_some() => {
            let s = saved.unwrap();
            save(s["etag"].as_str().unwrap_or(""), &s["body"]);
            Ok(s["body"].clone())
        }
        Ok(mut resp) => {
            let etag = resp.headers().get("etag").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            let body: Value = resp.body_mut().read_json()?;
            save(&etag, &body);
            Ok(body)
        }
        Err(e) => match saved {
            Some(s) => {
                crate::log::info(&format!("{url}: {e}, last copy used"));
                Ok(s["body"].clone())
            }
            None if matches!(status(&e), Some(403 | 429)) => Err(anyhow!("GitHub limite le nombre de vérifications par heure : réessaie un peu plus tard.")),
            None => Err(e),
        },
    }
}

/// A text answer, cut at `limit` bytes.
pub fn get_text(url: &str, limit: u64) -> Result<String> {
    let mut text = String::new();
    get(url, None)?.into_body().into_reader().take(limit).read_to_string(&mut text)?;
    Ok(text)
}

/// At most `limit` + 1 bytes, so that the caller can tell a file that is too big.
pub fn get_bytes(url: &str, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    get(url, None)?.into_body().into_reader().take(limit + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Saves an answer to a file. `progress` receives the percentage when the size is known.
pub fn download(url: &str, auth: Option<&str>, out: &Path, size: u64, progress: &dyn Fn(u64)) -> Result<()> {
    let mut reader = get(url, auth)?.into_body().into_reader();
    let mut file = fs::File::create(out)?;
    let mut buf = vec![0u8; 256 * 1024];
    let (mut done, mut last) = (0u64, 0u64);
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        std::io::Write::write_all(&mut file, &buf[..n])?;
        done += n as u64;
        if size > 0 && done * 100 / size != last {
            last = done * 100 / size;
            progress(last);
        }
    }
    Ok(())
}

/// Sends a form and reads the JSON answer.
pub fn post_form(url: &str, form: &[(&str, &str)]) -> Result<Value> {
    Ok(run(agent().post(url).send_form(form.iter().copied()))?.body_mut().read_json()?)
}

/// Sends a body with a bearer token: a new file (POST) or a new version of it (PATCH).
pub fn send(method: &str, url: &str, auth: &str, content_type: &str, body: &[u8]) -> Result<()> {
    let r = match method { "PATCH" => agent().patch(url), _ => agent().post(url) };
    run(r.header("Authorization", format!("Bearer {auth}")).header("Content-Type", content_type).send(body))?;
    Ok(())
}

/// Encodes a value for an address.
pub fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn encode_keeps_safe_characters() {
        assert_eq!(super::encode("Mario Kart-8_v1.0~"), "Mario%20Kart-8_v1.0~");
        assert_eq!(super::encode("é&="), "%C3%A9%26%3D");
    }
}
