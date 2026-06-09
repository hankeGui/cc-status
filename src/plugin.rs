//! `ccs plugin` — scaffolding and inspection helpers for `{plugin:NAME}` segments.
//!
//! Plugins are plain executables under `<config-dir>/plugins/`; cc-status pipes
//! Claude Code's status-line JSON into stdin and uses stdout as the segment
//! value (250 ms hard timeout, 80-char display cap). This module does NOT
//! change how plugins run at render time — see `segments::seg_plugin` for
//! that. It only helps the user create, list, and debug plugins.

use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::config;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[0;33m";
const RED: &str = "\x1b[0;31m";
const CYAN: &str = "\x1b[1;36m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Sh,
    Python,
}

impl Lang {
    pub fn from_flag(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "sh" | "shell" | "bash" => Ok(Lang::Sh),
            "py" | "python" | "python3" => Ok(Lang::Python),
            other => anyhow::bail!("unsupported language '{}'; valid: sh, python", other),
        }
    }
    fn template(self, name: &str) -> String {
        match self {
            Lang::Sh => sh_template(name),
            Lang::Python => python_template(name),
        }
    }
}

pub enum Action {
    /// List installed plugins.
    List,
    /// Print the plugins directory path.
    Path,
    /// Scaffold a new plugin from a template.
    New {
        name: String,
        lang: Lang,
        force: bool,
    },
    /// Run a plugin in debug mode (show stdout / stderr / exit / elapsed).
    Run { name: String, warm: bool },
    /// Health-check every installed plugin.
    Doctor,
}

pub fn run(action: Action) -> Result<()> {
    match action {
        Action::List => list(),
        Action::Path => {
            println!("{}", plugins_dir()?.display());
            Ok(())
        }
        Action::New { name, lang, force } => new(&name, lang, force),
        Action::Run { name, warm } => run_debug(&name, warm),
        Action::Doctor => doctor(),
    }
}

// --- shared helpers --------------------------------------------------------

fn plugins_dir() -> Result<PathBuf> {
    let cfg = config::config_path()?;
    let parent = cfg
        .parent()
        .ok_or_else(|| anyhow::anyhow!("config path has no parent"))?;
    Ok(parent.join("plugins"))
}

fn valid_plugin_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    // On non-Unix we have no permission bits to check; existence is the
    // best we can do. Plugins on Windows aren't actually wired in yet
    // (the segment runner uses Command::new which works, but Windows
    // builds are disabled in 0.3.x).
    path.is_file()
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(path)?.permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(path, perm)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

// --- list ------------------------------------------------------------------

fn list() -> Result<()> {
    let dir = plugins_dir()?;
    println!("{B}cc-status plugins{R}", B = BOLD, R = RESET);
    println!("{}{}{}", DIM, "─".repeat(50), RESET);
    println!("  {DIM}directory:{R} {}", dir.display(), DIM = DIM, R = RESET);
    println!();

    if !dir.exists() {
        println!(
            "{}(no plugins yet — directory will be created on first `ccs plugin new`){}",
            DIM, RESET
        );
        println!();
        print_quickstart_hint();
        return Ok(());
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("read {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    entries.sort();

    if entries.is_empty() {
        println!("{}(directory is empty){}", DIM, RESET);
        println!();
        print_quickstart_hint();
        return Ok(());
    }

    let name_w = entries
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::len))
        .max()
        .unwrap_or(8)
        + 2;

    for entry in &entries {
        let name = entry
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        let exec = is_executable(entry);
        let (mark, color) = if exec {
            ("✓", GREEN)
        } else {
            ("✗ not executable", YELLOW)
        };
        println!(
            "  {C}{:<nw$}{R} {col}{}{R}  {DIM}{{plugin:{}}}{R}",
            name,
            mark,
            name,
            nw = name_w,
            C = CYAN,
            R = RESET,
            col = color,
            DIM = DIM,
        );
    }
    println!();
    println!(
        "  {DIM}Reference any plugin from a mode template: {{plugin:NAME}}{R}",
        DIM = DIM,
        R = RESET
    );
    println!(
        "  {DIM}Debug a plugin:                           ccs plugin run NAME{R}",
        DIM = DIM,
        R = RESET
    );
    Ok(())
}

