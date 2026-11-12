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
    ureq::Agent::new_with_config(ureq::Agent::config_builder().user_agent("NX2 Launcher").timeout_connect(secs(20)).timeout_per_read(secs(120)).build())
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
    let mut r = agent().get(url).config().http_status_as_error(false).build();
    if let Some(tag) = saved.as_ref().and_then(|s| s["etag"].as_str()).filter(|t| !t.is_empty()) {
        r = r.header("If-None-Match", tag);
    }
    // GitHub says when its hourly limit ends: the message gives the wait.
    let mut wait_min = 0u64;
    let answer = match r.call() {
        Ok(resp) if resp.status().as_u16() >= 400 => {
            let h = |k: &str| resp.headers().get(k).and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u64>().ok());
            let secs = h("retry-after").or_else(|| h("x-ratelimit-reset").map(|at| at.saturating_sub(now)));
            wait_min = secs.map(|s| s.div_ceil(60).max(1)).unwrap_or(0);
            Err(HttpStatus(resp.status().as_u16()).into())
        }
        other => run(other),
    };
    match answer {
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
            None if matches!(status(&e), Some(403 | 429)) && wait_min > 0 => Err(anyhow!("GitHub limite le nombre de vérifications par heure : réessaie dans {wait_min} min.")),
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
/// A cut connection or a busy server is tried again, up to 4 times, from where the file stopped when the server allows it.
pub fn download(url: &str, auth: Option<&str>, out: &Path, size: u64, progress: &dyn Fn(u64)) -> Result<()> {
    let mut file = fs::File::create(out)?;
    let (mut done, mut last) = (0u64, 0u64);
    let mut attempt = 0;
    loop {
        attempt += 1;
        match download_part(url, auth, &mut file, &mut done, &mut last, size, progress) {
            Ok(()) => return Ok(()),
            Err(e) => {
                // A refused or missing file does not come back by asking again.
                let retry = attempt < 4 && status(&e).is_none_or(|c| c == 429 || c >= 500);
                if !retry { return Err(e); }
                crate::log::info(&format!("download: {e}, try {} from {done} bytes", attempt + 1));
                std::thread::sleep(std::time::Duration::from_secs([2, 5, 10][attempt - 1]));
            }
        }
    }
}

fn download_part(url: &str, auth: Option<&str>, file: &mut fs::File, done: &mut u64, last: &mut u64, size: u64, progress: &dyn Fn(u64)) -> Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    let mut r = agent().get(url);
    if let Some(token) = auth { r = r.header("Authorization", format!("Bearer {token}")); }
    if *done > 0 { r = r.header("Range", format!("bytes={done}-")); }
    let resp = run(r.call())?;
    // Without a partial answer, the server sends the whole file again: it starts over.
    if resp.status().as_u16() != 206 && *done > 0 {
        *done = 0;
        file.set_len(0)?;
    }
    file.seek(SeekFrom::Start(*done))?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        file.write_all(&buf[..n])?;
        *done += n as u64;
        if size > 0 && *done * 100 / size != *last {
            *last = *done * 100 / size;
            progress(*last);
        }
    }
    // A connection closed early ends the answer without an error: the size tells it.
    if size > 0 && *done < size { anyhow::bail!("connexion coupée à {} %", *done * 100 / size); }
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

    use std::io::{Read, Write};
    use std::net::TcpListener;

    type Answer = Box<dyn Fn(&str) -> Vec<u8> + Send>;

    /// A one-off local server: each closure answers one connection, given the request it received.
    fn serve(answers: Vec<Answer>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/f", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for answer in answers {
                let (mut s, _) = l.accept().unwrap();
                let mut req = vec![];
                let mut b = [0u8; 1];
                while !req.ends_with(b"\r\n\r\n") && s.read(&mut b).unwrap_or(0) == 1 { req.push(b[0]); }
                let _ = s.write_all(&answer(&String::from_utf8_lossy(&req).to_lowercase()));
            }
        });
        url
    }

    fn body() -> Vec<u8> { (0..1000u32).map(|i| (i % 251) as u8).collect() }

    #[test]
    fn download_resumes_after_a_cut() {
        let url = serve(vec![
            Box::new(|_| [b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n".as_slice(), &body()[..400]].concat()),
            Box::new(|req| {
                assert!(req.contains("range: bytes=400-"), "{req}");
                [b"HTTP/1.1 206 Partial Content\r\nContent-Length: 600\r\nContent-Range: bytes 400-999/1000\r\nConnection: close\r\n\r\n".as_slice(), &body()[400..]].concat()
            }),
        ]);
        let out = std::env::temp_dir().join(format!("nx2-dl-resume-{}", std::process::id()));
        super::download(&url, None, &out, 1000, &|_| {}).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), body());
        let _ = std::fs::remove_file(out);
    }

    #[test]
    fn download_starts_over_when_the_server_cannot_resume() {
        let url = serve(vec![
            Box::new(|_| [b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n".as_slice(), &body()[..300]].concat()),
            Box::new(|_| [b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n".as_slice(), &body()].concat()),
        ]);
        let out = std::env::temp_dir().join(format!("nx2-dl-again-{}", std::process::id()));
        super::download(&url, None, &out, 1000, &|_| {}).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), body());
        let _ = std::fs::remove_file(out);
    }

    #[test]
    fn download_does_not_ask_again_for_a_missing_file() {
        let url = serve(vec![Box::new(|_| b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec())]);
        let out = std::env::temp_dir().join(format!("nx2-dl-404-{}", std::process::id()));
        let t = std::time::Instant::now();
        assert_eq!(super::status(&super::download(&url, None, &out, 1000, &|_| {}).unwrap_err()), Some(404));
        assert!(t.elapsed().as_secs() < 2);
        let _ = std::fs::remove_file(out);
    }

    #[test]
    fn rate_limit_says_how_long_to_wait() {
        let reset = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 600;
        let url = serve(vec![Box::new(move |_| format!("HTTP/1.1 403 Forbidden\r\nx-ratelimit-reset: {reset}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").into_bytes())]);
        let err = super::get_json_cached(&url, 300).unwrap_err().to_string();
        assert!(err.contains("réessaie dans 10 min") || err.contains("réessaie dans 9 min"), "{err}");
    }
}
