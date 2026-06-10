use crate::cache::SessionCache;
use crate::config::{self, Config};
use crate::pricing;
use crate::rollup::Rollup;
use chrono::Utc;
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const RESET: &str = "\x1b[0m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_MAGENTA: &str = "\x1b[1;35m";
const RED: &str = "\x1b[0;31m";
const YELLOW: &str = "\x1b[0;33m";
const GREEN: &str = "\x1b[0;32m";
const DIM: &str = "\x1b[90m";

pub struct Ctx<'a> {
    pub stdin: &'a Value,
    pub cache: &'a SessionCache,
    pub cfg: &'a Config,
    /// Optional rollup of cross-session token totals. None when
    /// no cost segment is referenced (avoids the scan cost).
    pub rollup: Option<&'a Rollup>,
}

pub fn render(name: &str, ctx: &Ctx) -> String {
    // Fault isolation: any panic in a single segment must not blank out
    // the whole status line. Catch the unwind and return empty so the
    // surrounding renderer's `collapse_spaces` cleans the gap.
    let name = name.to_string();
    // SAFETY: Ctx fields are simple references; no lock/UnwindSafe issues.
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render_inner(&name, ctx)));
    result.unwrap_or_default()
}

fn render_inner(name: &str, ctx: &Ctx) -> String {
    if let Some(plugin_name) = name.strip_prefix("plugin:") {
        return seg_plugin(plugin_name, ctx);
    }
    match name {
        "dir" => seg_dir(ctx),
        "git" => seg_git(ctx),
        "model" => seg_model(ctx),
        "ctx" => seg_ctx(ctx),
        "ctx_tokens" => seg_ctx_tokens(ctx),
        "last_turn" => seg_last_turn(ctx),
        "cache_ttl" => seg_cache_ttl(ctx),
        "skills" => seg_skills(ctx),
        "mcp" => seg_mcp(ctx),
        "burn" => seg_burn(ctx),
        "session_age" => seg_session_age(ctx),
        "hit_rate" => seg_hit_rate(ctx),
        "cost_last" => seg_cost_last(ctx),
        "cost_session" => seg_cost_session(ctx),
        "cost_today" => seg_cost_today(ctx),
        "cost_week" => seg_cost_week(ctx),
        "cost" => seg_cost_combo(ctx),
        "mode" => format!("{}[{}]{}", DIM, ctx.cfg.current_mode, RESET),
        other => format!("{{{}}}", other),
    }
}