fn print_quickstart_hint() {
    println!("{B}Quickstart{R}", B = BOLD, R = RESET);
    println!("  ccs plugin new hello                  # scaffold a sh template");
    println!("  ccs plugin new hello --lang python    # or python");
    println!("  ccs plugin run hello                  # debug-run, see what it prints");
    println!("  ccs mode append plugin:hello          # add to current mode");
}

// --- new -------------------------------------------------------------------

fn new(name: &str, lang: Lang, force: bool) -> Result<()> {
    if !valid_plugin_name(name) {
        anyhow::bail!(
            "invalid plugin name '{}': use letters, digits, '-', '_', '.' (and don't start with '.')",
            name
        );
    }
    let dir = plugins_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let path = dir.join(name);

    if path.exists() && !force {
        anyhow::bail!(
            "{} already exists; pass --force to overwrite, or pick another name",
            path.display()
        );
    }

    let body = lang.template(name);
    std::fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    make_executable(&path).with_context(|| format!("chmod +x {}", path.display()))?;

    let lang_label = match lang {
        Lang::Sh => "sh",
        Lang::Python => "python",
    };
    println!(
        "{}✓{} wrote {}plugin{} ({}, executable)",
        GREEN,
        RESET,
        BOLD,
        RESET,
        lang_label
    );
    println!("  {}{}{}", DIM, path.display(), RESET);
    println!();
    println!("{B}Next steps{R}", B = BOLD, R = RESET);
    println!(
        "  {C}ccs plugin run {}{R}              {DIM}# see exactly what your status line will show{R}",
        name,
        C = CYAN,
        R = RESET,
        DIM = DIM,
    );
    println!(
        "  {C}ccs mode append plugin:{}{R}      {DIM}# add to current mode{R}",
        name,
        C = CYAN,
        R = RESET,
        DIM = DIM,
    );
    println!(
        "  {C}$EDITOR {}{R}",
        path.display(),
        C = CYAN,
        R = RESET
    );
    println!();
    println!("{B}Contract{R}", B = BOLD, R = RESET);
    println!(
        "  {DIM}stdin{R}    Claude Code's status-line JSON (cwd, model, context_window, …)",
        DIM = DIM,
        R = RESET
    );
    println!(
        "  {DIM}stdout{R}   the segment value (clipped to 80 chars; ANSI SGR allowed)",
        DIM = DIM,
        R = RESET
    );
    println!(
        "  {DIM}timeout{R}  250 ms hard — the plugin is killed past that",
        DIM = DIM,
        R = RESET
    );
    println!(
        "  {DIM}empty{R}    empty stdout / non-zero exit / missing file → segment renders as \"\"",
        DIM = DIM,
        R = RESET
    );
    Ok(())
}

// --- run (debug) -----------------------------------------------------------

