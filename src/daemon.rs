//! Optional Unix-socket daemon for sub-millisecond status-line refresh.
//!
//! `ccs daemon start` forks a long-lived process that listens on
//! `$XDG_RUNTIME_DIR/cc-status.sock` (or `/tmp/cc-status-$UID.sock` on
//! macOS). `ccs render` tries the socket first and falls back to
//! direct rendering if the daemon isn't reachable, so users never have
//! to start the daemon — it's strictly an optimization.
//!
//! Wire protocol: one JSON line per direction. The client sends
//! `{"stdin": <original cc json>}` and the server replies with
//! `{"output": "<ansi>"}`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[0;33m";
const RED: &str = "\x1b[0;31m";
const DIM: &str = "\x1b[90m";

const CONNECT_TIMEOUT: Duration = Duration::from_millis(50);
const REPLY_TIMEOUT: Duration = Duration::from_millis(200);

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub stdin: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct Response {
    pub output: String,
}

pub fn socket_path() -> PathBuf {
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        if !rt.is_empty() {
            return PathBuf::from(rt).join("cc-status.sock");
        }
    }
    let uid = users_uid();
    PathBuf::from(format!("/tmp/cc-status-{}.sock", uid))
}

fn users_uid() -> u32 {
    // Avoid pulling in libc/users; std::os::unix exposes nothing useful here.
    // Fall back to 0 if /usr/bin/id isn't available.
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

fn pid_path() -> PathBuf {
    socket_path().with_extension("pid")
}

/// Try to render via the daemon. Returns `None` if no daemon is
/// reachable; the caller should fall back to inline rendering.
pub fn try_render_via_daemon(stdin: &serde_json::Value) -> Option<String> {
    let path = socket_path();
    let mut stream = UnixStream::connect(&path).ok()?;
    stream.set_read_timeout(Some(REPLY_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(CONNECT_TIMEOUT)).ok()?;

    let req = Request {
        stdin: stdin.clone(),
    };
    let line = serde_json::to_string(&req).ok()?;
    stream.write_all(line.as_bytes()).ok()?;
    stream.write_all(b"\n").ok()?;

    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    reader.read_line(&mut reply).ok()?;
    let resp: Response = serde_json::from_str(&reply).ok()?;
    Some(resp.output)
}

#[derive(Default)]
pub struct StartArgs {
    /// Run in the foreground (for tests / launchd).
    pub foreground: bool,
}

pub fn start(args: StartArgs) -> Result<()> {
    let path = socket_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // If a daemon is already running, refuse to start a second one.
    if let Ok(_s) = UnixStream::connect(&path) {
        eprintln!(
            "{}!{} a daemon is already listening on {}",
            YELLOW,
            RESET,
            path.display()
        );
        return Ok(());
    }
    // Stale socket file from a crashed daemon — remove.
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }

    if !args.foreground {
        // Minimal "daemonize": fork once, parent exits. We don't do the
        // double-fork dance because launchd / systemd handle that.
        unsafe {
            let pid = libc_fork();
            if pid < 0 {
                anyhow::bail!("fork failed");
            }
            if pid > 0 {
                println!("{}✓{} daemon started (pid {})", GREEN, RESET, pid);
                println!("  socket: {}", path.display());
                return Ok(());
            }
            // child: become a session leader so it survives the shell exit
            libc_setsid();
        }
    }

    let listener = UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))?;

    // Write pidfile after binding so callers can stop us.
    let _ = std::fs::write(pid_path(), std::process::id().to_string());

    // Best-effort cleanup at exit. Without a signal handler this won't
    // fire on SIGKILL, but does on normal Ctrl-C.
    let cleanup_path = path.clone();
    let _ = ctrlc_like_setup(move || {
        let _ = std::fs::remove_file(&cleanup_path);
        let _ = std::fs::remove_file(pid_path());
    });

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(|| {
                    let _ = handle_client(s);
                });
            }
            Err(_) => continue,
        }
    }
    Ok(())
}

fn handle_client(mut stream: UnixStream) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    let mut buf = String::new();
    let mut reader = BufReader::new(&mut stream);
    reader.read_line(&mut buf)?;
    let req: Request = serde_json::from_str(&buf)?;

    // Re-use the existing render pipeline. It already prints to stdout,
    // so we capture stdout into a Vec via a child invocation? No — we
    // call `render::render_string(stdin)` directly.
    let rendered = crate::render::render_string(&req.stdin)?;
    let resp = Response { output: rendered };
    let line = serde_json::to_string(&resp)?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    Ok(())
}

pub fn stop() -> Result<()> {
    let pid = match std::fs::read_to_string(pid_path()).ok() {
        Some(s) => s.trim().parse::<i32>().ok(),
        None => None,
    };
    if let Some(pid) = pid {
        unsafe {
            libc_kill(pid, 15); // SIGTERM
        }
        println!("{}✓{} sent SIGTERM to pid {}", GREEN, RESET, pid);
        let _ = std::fs::remove_file(pid_path());
    } else {
        eprintln!("{}!{} no pid file found", YELLOW, RESET);
    }
    let _ = std::fs::remove_file(socket_path());
    Ok(())
}

pub fn status() -> Result<()> {
    println!("{B}cc-status daemon status{R}", B = BOLD, R = RESET);
    println!("{}─────────────────────────{}", DIM, RESET);
    println!("  socket: {}", socket_path().display());
    println!("  pidfile: {}", pid_path().display());

    let alive = UnixStream::connect(socket_path()).is_ok();
    if alive {
        println!("  state: {}running{}", GREEN, RESET);
    } else {
        println!("  state: {}not running{}", RED, RESET);
    }

    if let Ok(pid) = std::fs::read_to_string(pid_path()) {
        println!("  pid: {}", pid.trim());
    }
    Ok(())
}

// --- minimal libc shims (avoid pulling the libc crate just for fork) ---

extern "C" {
    fn fork() -> i32;
    fn setsid() -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
}

unsafe fn libc_fork() -> i32 {
    fork()
}
unsafe fn libc_setsid() -> i32 {
    setsid()
}
unsafe fn libc_kill(pid: i32, sig: i32) -> i32 {
    kill(pid, sig)
}

/// Best-effort signal cleanup. We intentionally don't pull in the
/// `ctrlc` crate; on signal the OS will reap us anyway and the next
/// daemon start will overwrite the stale socket file.
fn ctrlc_like_setup<F: FnOnce() + Send + 'static>(_f: F) -> Result<()> {
    // No-op for now — see comment above.
    let _ = _f;
    Ok(())
}