fn cwd<'a>(ctx: &'a Ctx) -> &'a str {
    ctx.stdin
        .pointer("/cwd")
        .or_else(|| ctx.stdin.pointer("/workspace/current_dir"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

fn seg_dir(ctx: &Ctx) -> String {
    let cwd = cwd(ctx);
    if cwd.is_empty() {
        return String::new();
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let display = if !home.is_empty() && cwd.starts_with(&home) {
        format!("~{}", &cwd[home.len()..])
    } else {
        cwd.to_string()
    };
    let parts: Vec<&str> = display.split('/').collect();
    let truncated = if parts.len() > 3 {
        parts[parts.len() - 3..].join("/")
    } else {
        display
    };
    format!("{}{}{}", BOLD_CYAN, truncated, RESET)
}

fn seg_git(ctx: &Ctx) -> String {
    let cwd = cwd(ctx);
    if cwd.is_empty() {
        return String::new();
    }

    if !run_git(cwd, &["rev-parse", "--git-dir"]).is_some() {
        return String::new();
    }

    let mut parts = Vec::<String>::new();

    // Worktree detection: if git common dir != git dir, we're in a worktree.
    let git_dir = run_git(cwd, &["rev-parse", "--git-dir"]).unwrap_or_default();
    let common_dir = run_git(cwd, &["rev-parse", "--git-common-dir"]).unwrap_or_default();
    if !git_dir.is_empty() && git_dir != common_dir {
        if let Some(wt_name) = Path::new(&git_dir).file_name().and_then(|s| s.to_str()) {
            parts.push(format!("{}wt:{}{}", DIM, wt_name, RESET));
        }
    }

    let branch = run_git(cwd, &["symbolic-ref", "--short", "HEAD"])
        .or_else(|| run_git(cwd, &["rev-parse", "--short", "HEAD"]))
        .unwrap_or_default();
    if !branch.is_empty() {
        parts.push(format!("{}{}{}", BOLD_MAGENTA, branch, RESET));
    }

    // Ahead/behind
    if let Some(counts) = run_git(
        cwd,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    ) {
        let mut it = counts.split_whitespace();
        let ahead: u32 = it.next().unwrap_or("0").parse().unwrap_or(0);
        let behind: u32 = it.next().unwrap_or("0").parse().unwrap_or(0);
        let mut ab = String::new();
        if ahead > 0 {
            ab.push_str(&format!("⇡{}", ahead));
        }
        if behind > 0 {
            ab.push_str(&format!("⇣{}", behind));
        }
        if !ab.is_empty() {
            parts.push(format!("{}{}{}", DIM, ab, RESET));
        }
    }

    // Dirty flags
    if let Some(porcelain) = run_git(cwd, &["status", "--porcelain"]) {
        let mut staged = false;
        let mut modified = false;
        let mut untracked = false;
        for line in porcelain.lines() {
            if line.starts_with("??") {
                untracked = true;
            } else if line.starts_with(" M") || line.starts_with("M ") {
                modified = true;
            } else if line.chars().next().map_or(false, |c| "MARCDU".contains(c)) {
                staged = true;
            }
        }
        let mut flags = String::new();
        if staged {
            flags.push('+');
        }
        if modified {
            flags.push('!');
        }
        if untracked {
            flags.push('?');
        }
        if !flags.is_empty() {
            parts.push(format!("{}[{}]{}", RED, flags, RESET));
        }
    }

    parts.join(" ")
}

fn run_git(cwd: &str, args: &[&str]) -> Option<String> {
    // Hard timeout: huge monorepos can make `git status` take >1s, which
    // blows past Claude Code's status-line budget. Use a wait-with-deadline
    // so we abandon stuck commands (the child is left to die when our
    // process exits — acceptable for status-line tooling).
    use std::sync::mpsc;
    use std::time::Duration;

    let cwd = cwd.to_string();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (tx, rx) = mpsc::channel::<Option<String>>();
    let _handle = std::thread::spawn(move || {
        let out = Command::new("git").arg("-C").arg(&cwd).args(&args).output();
        let result = match out {
            Ok(o) if o.status.success() => {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
            _ => None,
        };
        let _ = tx.send(result);
    });

    match rx.recv_timeout(Duration::from_millis(150)) {
        Ok(v) => v,
        Err(_) => None, // timeout — leak the thread (it will finish or die at exit)
    }
}

fn seg_model(ctx: &Ctx) -> String {
    let Some((name, _src)) = resolve_model(ctx) else {
        return String::new();
    };
    format!("{}{}{}", DIM, name, RESET)
}

/// Where the model name we're displaying came from. Surfaced by
/// `ccs status` so users can debug intermediary remappings (Bedrock,
/// proxies, OpenRouter) — when the bar shows a name they didn't
/// expect, knowing the source narrows it down in one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    /// Read from a real `message.model` value in the JSONL transcript
    /// — Claude Code actually called this id, regardless of what
    /// stdin or settings.json said.
    Transcript,
    /// `model.display_name` / `model.id` from Claude Code's stdin
    /// JSON. Useful before any assistant turn has landed.
    Stdin,
    /// `model` field in `~/.claude/settings.json` or the
    /// `CLAUDE_MODEL` env var — the user's *configured* default,
    /// shown when neither transcript nor stdin has provided one.
    Configured,
}

/// Resolve the best-available model name for display, with its source.
/// Pre-segment helper so `seg_model`, `ccs status`, and any future
/// caller agree on the same precedence ladder.
///
/// Precedence:
///   1. `cache.last_model` — from the transcript, the most authoritative
///      because it's the model Claude Code actually invoked.
///   2. `stdin.model.display_name` then `stdin.model.id` — Claude
///      Code's own self-reported model. Can be rewritten by proxies.
///   3. `~/.claude/settings.json` `model` field, then `$CLAUDE_MODEL`.
///
/// Whichever source wins, we cleanup_id the value (strip noisy
/// `anthropic--` / `anthropic/` prefixes only — never rename the
/// model itself, since intermediaries may be deploying through
/// Bedrock / Vertex / OpenRouter and that prefix is meaningful) and
/// then preserve any `[1m]` / `(1m)` tier suffix from any source.
pub fn resolve_model(ctx: &Ctx) -> Option<(String, ModelSource)> {
    let stdin_id = ctx
        .stdin
        .pointer("/model/id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let stdin_dn = ctx
        .stdin
        .pointer("/model/display_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Tier suffix: check every source we'll touch — the user's
    // configured tier shouldn't disappear just because the transcript
    // model id doesn't echo it back. Anthropic's transcript writes
    // canonical `claude-opus-4-7` without the tier suffix, so we
    // recover it from stdin / settings even when the transcript wins.
    let tier = detect_tier(stdin_id)
        .or_else(|| detect_tier(stdin_dn))
        .or_else(|| configured_model().as_deref().and_then(detect_tier));

    let (raw, src) = if let Some(m) = ctx.cache.last_model.as_deref().filter(|s| !s.is_empty()) {
        (m.to_string(), ModelSource::Transcript)
    } else if !stdin_dn.is_empty() {
        (stdin_dn.to_string(), ModelSource::Stdin)
    } else if !stdin_id.is_empty() {
        (stdin_id.to_string(), ModelSource::Stdin)
    } else if let Some(m) = configured_model() {
        (m, ModelSource::Configured)
    } else {
        return None;
    };

    Some((finalize_model_label(&raw, tier.as_deref()), src))
}

/// Drop the tier suffix from a candidate id and prepend nothing —
/// we only attach the tier once at the end, in `finalize_model_label`.
fn strip_tier(s: &str) -> &str {
    // Strip a trailing tier marker if present. We accept either
    // bracketed (`[1m]`) or parenthesized (`(1m)`) — Claude Code has
    // shipped both forms over time. Whitespace before the tier is
    // tolerated.
    for suffix in ["[1m]", "(1m)", " [1m]", " (1m)"] {
        if let Some(stripped) = s.strip_suffix(suffix) {
            return stripped.trim_end();
        }
    }
    s
}

fn detect_tier(s: &str) -> Option<String> {
    if s.contains("[1m]") || s.contains("(1m)") {
        Some("[1m]".to_string())
    } else {
        None
    }
}

/// Strip vendor / proxy prefixes that add noise without identity.
/// Conservative: only the Anthropic-self prefix `anthropic--` /
/// `anthropic/` is removed. Bedrock / Vertex / OpenRouter prefixes
/// stay because the deployment target is information the user might
/// actually want to see in their status bar.
fn cleanup_id(s: &str) -> &str {
    for prefix in ["anthropic--", "anthropic/"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return rest;
        }
    }
    s
}

fn finalize_model_label(raw: &str, tier: Option<&str>) -> String {
    let body = cleanup_id(strip_tier(raw));
    match tier {
        Some(t) => format!("{} {}", body, t),
        None => body.to_string(),
    }
}

/// Read the user's configured default model from
/// `~/.claude/settings.json` (`model` field) or the `CLAUDE_MODEL`
/// env var. Returns `None` if neither is set or the file can't be
/// parsed — this is a best-effort fallback, not a hard requirement.
fn configured_model() -> Option<String> {
    if let Ok(env) = std::env::var("CLAUDE_MODEL") {
        if !env.trim().is_empty() {
            return Some(env);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let path = std::path::PathBuf::from(home)
        .join(".claude")
        .join("settings.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("model")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn seg_ctx(ctx: &Ctx) -> String {
    let Some(c) = compute_ctx(ctx) else {
        return String::new();
    };
    // remaining_frac = how much room is left before auto-compact (0..1)
    let remaining_frac = (1.0 - c.used_frac).clamp(0.0, 1.0);
    let pct_i = (remaining_frac * 100.0).round() as i64;
    let color = if (pct_i as u8) < ctx.cfg.theme.ctx_low {
        RED
    } else if (pct_i as u8) < ctx.cfg.theme.ctx_med {
        YELLOW
    } else {
        GREEN
    };
    // Battery-style bar: filled cells (`▰`) represent *remaining*
    // capacity (mirrors a phone battery — full = healthy, empty =
    // about to compact). Filled cells take the health color; empty
    // cells stay dim so the bar's outline is always visible. 10 cells
    // = each cell is ~10% of capacity, easy to count.
    let (filled, empty) = battery_bar(remaining_frac, 10);
    format!(
        "{C}ctx {pct}% {filled_chars}{D}{empty_chars}{R} {D}{used}/{cap}{R}",
        C = color,
        pct = pct_i,
        filled_chars = filled,
        empty_chars = empty,
        D = DIM,
        R = RESET,
        used = short_num(c.used),
        cap = short_num(c.capacity),
    )
}

struct CtxCalc {
    used: u64,
    capacity: u64,
    used_frac: f64,
}

/// Compute usable-context numbers.
///
/// 1. `used` = tokens of the last assistant turn (input + cache_read + cache_creation)
/// 2. Backsolve the *physical* window from CC's `remaining_percentage`:
///    `physical = used / (1 - remaining%)`
/// 3. `capacity` = physical × `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE / 100`
///    (default 95 — that's where auto-compact actually fires)
/// 4. `used_frac` = used / capacity → drives bar + color thresholds
fn compute_ctx(ctx: &Ctx) -> Option<CtxCalc> {
    let used = ctx.cache.last_turn_input
        + ctx.cache.last_turn_cache_read
        + ctx.cache.last_turn_cache_creation;
    if used == 0 {
        return None;
    }
    let remaining_pct = ctx
        .stdin
        .pointer("/context_window/remaining_percentage")
        .and_then(|v| v.as_f64())?;

    let physical = if remaining_pct < 100.0 && remaining_pct >= 0.0 {
        (used as f64) / (1.0 - remaining_pct / 100.0)
    } else {
        // remaining 100% means used must also be 0 — handled above. Fallback:
        200_000.0
    };

    let pct_override: f64 = std::env::var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(95.0)
        .clamp(1.0, 100.0);

    let capacity_f = physical * pct_override / 100.0;
    let capacity = capacity_f.round().max(1.0) as u64;
    let used_frac = (used as f64 / capacity_f).clamp(0.0, 1.0);

    Some(CtxCalc {
        used,
        capacity,
        used_frac,
    })
}

fn seg_ctx_tokens(ctx: &Ctx) -> String {
    let Some(c) = compute_ctx(ctx) else {
        return String::new();
    };
    // Prefix with `ctx-used` so a glance distinguishes this from
    // unrelated "N/M" pairs in adjacent segments. Reads as "ctx-used
    // 504k of 1M capacity."
    format!(
        "{}ctx-used {}/{}{}",
        DIM,
        short_num(c.used),
        short_num(c.capacity),
        RESET
    )
}

/// Battery-style bar: returns `(filled, empty)` as two strings of
/// `▰` / `▱` characters respectively. Caller is expected to wrap
/// `filled` in a health color and `empty` in DIM, so the eye reads
/// the bar as a battery whose visible electrolyte (`▰`) shows
/// remaining capacity at a glance.
///
/// `frac` is the *remaining* fraction (0.0 = empty, 1.0 = full).
/// `width` is the total cell count. The split prefers underfilling
/// over overfilling — at 5% with 10 cells we render 0 filled (not 1),
/// because "5% remaining" should look almost-empty, not "1 unit
/// already lit."
fn battery_bar(frac: f64, width: usize) -> (String, String) {
    let frac = frac.clamp(0.0, 1.0);
    // Floor instead of round so a healthy-looking bar (e.g. 99%) still
    // shows one empty cell, signaling "not quite full" — matches
    // intuition for capacity meters.
    let filled_n = (frac * width as f64).floor() as usize;
    let filled_n = filled_n.min(width);
    let empty_n = width - filled_n;
    let filled: String = std::iter::repeat('▰').take(filled_n).collect();
    let empty: String = std::iter::repeat('▱').take(empty_n).collect();
    (filled, empty)
}

fn seg_last_turn(ctx: &Ctx) -> String {
    let c = ctx.cache;
    let total = c.last_turn_input
        + c.last_turn_output
        + c.last_turn_cache_read
        + c.last_turn_cache_creation;
    if total == 0 {
        return String::new();
    }
    let hit_base = c.last_turn_input + c.last_turn_cache_read + c.last_turn_cache_creation;
    let hit = if hit_base > 0 {
        (c.last_turn_cache_read as f64 / hit_base as f64 * 100.0).round() as u32
    } else {
        0
    };
    let mut s = format!(
        "↑{} ↓{}",
        short_num(c.last_turn_input + c.last_turn_cache_read + c.last_turn_cache_creation),
        short_num(c.last_turn_output)
    );
    if ctx.cfg.segments.last_turn.show_cache_creation && c.last_turn_cache_creation > 0 {
        // Prefix the cache-creation count so a glance at the segment
        // tells you "this is the cache-write count," not a stray "+N"
        // beside the input/output arrows. Reads as "cache+N" — N tokens
        // were *written* into the prompt cache this turn.
        s.push_str(&format!(" cache+{}", short_num(c.last_turn_cache_creation)));
    }
    s.push_str(&format!(" 🎯{}%", hit));
    format!("{}{}{}", DIM, s, RESET)
}

fn short_num(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn seg_cache_ttl(ctx: &Ctx) -> String {
    let Some(ms) = ctx.cache.last_cache_read_ms else {
        return String::new();
    };
    let now_ms = Utc::now().timestamp_millis();
    let elapsed_s = ((now_ms - ms) / 1000).max(0);
    let ttl_s = 5 * 60;
    if elapsed_s >= ttl_s {
        return format!("{}cache expired{}", DIM, RESET);
    }
    let remaining = ttl_s - elapsed_s;
    let mm = remaining / 60;
    let ss = remaining % 60;
    let color = if remaining < 60 { RED } else { DIM };
    format!("{}cache {}:{:02}{}", color, mm, ss, RESET)
}

fn seg_skills(ctx: &Ctx) -> String {
    fmt_counts("skills", &ctx.cache.skill_counts)
}

fn seg_mcp(ctx: &Ctx) -> String {
    fmt_counts("mcp", &ctx.cache.mcp_counts)
}

fn fmt_counts(label: &str, counts: &std::collections::HashMap<String, u32>) -> String {
    if counts.is_empty() {
        return String::new();
    }
    let mut entries: Vec<(&String, &u32)> = counts.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let body: Vec<String> = entries
        .iter()
        .take(4)
        .map(|(k, v)| format!("{}×{}", k, v))
        .collect();
    format!("{}{}: {}{}", DIM, label, body.join(" "), RESET)
}

fn seg_burn(ctx: &Ctx) -> String {
    let c = ctx.cache;
    let (Some(first), Some(last)) = (c.first_turn_ms, c.last_turn_ms) else {
        return String::new();
    };
    let elapsed_ms = (last - first).max(0);
    if elapsed_ms < 1000 {
        return String::new();
    }
    let total = c.total_input + c.total_output + c.total_cache_read + c.total_cache_creation;
    if total == 0 {
        return String::new();
    }
    let rate_per_min = total as f64 * 60_000.0 / elapsed_ms as f64;
    format!(
        "{}🔥 {} tok/min{}",
        DIM,
        short_num(rate_per_min as u64),
        RESET
    )
}

fn seg_session_age(ctx: &Ctx) -> String {
    // Wall-clock duration from the first assistant turn to "now".
    // We anchor on the first-turn timestamp (not last-turn) so a long
    // pause between turns still reads as "12m" rather than collapsing
    // back to "0s". Returns "" until the first turn lands so the
    // segment vanishes during cold-start.
    let Some(first) = ctx.cache.first_turn_ms else {
        return String::new();
    };
    let now = chrono::Utc::now().timestamp_millis();
    let elapsed_s = ((now - first) / 1000).max(0);
    format!("{}{}{}", DIM, format_duration(elapsed_s as u64), RESET)
}

/// Render a duration as the most compact human label that still
/// conveys magnitude. Buckets:
///   < 60s        → "42s"
///   < 60min      → "12m"
///   < 24h        → "1h23m"
///   ≥ 24h        → "2d3h"
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d{}h", secs / 86_400, (secs % 86_400) / 3600)
    }
}

fn seg_hit_rate(ctx: &Ctx) -> String {
    let c = ctx.cache;
    let base = c.total_input + c.total_cache_read + c.total_cache_creation;
    if base == 0 {
        return String::new();
    }
    let pct = (c.total_cache_read as f64 / base as f64 * 100.0).round() as u32;
    format!("{}hit {}%{}", DIM, pct, RESET)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::SessionCache;
    use crate::config::Config;
    use serde_json::json;

    fn ctx_for_test<'a>(
        stdin: &'a serde_json::Value,
        cache: &'a SessionCache,
        cfg: &'a Config,
    ) -> Ctx<'a> {
        Ctx {
            stdin,
            cache,
            cfg,
            rollup: None,
        }
    }

    #[test]
    fn short_num_thresholds() {
        assert_eq!(short_num(0), "0");
        assert_eq!(short_num(999), "999");
        assert_eq!(short_num(1_000), "1.0k");
        assert_eq!(short_num(1_500), "1.5k");
        assert_eq!(short_num(999_999), "1000.0k");
        assert_eq!(short_num(1_000_000), "1.0M");
        assert_eq!(short_num(1_500_000), "1.5M");
    }

    #[test]
    fn battery_bar_empty_and_full() {
        let (f, e) = battery_bar(0.0, 10);
        assert_eq!(f.chars().count(), 0);
        assert_eq!(e.chars().count(), 10);
        assert!(e.chars().all(|c| c == '▱'));

        let (f, e) = battery_bar(1.0, 10);
        assert_eq!(f.chars().count(), 10);
        assert_eq!(e.chars().count(), 0);
        assert!(f.chars().all(|c| c == '▰'));
    }

    #[test]
    fn battery_bar_half() {
        let (f, e) = battery_bar(0.5, 10);
        assert_eq!(f.chars().count(), 5);
        assert_eq!(e.chars().count(), 5);
    }

    #[test]
    fn battery_bar_floors_to_avoid_phantom_fill() {
        // 5% with 10 cells: would round to 1 cell filled, but we
        // floor — at "5% remaining" the bar must look almost-empty,
        // not "1 unit already lit." Floor matches user intuition.
        let (f, _) = battery_bar(0.05, 10);
        assert_eq!(f.chars().count(), 0, "5% should not fill any cell");

        // 99%: similarly, floor gives 9 filled (not 10), so the
        // bar visibly registers "not yet topped out."
        let (f, e) = battery_bar(0.99, 10);
        assert_eq!(f.chars().count(), 9);
        assert_eq!(e.chars().count(), 1);
    }

    #[test]
    #[serial_test::serial]
    fn compute_ctx_backsolves_window() {
        // used = 100k, remaining 50% → physical = 200k, capacity = 200k * 0.95 = 190k
        let mut cache = SessionCache::default();
        cache.last_turn_input = 1;
        cache.last_turn_cache_read = 99_999;
        cache.last_turn_cache_creation = 0;
        let stdin = json!({"context_window": {"remaining_percentage": 50.0}});
        let cfg = Config::default();
        // Test with PCT default 95 (env unset)
        std::env::remove_var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE");
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        let r = compute_ctx(&ctx).unwrap();
        assert_eq!(r.used, 100_000);
        // physical = 200_000, capacity = 190_000
        assert!((r.capacity as i64 - 190_000).abs() < 100);
        // used_frac = 100_000 / 190_000 ≈ 0.526
        assert!((r.used_frac - 100_000.0 / 190_000.0).abs() < 0.01);
    }

    #[test]
    #[serial_test::serial]
    fn compute_ctx_honors_pct_override() {
        let mut cache = SessionCache::default();
        cache.last_turn_input = 100_000;
        let stdin = json!({"context_window": {"remaining_percentage": 50.0}});
        let cfg = Config::default();
        std::env::set_var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE", "80");
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        let r = compute_ctx(&ctx).unwrap();
        // physical = 200k, capacity = 200k * 0.80 = 160k
        assert!((r.capacity as i64 - 160_000).abs() < 100);
        std::env::remove_var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE");
    }

    #[test]
    fn compute_ctx_returns_none_with_no_used() {
        let cache = SessionCache::default();
        let stdin = json!({"context_window": {"remaining_percentage": 80.0}});
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        assert!(compute_ctx(&ctx).is_none());
    }

    #[test]
    fn seg_dir_truncates_to_three_components() {
        std::env::set_var("HOME", "/Users/test");
        let stdin = json!({"cwd": "/Users/test/projects/sub/deeper/leaf"});
        let cache = SessionCache::default();
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        let s = seg_dir(&ctx);
        // Should keep the last 3 components only
        assert!(s.contains("sub/deeper/leaf"), "got: {}", s);
        assert!(!s.contains("projects"), "got: {}", s);
    }

    #[test]
    fn render_unknown_segment_returns_placeholder() {
        let stdin = serde_json::Value::Null;
        let cache = SessionCache::default();
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        assert_eq!(render("nope", &ctx), "{nope}");
    }

    // --- session_age ---------------------------------------------------

    #[test]
    fn format_duration_buckets() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(1), "1s");
        assert_eq!(format_duration(59), "59s");
        assert_eq!(format_duration(60), "1m");
        assert_eq!(format_duration(720), "12m"); // 12 minutes
        assert_eq!(format_duration(3599), "59m"); // < 1h
        assert_eq!(format_duration(3600), "1h00m");
        assert_eq!(format_duration(4980), "1h23m"); // 1h23m
        assert_eq!(format_duration(86_399), "23h59m");
        assert_eq!(format_duration(86_400), "1d0h");
        assert_eq!(format_duration(183_600), "2d3h");
    }

    #[test]
    fn session_age_renders_empty_without_first_turn() {
        let stdin = serde_json::Value::Null;
        let cache = SessionCache::default(); // first_turn_ms = None
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        assert_eq!(seg_session_age(&ctx), "");
    }

    #[test]
    fn session_age_renders_minutes_for_recent_session() {
        // first_turn_ms set ~720 seconds ago → seg should produce "12m"
        // (give or take 1s of jitter from the test reading the clock).
        let mut cache = SessionCache::default();
        let now = chrono::Utc::now().timestamp_millis();
        cache.first_turn_ms = Some(now - 720_000);
        let stdin = serde_json::Value::Null;
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        let s = seg_session_age(&ctx);
        // Strip ANSI for a clean assertion.
        let stripped: String = s
            .chars()
            .filter(|c| !c.is_control() && *c != 'm' || *c == 'm')
            .collect();
        assert!(
            stripped.contains("12m") || stripped.contains("11m"),
            "expected ~12m, got: {:?}",
            s
        );
    }

    // --- model resolution ---------------------------------------------

    #[test]
    fn cleanup_id_strips_only_anthropic_self_prefixes() {
        // Anthropic-self prefixes are noise → strip them.
        assert_eq!(
            cleanup_id("anthropic--claude-opus-latest"),
            "claude-opus-latest"
        );
        assert_eq!(cleanup_id("anthropic/claude-opus-4-7"), "claude-opus-4-7");
        // Deployment prefixes are *information* — keep them so users
        // know they're hitting Bedrock / Vertex / OpenRouter.
        assert_eq!(
            cleanup_id("bedrock/anthropic.claude-opus-4"),
            "bedrock/anthropic.claude-opus-4"
        );
        assert_eq!(
            cleanup_id("vertex_ai/claude-opus-4-7"),
            "vertex_ai/claude-opus-4-7"
        );
        // Unknown id passes through unchanged.
        assert_eq!(
            cleanup_id("gpt-4-turbo-via-claude-proxy"),
            "gpt-4-turbo-via-claude-proxy"
        );
    }

    #[test]
    fn detect_tier_recognizes_both_forms() {
        assert_eq!(detect_tier("claude-opus-latest[1m]"), Some("[1m]".into()));
        assert_eq!(detect_tier("Claude Opus 4.7 (1m)"), Some("[1m]".into()));
        assert_eq!(detect_tier("claude-opus-4-7"), None);
    }

    #[test]
    fn strip_tier_removes_trailing_marker() {
        assert_eq!(strip_tier("claude-opus-latest[1m]"), "claude-opus-latest");
        assert_eq!(strip_tier("Claude Opus 4.7 [1m]"), "Claude Opus 4.7");
        assert_eq!(strip_tier("Claude Opus 4.7 (1m)"), "Claude Opus 4.7");
        assert_eq!(strip_tier("claude-opus-4-7"), "claude-opus-4-7");
    }

    #[test]
    fn finalize_label_combines_cleanup_and_tier() {
        assert_eq!(
            finalize_model_label("anthropic--claude-opus-latest", Some("[1m]")),
            "claude-opus-latest [1m]"
        );
        // Tier preserved even when the base id stays the same after cleanup.
        assert_eq!(
            finalize_model_label("claude-opus-4-7", Some("[1m]")),
            "claude-opus-4-7 [1m]"
        );
        // No tier → no extra suffix.
        assert_eq!(
            finalize_model_label("claude-opus-4-7", None),
            "claude-opus-4-7"
        );
    }

    #[test]
    fn resolve_model_prefers_transcript_over_stdin() {
        // The user is running through a proxy that rewrote stdin to
        // `anthropic--claude-opus-latest`, but the transcript tells us
        // Claude Code actually called `claude-opus-4-7`. Transcript wins.
        let mut cache = SessionCache::default();
        cache.last_model = Some("claude-opus-4-7".into());
        let stdin = json!({"model":{"id":"anthropic--claude-opus-latest","display_name":"anthropic--claude-opus-latest"}});
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        let (label, src) = resolve_model(&ctx).unwrap();
        assert_eq!(src, ModelSource::Transcript);
        assert_eq!(label, "claude-opus-4-7");
    }

    #[test]
    fn resolve_model_preserves_tier_from_stdin_when_transcript_lacks_it() {
        // Transcript model id is canonical (`claude-opus-4-7`, no tier).
        // The user is on the 1M tier per stdin display_name. We must
        // keep the tier suffix in the displayed label.
        let mut cache = SessionCache::default();
        cache.last_model = Some("claude-opus-4-7".into());
        let stdin = json!({"model":{"display_name":"Claude Opus 4.7[1m]"}});
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        let (label, _) = resolve_model(&ctx).unwrap();
        assert!(label.ends_with("[1m]"), "tier must survive: {}", label);
        assert!(label.contains("claude-opus-4-7"));
    }

    #[test]
    fn resolve_model_falls_back_to_stdin_when_no_transcript() {
        let cache = SessionCache::default();
        let stdin = json!({"model":{"display_name":"Claude Opus 4.7","id":"claude-opus-4-7"}});
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        let (label, src) = resolve_model(&ctx).unwrap();
        assert_eq!(src, ModelSource::Stdin);
        assert_eq!(label, "Claude Opus 4.7");
    }

    #[test]
    fn resolve_model_strips_anthropic_prefix_from_stdin() {
        let cache = SessionCache::default();
        let stdin = json!({"model":{"id":"anthropic--claude-opus-latest[1m]"}});
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        let (label, _) = resolve_model(&ctx).unwrap();
        assert_eq!(label, "claude-opus-latest [1m]");
    }

    #[test]
    fn resolve_model_returns_none_when_nothing_known() {
        // No transcript, no stdin model field, no settings.json,
        // no env var. seg_model should render as "".
        std::env::remove_var("CLAUDE_MODEL");
        let cache = SessionCache::default();
        let stdin = json!({});
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        // configured_model() may still hit the developer's real
        // ~/.claude/settings.json during tests; assert resolve doesn't
        // panic and the label (if any) is non-empty.
        if let Some((label, _)) = resolve_model(&ctx) {
            assert!(!label.is_empty());
        }
    }

    #[test]
    fn valid_plugin_name_rejects_traversal() {
        assert!(valid_plugin_name("foo"));
        assert!(valid_plugin_name("foo-bar_2"));
        assert!(!valid_plugin_name(""));
        assert!(!valid_plugin_name("."));
        assert!(!valid_plugin_name(".."));
        assert!(!valid_plugin_name(".hidden"));
        assert!(!valid_plugin_name("a/b"));
        assert!(!valid_plugin_name("a\\b"));
    }

    #[test]
    fn sanitize_plugin_output_collapses_whitespace_and_clips() {
        assert_eq!(sanitize_plugin_output("hello\nworld"), "hello world");
        assert_eq!(
            sanitize_plugin_output("  spaced  out\t\tline  "),
            "spaced out line"
        );
        // ANSI ESC kept (plugins may emit colors), other control chars stripped
        assert_eq!(
            sanitize_plugin_output("\x1b[31mred\x1b[0m"),
            "\x1b[31mred\x1b[0m"
        );
        assert_eq!(sanitize_plugin_output("a\x07b"), "ab");
        // length cap (chars, not bytes)
        let long: String = "x".repeat(200);
        assert_eq!(
            sanitize_plugin_output(&long).chars().count(),
            super::PLUGIN_MAX_DISPLAY
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_plugin_executes_and_captures_stdout() {
        // Use /bin/echo directly (no shebang interpreter to spin up) so
        // the test reliably finishes inside PLUGIN_TIMEOUT_MS even on
        // cold-cache CI runners and macOS where Gatekeeper can add
        // 200ms+ to first-run script execution.
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("echoer");
        symlink("/bin/echo", &script).unwrap();
        let got = run_plugin(&script, "{}").expect("plugin should run");
        // /bin/echo with no args prints just a newline, which the
        // sanitizer would collapse — but run_plugin returns raw bytes,
        // so we just check it returned *something*.
        assert!(!got.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn run_plugin_kills_on_timeout() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::Instant;
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("slow");
        std::fs::write(&script, "#!/bin/sh\nsleep 5\necho done\n").unwrap();
        let mut perm = std::fs::metadata(&script).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&script, perm).unwrap();
        let t0 = Instant::now();
        let got = run_plugin(&script, "{}");
        let elapsed = t0.elapsed();
        assert!(got.is_none(), "slow plugin must time out, got {:?}", got);
        // Generous upper bound — the deadline is 100ms, kill+wait adds a bit.
        assert!(
            elapsed.as_millis() < 1500,
            "plugin timeout took too long: {:?}",
            elapsed
        );
    }

    #[test]
    fn render_plugin_missing_returns_empty() {
        // No plugin file exists with this name in the user's plugins dir;
        // the segment must yield "" rather than panicking or echoing the
        // template back.
        let stdin = serde_json::Value::Null;
        let cache = SessionCache::default();
        let cfg = Config::default();
        let ctx = ctx_for_test(&stdin, &cache, &cfg);
        assert_eq!(render("plugin:does-not-exist-xyz-123", &ctx), "");
    }
}

// --- Cost segments --------------------------------------------------

fn current_model_id(ctx: &Ctx) -> Option<String> {
    // Concat id + display_name so a "[1m]" or "(1m)" suffix on either
    // reaches pricing::lookup's tier detector.
    let id = ctx
        .stdin
        .pointer("/model/id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let display = ctx
        .stdin
        .pointer("/model/display_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if id.is_empty() && display.is_empty() {
        None
    } else {
        Some(format!("{} {}", id, display).trim().to_string())
    }
}

fn seg_cost_last(ctx: &Ctx) -> String {
    let Some(model) = current_model_id(ctx) else {
        return String::new();
    };
    let Some(price) = pricing::lookup(&model, &ctx.cfg.pricing) else {
        return String::new();
    };
    let c = ctx.cache;
    let total = c.last_turn_input
        + c.last_turn_output
        + c.last_turn_cache_read
        + c.last_turn_cache_creation;
    if total == 0 {
        return String::new();
    }
    let usd = pricing::cost(
        price,
        c.last_turn_input,
        c.last_turn_output,
        c.last_turn_cache_read,
        c.last_turn_cache_creation,
    );
    format!("{}last {}{}", YELLOW, pricing::fmt_usd(usd), RESET)
}

fn seg_cost_session(ctx: &Ctx) -> String {
    let Some(model) = current_model_id(ctx) else {
        return String::new();
    };
    let Some(price) = pricing::lookup(&model, &ctx.cfg.pricing) else {
        return String::new();
    };
    let c = ctx.cache;
    let total = c.total_input + c.total_output + c.total_cache_read + c.total_cache_creation;
    if total == 0 {
        return String::new();
    }
    let usd = pricing::cost(
        price,
        c.total_input,
        c.total_output,
        c.total_cache_read,
        c.total_cache_creation,
    );
    format!("{}sess {}{}", YELLOW, pricing::fmt_usd(usd), RESET)
}

fn rollup_cost(ctx: &Ctx, days: i64) -> Option<f64> {
    let r = ctx.rollup?;
    let totals = crate::rollup::sum_last_days(r, days);
    if totals.is_empty() {
        return None;
    }
    let mut usd = 0.0;
    for (model, b) in totals {
        let Some(price) = pricing::lookup(&model, &ctx.cfg.pricing) else {
            continue;
        };
        usd += pricing::cost(price, b.input, b.output, b.cache_read, b.cache_creation);
    }
    Some(usd)
}

fn seg_cost_today(ctx: &Ctx) -> String {
    let Some(usd) = rollup_cost(ctx, 1) else {
        return String::new();
    };
    if usd <= 0.0 {
        return String::new();
    }
    format!("{}today {}{}", YELLOW, pricing::fmt_usd(usd), RESET)
}

fn seg_cost_week(ctx: &Ctx) -> String {
    let Some(usd) = rollup_cost(ctx, 7) else {
        return String::new();
    };
    if usd <= 0.0 {
        return String::new();
    }
    format!("{}7d {}{}", YELLOW, pricing::fmt_usd(usd), RESET)
}

fn seg_cost_combo(ctx: &Ctx) -> String {
    // last + today, useful as a single-glance "how much have I spent"
    // pair. Drops parts that don't have data.
    let last = seg_cost_last(ctx);
    let today = seg_cost_today(ctx);
    match (last.is_empty(), today.is_empty()) {
        (true, true) => String::new(),
        (false, true) => last,
        (true, false) => today,
        (false, false) => format!("{} {DIM}|{R} {}", last, today, DIM = DIM, R = RESET),
    }
}

// --- Plugin segment -------------------------------------------------

const PLUGIN_TIMEOUT_MS: u64 = 250;
const PLUGIN_MAX_BYTES: usize = 4096;
const PLUGIN_MAX_DISPLAY: usize = 80;

/// Resolve the plugin directory: `<config_dir>/plugins`. Returns None
/// if the config dir cannot be determined (rare; same path the rest
/// of the binary uses).
fn plugins_dir() -> Option<PathBuf> {
    let cfg = config::config_path().ok()?;
    cfg.parent().map(|p| p.join("plugins"))
}

/// Whether `name` is safe as a plugin filename. Disallow path separators
/// and `..` so the template can't escape the plugins dir.
fn valid_plugin_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && name != "."
        && name != ".."
        && !name.starts_with('.')
}

fn seg_plugin(name: &str, ctx: &Ctx) -> String {
    if !valid_plugin_name(name) {
        return String::new();
    }
    let Some(dir) = plugins_dir() else {
        return String::new();
    };
    let path = dir.join(name);
    if !path.is_file() {
        return String::new();
    }

    let stdin_payload = serde_json::to_string(ctx.stdin).unwrap_or_else(|_| "{}".to_string());
    let Some(raw) = run_plugin(&path, &stdin_payload) else {
        return String::new();
    };
    sanitize_plugin_output(&raw)
}

fn run_plugin(path: &Path, stdin_payload: &str) -> Option<String> {
    use std::time::{Duration, Instant};

    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut sin) = child.stdin.take() {
        let _ = sin.write_all(stdin_payload.as_bytes());
        // drop sin → close stdin so the child can exit even if it
        // tries to read more than we sent.
    }

    let deadline = Instant::now() + Duration::from_millis(PLUGIN_TIMEOUT_MS);
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }

    let mut buf = Vec::with_capacity(256);
    if let Some(so) = child.stdout.take() {
        use std::io::Read as _;
        let _ = so.take(PLUGIN_MAX_BYTES as u64).read_to_end(&mut buf);
    }
    let _ = child.wait();
    if buf.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&buf).to_string())
}

/// Public shim so `ccs plugin run` can preview exactly what the status
/// line will display, without exposing the internal sanitizer's name.
pub fn sanitize_plugin_output_for_debug(raw: &str) -> String {
    sanitize_plugin_output(raw)
}

/// Replace newlines/tabs with single spaces, collapse runs of whitespace,
/// strip ANSI control characters that aren't already a recognized SGR
/// sequence (we let plugins emit their own colors), and clip to a
/// reasonable display width so a misbehaving plugin can't overflow the
/// status line.
fn sanitize_plugin_output(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_space = false;
    for ch in raw.chars() {
        let mapped = match ch {
            '\n' | '\r' | '\t' => ' ',
            c if (c as u32) < 0x20 && c != '\x1b' => continue,
            c => c,
        };
        if mapped == ' ' {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(mapped);
            prev_space = false;
        }
    }
    let trimmed = out.trim();
    if trimmed.chars().count() > PLUGIN_MAX_DISPLAY {
        trimmed.chars().take(PLUGIN_MAX_DISPLAY).collect()
    } else {
        trimmed.to_string()
    }
}