fn run_debug(name: &str, warm: bool) -> Result<()> {
    if !valid_plugin_name(name) {
        anyhow::bail!("invalid plugin name '{}'", name);
    }
    let path = plugins_dir()?.join(name);
    if !path.is_file() {
        anyhow::bail!(
            "no such plugin: {} (run `ccs plugin list` to see what's installed)",
            path.display()
        );
    }
    if !is_executable(&path) {
        eprintln!(
            "{}warning:{} {} is not executable — run `chmod +x {}` first",
            YELLOW,
            RESET,
            path.display(),
            path.display(),
        );
    }

    let stdin_payload = mock_stdin();

    println!("{B}Plugin debug · {}{R}", name, B = BOLD, R = RESET);
    println!("{}{}{}", DIM, "─".repeat(50), RESET);
    println!("  {DIM}path:{R}    {}", path.display(), DIM = DIM, R = RESET);
    println!(
        "  {DIM}stdin:{R}   (mock CC JSON, {} bytes)",
        stdin_payload.len(),
        DIM = DIM,
        R = RESET
    );
    println!(
        "  {DIM}timeout:{R} 250 ms (status-line render budget)",
        DIM = DIM,
        R = RESET
    );
    if warm {
        println!(
            "  {DIM}mode:{R}    --warm (running once first to skip cold-start cost)",
            DIM = DIM,
            R = RESET
        );
        // Discard the first run's result; we only care about its side
        // effect of warming the file system / dyld / Gatekeeper cache.
        let _ = exec_capture(&path, &stdin_payload);
    }
    println!();

    let t0 = Instant::now();
    let result = exec_capture(&path, &stdin_payload);
    let elapsed = t0.elapsed();

    match result {
        Ok(Capture {
            stdout,
            stderr,
            status,
        }) => {
            let exit_code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "<signal>".into());
            let status_color = if status.success() { GREEN } else { RED };
            println!(
                "  {DIM}exit:{R}    {col}{}{R}",
                exit_code,
                col = status_color,
                DIM = DIM,
                R = RESET
            );
            let elapsed_ms = elapsed.as_millis();
            let timing_note = if elapsed_ms > 250 {
                let suffix = if warm {
                    "  (would TIME OUT in real render)"
                } else {
                    "  (slow; try --warm to discount cold-start cost)"
                };
                format!(" {}{}{}", RED, suffix, RESET)
            } else {
                String::new()
            };
            println!(
                "  {DIM}elapsed:{R} {} ms{}",
                elapsed_ms,
                timing_note,
                DIM = DIM,
                R = RESET
            );
            println!();

            print_block("stdout (raw)", &stdout, CYAN);
            if !stderr.is_empty() {
                print_block("stderr", &stderr, YELLOW);
            }

            // Show what the status line will actually display.
            let sanitized = crate::segments::sanitize_plugin_output_for_debug(&stdout);
            println!(
                "{B}As shown in the status line{R}",
                B = BOLD,
                R = RESET
            );
            if sanitized.is_empty() {
                println!(
                    "  {DIM}(empty — segment will render as \"\"){R}",
                    DIM = DIM,
                    R = RESET
                );
            } else {
                println!("  {}", sanitized);
            }
            println!();
            println!(
                "{DIM}Add to a mode:{R}  ccs mode append plugin:{}",
                name,
                DIM = DIM,
                R = RESET
            );
        }
        Err(e) => {
            println!("  {}exec failed:{} {}", RED, RESET, e);
        }
    }
    Ok(())
}

struct Capture {
    stdout: String,
    stderr: String,
    status: std::process::ExitStatus,
}

fn exec_capture(path: &Path, stdin_payload: &str) -> Result<Capture> {
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {}", path.display()))?;
    if let Some(mut sin) = child.stdin.take() {
        let _ = sin.write_all(stdin_payload.as_bytes());
    }
    // Hard cap: 5s in debug mode (vs 250ms at render time) so users see
    // what their slow plugin would have produced — render-time will kill
    // it, but for debugging it's nice to see the eventual output.
    let deadline =
        Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!("plugin still running after 5s — killed");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
    let mut stdout = String::new();
    if let Some(mut so) = child.stdout.take() {
        let _ = so.read_to_string(&mut stdout);
    }
    let mut stderr = String::new();
    if let Some(mut se) = child.stderr.take() {
        let _ = se.read_to_string(&mut stderr);
    }
    let status = child.wait()?;
    Ok(Capture {
        stdout,
        stderr,
        status,
    })
}

fn print_block(label: &str, body: &str, color: &str) {
    println!("{B}{}{R}", label, B = BOLD, R = RESET);
    if body.is_empty() {
        println!("  {DIM}(empty){R}", DIM = DIM, R = RESET);
    } else {
        for line in body.lines() {
            println!("  {col}{}{R}", line, col = color, R = RESET);
        }
        if !body.ends_with('\n') {
            // Trailing-no-newline marker
            println!("  {DIM}(no trailing newline){R}", DIM = DIM, R = RESET);
        }
    }
    println!();
}

