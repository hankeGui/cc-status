use crate::{cache, config, daemon, rollup, segments, transcript};
use anyhow::Result;
use serde_json::Value;
use std::io::Read;
use std::path::PathBuf;

pub fn run() -> Result<()> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    let stdin: Value = if buf.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&buf).unwrap_or(Value::Null)
    };

    // Try the daemon first; fall back to inline rendering on any failure
    // so the user is never blocked by a crashed daemon.
    if let Some(out) = daemon::try_render_via_daemon(&stdin) {
        println!("{}", out);
        return Ok(());
    }

    let out = render_string(&stdin)?;
    println!("{}", out);
    Ok(())
}

/// Pure rendering path: takes the parsed CC stdin JSON and returns the
/// final ANSI string (without a trailing newline). Shared between
/// `ccs render` and the daemon's request handler.
pub fn render_string(stdin: &Value) -> Result<String> {
    let cfg = config::load()?;

    let session_id = stdin
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    let mut sess = cache::load(&session_id);
    if let Some(p) = stdin.get("transcript_path").and_then(|v| v.as_str()) {
        let path = PathBuf::from(p);
        if path.exists() {
            let _ = transcript::update(&path, &mut sess);
            let _ = cache::save(&session_id, &sess);
        }
    }

    let mode_name = &cfg.current_mode;
    let mode = cfg.modes.get(mode_name).or_else(|| cfg.modes.get("full"));
    let Some(mode) = mode else {
        return Ok(String::new());
    };

    // Only do the cross-session rollup scan if a cost_today / cost_week
    // / cost segment is actually referenced by the active mode.
    let needs_rollup = mode.lines.iter().any(|line| {
        line.contains("{cost_today}") || line.contains("{cost_week}") || line.contains("{cost}")
    });
    let rollup_data = if needs_rollup {
        let mut r = rollup::load();
        rollup::refresh(&mut r);
        let _ = rollup::save(&r);
        Some(r)
    } else {
        None
    };

    let ctx = segments::Ctx {
        stdin,
        cache: &sess,
        cfg: &cfg,
        rollup: rollup_data.as_ref(),
    };

    let lines: Vec<String> = mode
        .lines
        .iter()
        .map(|tpl| render_line(tpl, &ctx))
        .collect();
    Ok(lines.join("\n"))
}

/// Expand `{segment}` placeholders in `template` against `ctx` and
/// collapse the runs of whitespace left behind by empty segments.
/// Public so other subcommands (notably `mode list`'s preview) can
/// share the exact rendering path the status line uses.
pub fn render_line(template: &str, ctx: &segments::Ctx) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('}') {
            let name = &after[..end];
            out.push_str(&segments::render(name, ctx));
            rest = &after[end + 1..];
        } else {
            out.push_str(&rest[start..]);
            rest = "";
        }
    }
    out.push_str(rest);
    collapse_spaces(&out)
}

/// Trim runs of whitespace that result from empty segments.
fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == ' ' {
            if !prev_space {
                out.push(ch);
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}
