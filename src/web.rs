//! `ccs config edit` — short-lived 127.0.0.1 HTTP server backing a
//! drag-and-drop mode editor. Designed to be the **only** web entry
//! point in cc-status; if you're tempted to add another endpoint here
//! for an unrelated feature, write a CLI subcommand instead.
//!
//! Lifecycle:
//!   1. bind 127.0.0.1:0 (kernel picks a free port)
//!   2. mint a 32-char random token
//!   3. `open` the user's browser to http://127.0.0.1:<port>/?token=<t>
//!   4. accept GET / (HTML) and GET/POST /api/* (JSON), each request
//!      validated by token + Host-header allowlist
//!   5. exit when the user clicks Save (delayed 5s so the response can
//!      flush) OR after 30 minutes of idle OR on Ctrl-C
//!
//! No new dependencies: HTTP is parsed by hand. The parser is
//! deliberately narrow — only what this single page needs.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::{self, Config, Mode};

const HTML_BODY: &str = include_str!("../assets/editor.html");

const MAX_BODY: usize = 64 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const POST_SAVE_GRACE: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Args {
    /// Skip launching the browser; only print the URL.
    pub no_open: bool,
    /// Bind to this port instead of 0 (mostly for testing).
    pub port: Option<u16>,
}

pub fn run(args: Args) -> Result<()> {
    let bind_port = args.port.unwrap_or(0);
    let listener = TcpListener::bind(("127.0.0.1", bind_port))
        .with_context(|| format!("bind 127.0.0.1:{}", bind_port))?;
    listener.set_nonblocking(false).ok();
    let local = listener.local_addr()?;
    let port = local.port();
    let token = mint_token();
    let url = format!("http://127.0.0.1:{}/?token={}", port, token);

    eprintln!("ccs config edit — open this URL in a browser:");
    eprintln!();
    eprintln!("  {}", url);
    eprintln!();
    eprintln!("(server exits 5s after Save, or after 30 min idle)");
    let _ = std::io::Write::flush(&mut std::io::stderr());

    // Also drop the URL into a well-known cache file. Some macOS shells
    // buffer stderr aggressively when the launching process is itself
    // backgrounded; a tail-able file is the most reliable handoff.
    if let Ok(home) = std::env::var("HOME") {
        let cache = std::path::PathBuf::from(home)
            .join(".cache")
            .join("cc-status");
        if std::fs::create_dir_all(&cache).is_ok() {
            let _ = std::fs::write(cache.join("config-edit.url"), &url);
        }
    }

    if !args.no_open {
        let _ = open_in_browser(&url);
    }

    // We use a non-blocking accept loop with a deadline so we can
    // honor the idle timeout *and* the post-save grace period without
    // a separate thread.
    listener.set_nonblocking(true)?;
    let saved = Arc::new(AtomicBool::new(false));
    let save_at = Arc::new(parking_save_time());
    let mut last_activity = Instant::now();

    loop {
        if saved.load(Ordering::Acquire) {
            let elapsed = save_at.lock().unwrap().elapsed();
            if elapsed > POST_SAVE_GRACE {
                eprintln!("Save complete; exiting.");
                return Ok(());
            }
        }
        if last_activity.elapsed() > IDLE_TIMEOUT {
            eprintln!("Idle timeout reached; exiting.");
            return Ok(());
        }

        match listener.accept() {
            Ok((stream, _peer)) => {
                last_activity = Instant::now();
                // The TcpStream inherits the listener's non-blocking
                // flag on macOS/Linux. We want a *blocking* connection
                // for the per-request read so handle_client's
                // BufReader::read_line waits for data instead of
                // immediately returning EAGAIN — which would happen
                // every time a browser opens a speculative pre-connect
                // (`<link rel=preconnect>`, HTTP/1.1 connection
                // pre-warming) where the socket is established before
                // any bytes are sent.
                let _ = stream.set_nonblocking(false);
                let saved = saved.clone();
                let save_at = save_at.clone();
                let token = token.clone();
                std::thread::spawn(move || {
                    let _ = handle_client(stream, &token, &port, &saved, &save_at);
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                eprintln!("accept error: {}", e);
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// Each connection: parse one request, dispatch, close. No keep-alive.
fn handle_client(
    mut stream: TcpStream,
    token: &str,
    port: &u16,
    saved: &Arc<AtomicBool>,
    save_at: &Arc<std::sync::Mutex<Instant>>,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    let req = match parse_request(&mut stream) {
        Ok(r) => {
            // Brief access log to stderr — helps debug bizarre client
            // behaviors (preflights, broken redirects, prefetch). The
            // token is logged as `?token=...` redacted.
            let path_redacted = if let Some(i) = r.path.find("token=") {
                let (a, _) = r.path.split_at(i);
                format!("{}token=...", a)
            } else {
                r.path.clone()
            };
            eprintln!("[ccs] {} {}", r.method, path_redacted);
            let _ = std::io::Write::flush(&mut std::io::stderr());
            r
        }
        Err(e) => {
            eprintln!("[ccs] PARSE-ERR: {}", e);
            let _ = std::io::Write::flush(&mut std::io::stderr());
            return write_response(&mut stream, 400, "text/plain", e.to_string().as_bytes());
        }
    };

    // --- Host header allowlist (defense against DNS rebinding) -----
    let expected_hosts = [format!("127.0.0.1:{}", port), format!("localhost:{}", port)];
    let host = req.headers.get("host").map(String::as_str).unwrap_or("");
    if !expected_hosts.iter().any(|h| h == host) {
        return write_response(
            &mut stream,
            403,
            "text/plain",
            b"forbidden: bad Host header",
        );
    }

    // --- CORS preflight: fetch() with a custom header (X-CCS-Token)
    // triggers an OPTIONS preflight even on same-origin in some
    // browsers. Always answer it 200 with permissive same-origin
    // CORS headers — the actual security gate is the token below.
    if req.method == "OPTIONS" {
        let head = format!(
            "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: http://{host}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, X-CCS-Token\r\nAccess-Control-Max-Age: 600\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            host = host
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Write);
        return Ok(());
    }

    // --- Token check (skip only for GET /, where we need the page
    // to load before the JS can read the token from the URL) -------
    let is_html_landing = req.method == "GET" && req.path_only() == "/";
    if !is_html_landing && !token_ok(&req, token) {
        return write_response(&mut stream, 403, "text/plain", b"forbidden: bad token");
    }

    match (req.method.as_str(), req.path_only().as_str()) {
        ("GET", "/") => write_response(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            HTML_BODY.as_bytes(),
        ),
        ("GET", "/api/state") => {
            let body = build_state()?;
            write_response(&mut stream, 200, "application/json", body.as_bytes())
        }
        ("POST", "/api/save") => {
            let result = handle_save(&req.body);
            match result {
                Ok(()) => {
                    saved.store(true, Ordering::Release);
                    *save_at.lock().unwrap() = Instant::now();
                    write_response(&mut stream, 200, "application/json", b"{\"ok\":true}")
                }
                Err(e) => {
                    let body = json!({"ok": false, "error": e.to_string()}).to_string();
                    write_response(&mut stream, 400, "application/json", body.as_bytes())
                }
            }
        }
        _ => write_response(&mut stream, 404, "text/plain", b"not found"),
    }
}

// --- HTTP parsing (deliberately narrow) -----------------------------

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Request {
    fn path_only(&self) -> String {
        match self.path.find('?') {
            Some(i) => self.path[..i].to_string(),
            None => self.path.clone(),
        }
    }
    fn query(&self) -> HashMap<String, String> {
        let q = match self.path.find('?') {
            Some(i) => &self.path[i + 1..],
            None => "",
        };
        let mut out = HashMap::new();
        for pair in q.split('&').filter(|s| !s.is_empty()) {
            if let Some((k, v)) = pair.split_once('=') {
                out.insert(k.to_string(), url_decode(v));
            }
        }
        out
    }
}

fn parse_request(stream: &mut TcpStream) -> Result<Request> {
    let mut reader = BufReader::new(stream.try_clone()?);

    // Read request line
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.trim_end().splitn(3, ' ');
    let method = parts.next().context("missing method")?.to_string();
    let path = parts.next().context("missing path")?.to_string();
    // We ignore the HTTP version.

    if method != "GET" && method != "POST" && method != "OPTIONS" {
        bail!("method not supported");
    }

    // Read headers until blank line
    let mut headers = HashMap::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.trim_end().split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    // Body — only when Content-Length is set, capped at MAX_BODY.
    let len: usize = headers
        .get("content-length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if len > MAX_BODY {
        bail!("body too large: {} bytes (max {})", len, MAX_BODY);
    }
    let mut body = vec![0u8; len];
    if len > 0 {
        reader.read_exact(&mut body)?;
    }

    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

fn token_ok(req: &Request, expected: &str) -> bool {
    if let Some(v) = req.headers.get("x-ccs-token") {
        if constant_eq(v, expected) {
            return true;
        }
    }
    if let Some(v) = req.query().get("token") {
        if constant_eq(v, expected) {
            return true;
        }
    }
    false
}

/// Constant-time-ish string compare. The token is short and trusted
/// at rest; we still avoid an early-exit `==` to remove an obvious
/// timing oracle.
fn constant_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn write_response(stream: &mut TcpStream, status: u16, ct: &str, body: &[u8]) -> Result<()> {
    let phrase = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        status, phrase, ct, body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
    Ok(())
}

// --- /api/state -----------------------------------------------------

fn build_state() -> Result<String> {
    let cfg = config::load()?;
    let modes_json: serde_json::Map<String, Value> = cfg
        .modes
        .iter()
        .map(|(name, mode)| (name.clone(), json!({ "lines": mode.lines })))
        .collect();

    let segments_json: Vec<Value> = crate::segments_meta::SEGMENTS
        .iter()
        // Skip the synthetic `plugin:NAME` row from the catalog — the
        // editor pulls real plugin names from disk separately.
        .filter(|s| !s.name.starts_with("plugin:"))
        .map(|s| {
            json!({
                "name": s.name,
                "example": s.example,
                "description": s.description,
            })
        })
        .collect();

    let plugins_json: Vec<Value> = list_plugins()
        .into_iter()
        .map(|p| {
            json!({
                "name": p.name,
                "executable": p.executable,
                "example": format!("{{plugin:{}}}", p.name),
            })
        })
        .collect();

    let body = json!({
        "current_mode": cfg.current_mode,
        "modes": modes_json,
        "segments": segments_json,
        "plugins": plugins_json,
    })
    .to_string();
    Ok(body)
}

struct PluginEntry {
    name: String,
    executable: bool,
}

fn list_plugins() -> Vec<PluginEntry> {
    let dir = match plugins_dir() {
        Some(d) => d,
        None => return Vec::new(),
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<PluginEntry> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            Some(PluginEntry {
                executable: is_executable(&e.path()),
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn plugins_dir() -> Option<PathBuf> {
    let cfg = config::config_path().ok()?;
    cfg.parent().map(|p| p.join("plugins"))
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

// --- /api/save ------------------------------------------------------

fn handle_save(body: &[u8]) -> Result<()> {
    let v: Value = serde_json::from_slice(body).context("invalid JSON")?;
    let current_mode = v
        .get("current_mode")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing current_mode"))?
        .to_string();
    let modes = v
        .get("modes")
        .and_then(|x| x.as_object())
        .ok_or_else(|| anyhow::anyhow!("missing modes"))?;

    if modes.is_empty() {
        bail!("must keep at least one mode");
    }
    if !modes.contains_key(&current_mode) {
        bail!("current_mode '{}' not in modes", current_mode);
    }

    // Refuse mode names that wouldn't round-trip through the CLI.
    for name in modes.keys() {
        if name.is_empty() {
            bail!("mode name cannot be empty");
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!(
                "mode name '{}' has invalid chars (use letters, digits, '-', '_')",
                name
            );
        }
    }

    // Load current cfg so we preserve unrelated fields (theme,
    // pricing, segments). Only `current_mode` and `modes` come from
    // the editor.
    let mut cfg = config::load()?;
    let mut new_modes = std::collections::BTreeMap::new();
    for (name, mode) in modes.iter() {
        let lines: Vec<String> = mode
            .get("lines")
            .and_then(|x| x.as_array())
            .ok_or_else(|| anyhow::anyhow!("mode '{}' missing lines", name))?
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();
        new_modes.insert(name.clone(), Mode { lines });
    }
    cfg.current_mode = current_mode;
    cfg.modes = new_modes;

    save_config(&cfg)?;
    Ok(())
}

fn save_config(cfg: &Config) -> Result<()> {
    config::save(cfg)
}

// --- helpers --------------------------------------------------------

fn parking_save_time() -> std::sync::Mutex<Instant> {
    std::sync::Mutex::new(Instant::now())
}

fn open_in_browser(url: &str) -> Result<()> {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(cmd)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("failed to invoke {}", cmd))?;
    Ok(())
}

fn mint_token() -> String {
    // 24 bytes of randomness from /dev/urandom → 32 url-safe base64
    // chars. Pure stdlib path; if /dev/urandom isn't readable we
    // fall back to a process-id + nanosecond mash that's still
    // unique enough for a short-lived local server.
    //
    // CRITICAL: must use a sized read here, not `fs::read`. /dev/urandom
    // has no EOF — `fs::read` would loop forever reading and growing
    // a Vec (the symptom is a process that never finishes binding).
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let mut buf = [0u8; 24];
        if std::io::Read::read_exact(&mut f, &mut buf).is_ok() {
            return base64_url(&buf);
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let mix = nanos ^ (pid * 0x9e37_79b9_7f4a_7c15);
    let bytes = mix.to_le_bytes();
    base64_url(&bytes)
}

fn base64_url(bytes: &[u8]) -> String {
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut s = String::with_capacity((bytes.len() * 4 + 2) / 3);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        s.push(ALPHA[(b0 >> 2) & 0x3f] as char);
        s.push(ALPHA[((b0 << 4) | (b1 >> 4)) & 0x3f] as char);
        if chunk.len() > 1 {
            s.push(ALPHA[((b1 << 2) | (b2 >> 6)) & 0x3f] as char);
        }
        if chunk.len() > 2 {
            s.push(ALPHA[b2 & 0x3f] as char);
        }
    }
    s
}

fn url_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = hex_val(bytes[i + 1]);
                let lo = hex_val(bytes[i + 2]);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push(((h << 4) | l) as char);
                    i += 3;
                    continue;
                }
                out.push(bytes[i] as char);
                i += 1;
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + b - b'a'),
        b'A'..=b'F' => Some(10 + b - b'A'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_eq_short_circuits_on_length_only() {
        assert!(constant_eq("abc", "abc"));
        assert!(!constant_eq("abc", "abcd"));
        assert!(!constant_eq("abc", "abd"));
    }

    #[test]
    fn url_decode_handles_plus_and_percent() {
        assert_eq!(url_decode("hello+world"), "hello world");
        assert_eq!(url_decode("a%20b"), "a b");
        assert_eq!(url_decode("token%3Dabc"), "token=abc");
        assert_eq!(url_decode("plain"), "plain");
    }

    #[test]
    fn base64_url_roundtrip_ish() {
        // Our base64_url is one-way (we never decode); just check the
        // output is non-empty and uses the expected alphabet.
        let s = base64_url(&[0u8, 1, 2, 3, 4, 5, 6, 7]);
        assert!(!s.is_empty());
        assert!(s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn mint_token_is_nontrivial_length() {
        let t = mint_token();
        assert!(t.len() >= 16, "token too short: {}", t);
    }

    #[test]
    fn handle_save_writes_modes_into_config() {
        // Use a temp config dir.
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("cfg"));
        std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
        std::env::set_var("HOME", tmp.path());
        std::env::set_var("APPDATA", tmp.path().join("appdata"));
        // Seed a default config first.
        config::init_default(false).unwrap();

        let body = serde_json::json!({
            "current_mode": "minimal",
            "modes": {
                "minimal": { "lines": ["{dir}"] },
                "shiny":   { "lines": ["{dir} {git}", "{ctx}"] },
            }
        })
        .to_string();
        handle_save(body.as_bytes()).expect("save should succeed");

        let cfg = config::load().unwrap();
        assert_eq!(cfg.current_mode, "minimal");
        assert!(cfg.modes.contains_key("shiny"));
        assert_eq!(cfg.modes.get("shiny").unwrap().lines.len(), 2);

        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
        std::env::remove_var("HOME");
        std::env::remove_var("APPDATA");
    }

    #[test]
    fn handle_save_rejects_invalid_mode_names() {
        let body = serde_json::json!({
            "current_mode": "bad name",
            "modes": { "bad name": { "lines": ["{dir}"] } }
        })
        .to_string();
        let r = handle_save(body.as_bytes());
        assert!(r.is_err());
        let msg = format!("{}", r.unwrap_err());
        assert!(
            msg.contains("invalid"),
            "expected invalid name error, got: {}",
            msg
        );
    }

    #[test]
    fn handle_save_rejects_empty_modes() {
        let body = serde_json::json!({
            "current_mode": "x",
            "modes": {}
        })
        .to_string();
        assert!(handle_save(body.as_bytes()).is_err());
    }
}
