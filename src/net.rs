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