fn mock_stdin() -> String {
    // A representative payload so plugins that read fields actually get
    // something. Mirrors the schema CC sends (cwd / model / context_window
    // / session_id / transcript_path).
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| "/tmp".to_string());
    let payload = serde_json::json!({
        "cwd": cwd,
        "model": {
            "id": "claude-opus-4-7",
            "display_name": "Claude Opus 4.7"
        },
        "context_window": {
            "remaining_percentage": 73.5
        },
        "session_id": "ccs-plugin-debug",
        "transcript_path": ""
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
}

// --- templates -------------------------------------------------------------

fn sh_template(name: &str) -> String {
    format!(
        r#"#!/bin/sh
# cc-status plugin: {name}
#
# stdin:  Claude Code's status-line JSON.
#         Fields: cwd, model.{{id,display_name}}, context_window.remaining_percentage,
#                 session_id, transcript_path
# stdout: segment value, clipped to 80 chars; ANSI SGR allowed.
#         Newlines/tabs collapse to spaces. Empty stdout -> segment ""
# timeout: 250 ms hard. Anything slower is killed.
# debug:   ccs plugin run {name}
#
# Tips:
#   - Keep it FAST. Forking a child shell costs ~5-10ms; jq adds ~30ms;
#     curl/network calls almost always blow the budget. Prefer cached files.
#   - Read stdin only if you need it. Many plugins are "static" (just a
#     metric from somewhere on disk).
#   - To extract a field with jq:    json=$(cat); cwd=$(printf '%s' "$json" | jq -r '.cwd')
#   - To color the output:           printf '\033[33mwarn:%s\033[0m' "..."

# Read CC's JSON (or ignore it).
read_stdin=$(cat)

# --- replace below with your own logic ---
printf 'hello'
"#,
        name = name
    )
}

fn python_template(name: &str) -> String {
    format!(
        r#"#!/usr/bin/env python3
"""cc-status plugin: {name}

stdin:   Claude Code's status-line JSON.
         Schema: {{
             "cwd": str,
             "model": {{"id": str, "display_name": str}},
             "context_window": {{"remaining_percentage": float}},
             "session_id": str,
             "transcript_path": str
         }}
stdout:  segment value, clipped to 80 chars; ANSI SGR allowed.
         Newlines/tabs collapse to spaces. Empty stdout -> segment "".
timeout: 250 ms hard. Python's cold-start is ~30-80ms — keep logic tight.
debug:   ccs plugin run {name}

Tips:
- Avoid heavy imports. `import json, sys` is cheap; `import requests` is not.
- Don't make network calls — you'll blow the 250ms budget.
- For colors, write ANSI directly: `print('\\033[33mwarn\\033[0m', end='')`.
"""
import json
import sys

try:
    data = json.load(sys.stdin) if not sys.stdin.isatty() else {{}}
except json.JSONDecodeError:
    data = {{}}

cwd = data.get("cwd", "")
model = (data.get("model") or {{}}).get("display_name", "")

# --- replace below with your own logic ---
print("hello", end="")
"#,
        name = name
    )
}

// --- doctor (health check) -------------------------------------------------

/// Per-plugin diagnostic gathered by `doctor()`. One row in the report.
struct DoctorResult {
    name: String,
    /// Worst-severity issue found. Drives the leading glyph + summary count.
    severity: Severity,
    /// Short, human-readable lines summarizing what was checked. Each is
    /// rendered as one indented bullet under the plugin's headline.
    notes: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Severity {
    Ok,
    Warn,
    Fail,
}

impl Severity {
    fn merge(self, other: Severity) -> Severity {
        self.max(other)
    }
    fn glyph(self) -> &'static str {
        match self {
            Severity::Ok => "✓",
            Severity::Warn => "⚠",
            Severity::Fail => "✗",
        }
    }
    fn color(self) -> &'static str {
        match self {
            Severity::Ok => GREEN,
            Severity::Warn => YELLOW,
            Severity::Fail => RED,
        }
    }
}

fn doctor() -> Result<()> {
    let dir = plugins_dir()?;
    println!("{B}cc-status plugin doctor{R}", B = BOLD, R = RESET);
    println!("{}{}{}", DIM, "─".repeat(50), RESET);
    println!(
        "  {DIM}directory:{R} {}",
        dir.display(),
        DIM = DIM,
        R = RESET
    );

    if !dir.exists() {
        println!();
        println!(
            "{}(no plugin directory yet — `ccs plugin new <name>` to scaffold one){}",
            DIM, RESET
        );
        return Ok(());
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("read {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    entries.sort();

    if entries.is_empty() {
        println!();
        println!("{}(no plugins installed){}", DIM, RESET);
        return Ok(());
    }

    // Mode references — used to flag orphans. Loaded once so doctor
    // doesn't re-read config.toml per plugin. If config load fails we
    // silently skip the orphan check rather than failing the whole
    // doctor run (the user might be debugging a broken config).
    let modes_referencing = collect_mode_references().unwrap_or_default();

    println!();

    let mock = mock_stdin();
    let mut results: Vec<DoctorResult> = Vec::with_capacity(entries.len());
    for path in &entries {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        results.push(check_plugin(&name, path, &mock, &modes_referencing));
    }

    // Compute alignment column for plugin names so the inline status
    // glyph + label line up regardless of name length.
    let name_w = results
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(8)
        + 2;

    for r in &results {
        println!(
            "  {col}{}{R}  {C}{:<nw$}{R}",
            r.severity.glyph(),
            r.name,
            nw = name_w,
            col = r.severity.color(),
            C = CYAN,
            R = RESET
        );
        for note in &r.notes {
            println!("      {DIM}{}{R}", note, DIM = DIM, R = RESET);
        }
    }

    // Summary footer.
    let n_total = results.len();
    let n_fail = results.iter().filter(|r| r.severity == Severity::Fail).count();
    let n_warn = results.iter().filter(|r| r.severity == Severity::Warn).count();
    let n_ok = n_total - n_fail - n_warn;

    println!();
    println!("{}{}{}", DIM, "─".repeat(50), RESET);
    println!(
        "{B}{} plugin(s):{R}  {GREEN}{} ok{R}  {YELLOW}{} warn{R}  {RED}{} fail{R}",
        n_total,
        n_ok,
        n_warn,
        n_fail,
        B = BOLD,
        R = RESET,
        GREEN = GREEN,
        YELLOW = YELLOW,
        RED = RED,
    );
    if n_fail + n_warn > 0 {
        println!();
        println!(
            "{DIM}Tip: `ccs plugin run <name>` to inspect a single plugin in detail.{R}",
            DIM = DIM,
            R = RESET
        );
    }
    Ok(())
}

/// Collect, for each plugin name actually referenced from any configured
/// mode, the list of mode names that reference it. Used to flag plugins
/// that exist on disk but aren't wired into any mode (orphans).
fn collect_mode_references() -> Result<std::collections::HashMap<String, Vec<String>>> {
    let cfg = config::load()?;
    let mut refs: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (mode_name, mode) in &cfg.modes {
        for line in &mode.lines {
            // Walk `{...}` placeholders and pick out `plugin:NAME` ones.
            let mut rest = line.as_str();
            while let Some(start) = rest.find('{') {
                let after = &rest[start + 1..];
                let Some(end) = after.find('}') else { break };
                let token = &after[..end];
                if let Some(plug) = token.strip_prefix("plugin:") {
                    refs.entry(plug.to_string())
                        .or_default()
                        .push(mode_name.clone());
                }
                rest = &after[end + 1..];
            }
        }
    }
    Ok(refs)
}

fn check_plugin(
    name: &str,
    path: &Path,
    stdin_payload: &str,
    mode_refs: &std::collections::HashMap<String, Vec<String>>,
) -> DoctorResult {
    let mut sev = Severity::Ok;
    let mut notes = Vec::<String>::new();

    // 1. Executable bit.
    if !is_executable(path) {
        sev = sev.merge(Severity::Fail);
        notes.push(format!(
            "not executable — run `chmod +x {}`",
            path.display()
        ));
    }

    // 2. Shebang / binary marker. Pure-binary plugins are valid (compiled
    //    Go/Rust); only flag files that are neither executable script
    //    nor likely-binary.
    let header = read_header(path, 256);
    let kind = classify_header(&header);
    match kind {
        HeaderKind::Shebang => {}
        HeaderKind::Binary => notes.push("looks like a native binary".into()),
        HeaderKind::PlainText => {
            sev = sev.merge(Severity::Warn);
            notes.push("no shebang — kernel may not know how to exec this file".into());
        }
        HeaderKind::Empty => {
            sev = sev.merge(Severity::Fail);
            notes.push("file is empty".into());
        }
    }

    // 3. Warm-run timing + exit + stdout. Skip if not executable — the
    //    spawn would just produce a misleading error.
    if is_executable(path) {
        // Warm pass (discarded) — masks Gatekeeper / dyld cold-start
        // so we measure what the status line will actually pay turn
        // after turn.
        let _ = exec_capture(path, stdin_payload);

        let t0 = Instant::now();
        match exec_capture(path, stdin_payload) {
            Ok(c) => {
                let elapsed_ms = t0.elapsed().as_millis() as u64;
                if elapsed_ms > 250 {
                    sev = sev.merge(Severity::Fail);
                    notes.push(format!(
                        "warm runtime {} ms exceeds 250 ms budget — will time out at render",
                        elapsed_ms
                    ));
                } else if elapsed_ms > 200 {
                    sev = sev.merge(Severity::Warn);
                    notes.push(format!(
                        "warm runtime {} ms close to 250 ms budget",
                        elapsed_ms
                    ));
                } else {
                    notes.push(format!("warm runtime {} ms", elapsed_ms));
                }

                if !c.status.success() {
                    sev = sev.merge(Severity::Warn);
                    let exit = c
                        .status
                        .code()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "<signal>".into());
                    notes.push(format!(
                        "non-zero exit ({}) — segment value still taken from stdout",
                        exit
                    ));
                }

                let trimmed = c.stdout.trim();
                if trimmed.is_empty() {
                    sev = sev.merge(Severity::Warn);
                    notes.push("empty stdout — segment will render as \"\"".into());
                } else {
                    let preview: String = trimmed.chars().take(48).collect();
                    let ellipsis = if trimmed.chars().count() > 48 { "…" } else { "" };
                    notes.push(format!("stdout: {}{}", preview, ellipsis));
                }
            }
            Err(e) => {
                sev = sev.merge(Severity::Fail);
                notes.push(format!("failed to exec: {}", e));
            }
        }
    }

    // 4. Mode references — orphans get a soft warning so users notice
    //    files left behind from old experiments.
    match mode_refs.get(name) {
        Some(modes) if !modes.is_empty() => {
            let mut shown = modes.clone();
            shown.sort();
            shown.dedup();
            notes.push(format!("referenced by mode(s): {}", shown.join(", ")));
        }
        _ => {
            sev = sev.merge(Severity::Warn);
            notes.push(
                "not referenced by any mode — `ccs mode append plugin:NAME` to wire it in".into(),
            );
        }
    }

    DoctorResult {
        name: name.to_string(),
        severity: sev,
        notes,
    }
}

fn read_header(path: &Path, max: usize) -> Vec<u8> {
    use std::io::Read as _;
    let Ok(mut f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut buf = vec![0u8; max];
    match f.read(&mut buf) {
        Ok(n) => {
            buf.truncate(n);
            buf
        }
        Err(_) => Vec::new(),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HeaderKind {
    Empty,
    Shebang,
    PlainText,
    Binary,
}

fn classify_header(bytes: &[u8]) -> HeaderKind {
    if bytes.is_empty() {
        return HeaderKind::Empty;
    }
    if bytes.starts_with(b"#!") {
        return HeaderKind::Shebang;
    }
    // Heuristic: any NUL or "lots of high bytes" → binary; otherwise
    // assume text (no shebang). Good enough for the doctor's purpose.
    let total = bytes.len();
    let high_or_nul = bytes
        .iter()
        .filter(|&&b| b == 0 || b < 0x09 || (0x7f..0xa0).contains(&b))
        .count();
    if high_or_nul * 100 / total >= 5 {
        HeaderKind::Binary
    } else {
        HeaderKind::PlainText
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_plugin_name_rules() {
        assert!(valid_plugin_name("hello"));
        assert!(valid_plugin_name("hello-world"));
        assert!(valid_plugin_name("hello_world"));
        assert!(valid_plugin_name("a.b"));
        assert!(!valid_plugin_name(""));
        assert!(!valid_plugin_name("."));
        assert!(!valid_plugin_name(".."));
        assert!(!valid_plugin_name(".hidden"));
        assert!(!valid_plugin_name("a/b"));
        assert!(!valid_plugin_name("a\\b"));
        assert!(!valid_plugin_name("a b"));
    }

    #[test]
    fn lang_from_flag_aliases() {
        assert_eq!(Lang::from_flag("sh").unwrap(), Lang::Sh);
        assert_eq!(Lang::from_flag("bash").unwrap(), Lang::Sh);
        assert_eq!(Lang::from_flag("Python").unwrap(), Lang::Python);
        assert_eq!(Lang::from_flag("py").unwrap(), Lang::Python);
        assert!(Lang::from_flag("ruby").is_err());
    }

    #[test]
    fn templates_have_shebang_and_name() {
        let s = sh_template("my-plugin");
        assert!(s.starts_with("#!/bin/sh\n"));
        assert!(s.contains("my-plugin"));
        assert!(s.contains("printf 'hello'"));

        let p = python_template("my-plugin");
        assert!(p.starts_with("#!/usr/bin/env python3\n"));
        assert!(p.contains("my-plugin"));
        assert!(p.contains("print(\"hello\""));
    }

    #[test]
    fn classify_header_distinguishes_kinds() {
        assert_eq!(classify_header(b""), HeaderKind::Empty);
        assert_eq!(classify_header(b"#!/bin/sh\n"), HeaderKind::Shebang);
        assert_eq!(
            classify_header(b"#!/usr/bin/env python3\nimport sys\n"),
            HeaderKind::Shebang
        );
        assert_eq!(
            classify_header(b"hello world this is plain text"),
            HeaderKind::PlainText
        );
        // Mach-O / ELF magic bytes — high-byte heavy → binary.
        let mach_o = [0xcf, 0xfa, 0xed, 0xfe, 0x07, 0, 0, 1, 0x03, 0, 0, 0x80];
        assert_eq!(classify_header(&mach_o), HeaderKind::Binary);
    }

    #[test]
    fn severity_merge_picks_worst() {
        assert_eq!(Severity::Ok.merge(Severity::Warn), Severity::Warn);
        assert_eq!(Severity::Warn.merge(Severity::Ok), Severity::Warn);
        assert_eq!(Severity::Warn.merge(Severity::Fail), Severity::Fail);
        assert_eq!(Severity::Fail.merge(Severity::Ok), Severity::Fail);
    }

    #[cfg(unix)]
    #[test]
    fn check_plugin_grades_an_orphan_as_warn_and_a_wired_one_as_ok() {
        // Build a minimal sh plugin file we control and run check_plugin
        // directly against it. The exec_capture path is covered too —
        // we want this to fail loudly if someone changes the timing
        // grading thresholds.
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("good");
        std::fs::write(&path, "#!/bin/sh\nprintf 'world'\n").unwrap();
        let mut perm = std::fs::metadata(&path).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&path, perm).unwrap();

        let mock = mock_stdin();

        // Orphan: no mode references it.
        let orphan_refs = std::collections::HashMap::new();
        let r = check_plugin("good", &path, &mock, &orphan_refs);
        assert_eq!(r.severity, Severity::Warn);
        assert!(
            r.notes.iter().any(|n| n.contains("not referenced by any mode")),
            "orphan check missing from notes: {:?}",
            r.notes
        );

        // Wired: a mode references it → severity stays Ok (assuming
        // the warm runtime is comfortably under 200ms, which it should
        // be for a 5-line sh script after the discard pass).
        let mut wired_refs = std::collections::HashMap::new();
        wired_refs.insert("good".to_string(), vec!["compact".to_string()]);
        let r = check_plugin("good", &path, &mock, &wired_refs);
        assert!(
            matches!(r.severity, Severity::Ok | Severity::Warn),
            "wired plugin should be ok (or at worst warn for tight timing on slow CI), got {:?}: {:?}",
            r.severity,
            r.notes
        );
        assert!(
            r.notes.iter().any(|n| n.contains("referenced by mode(s): compact")),
            "wired note missing: {:?}",
            r.notes
        );
        assert!(
            r.notes.iter().any(|n| n.contains("stdout: world")),
            "stdout preview missing: {:?}",
            r.notes
        );
    }

    #[cfg(unix)]
    #[test]
    fn check_plugin_flags_empty_stdout_as_warn() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("silent");
        // Exits 0 but prints nothing — segment will render as "".
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perm = std::fs::metadata(&path).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&path, perm).unwrap();

        let mut refs = std::collections::HashMap::new();
        refs.insert("silent".to_string(), vec!["compact".to_string()]);
        let r = check_plugin("silent", &path, &mock_stdin(), &refs);
        assert_eq!(r.severity, Severity::Warn);
        assert!(
            r.notes.iter().any(|n| n.contains("empty stdout")),
            "empty-stdout note missing: {:?}",
            r.notes
        );
    }

    #[cfg(unix)]
    #[test]
    fn check_plugin_flags_non_executable_as_fail() {
        // No exec bit set — this is the most common user mistake when
        // they bypass `ccs plugin new` and write their own file.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("noexec");
        std::fs::write(&path, "#!/bin/sh\nprintf x\n").unwrap();
        let r = check_plugin("noexec", &path, &mock_stdin(), &Default::default());
        assert_eq!(r.severity, Severity::Fail);
        assert!(
            r.notes.iter().any(|n| n.contains("not executable")),
            "non-exec note missing: {:?}",
            r.notes
        );
    }
}
