use crate::cache::SessionCache;
use crate::config::Config;
use chrono::Utc;
use serde_json::Value;
use std::path::Path;
use std::process::Command;

const RESET: &str = "\x1b[0m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_MAGENTA: &str = "\x1b[1;35m";
const RED: &str = "\x1b[0;31m";
const YELLOW: &str = "\x1b[0;33m";
const GREEN: &str = "\x1b[0;32m";
const DIM: &str = "\x1b[2m";

pub struct Ctx<'a> {
    pub stdin: &'a Value,
    pub cache: &'a SessionCache,
    pub cfg: &'a Config,
}

pub fn render(name: &str, ctx: &Ctx) -> String {
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
        "hit_rate" => seg_hit_rate(ctx),
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
    if let Some(counts) = run_git(cwd, &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]) {
        let mut it = counts.split_whitespace();
        let ahead: u32 = it.next().unwrap_or("0").parse().unwrap_or(0);
        let behind: u32 = it.next().unwrap_or("0").parse().unwrap_or(0);
        let mut ab = String::new();
        if ahead > 0 { ab.push_str(&format!("⇡{}", ahead)); }
        if behind > 0 { ab.push_str(&format!("⇣{}", behind)); }
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
            if line.starts_with("??") { untracked = true; }
            else if line.starts_with(" M") || line.starts_with("M ") { modified = true; }
            else if line.chars().next().map_or(false, |c| "MARCDU".contains(c)) { staged = true; }
        }
        let mut flags = String::new();
        if staged { flags.push('+'); }
        if modified { flags.push('!'); }
        if untracked { flags.push('?'); }
        if !flags.is_empty() {
            parts.push(format!("{}[{}]{}", RED, flags, RESET));
        }
    }

    parts.join(" ")
}

fn run_git(cwd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn seg_model(ctx: &Ctx) -> String {
    let name = ctx
        .stdin
        .pointer("/model/display_name")
        .or_else(|| ctx.stdin.pointer("/model/id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if name.is_empty() {
        return String::new();
    }
    format!("{}{}{}", DIM, name, RESET)
}

fn seg_ctx(ctx: &Ctx) -> String {
    let Some(c) = compute_ctx(ctx) else { return String::new() };
    // remaining_frac = 距离 compact 还剩多少（0..1）
    let remaining_frac = (1.0 - c.used_frac).clamp(0.0, 1.0);
    let pct_i = (remaining_frac * 100.0).round() as i64;
    let color = if (pct_i as u8) < ctx.cfg.theme.ctx_low {
        RED
    } else if (pct_i as u8) < ctx.cfg.theme.ctx_med {
        YELLOW
    } else {
        GREEN
    };
    let bar = progress_bar(remaining_frac, 6);
    format!(
        "{}ctx {}% {} {}{}/{}{}",
        color,
        pct_i,
        bar,
        DIM,
        short_num(c.used),
        short_num(c.capacity),
        RESET
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

    Some(CtxCalc { used, capacity, used_frac })
}

fn seg_ctx_tokens(ctx: &Ctx) -> String {
    let Some(c) = compute_ctx(ctx) else { return String::new() };
    format!("{}{}/{}{}", DIM, short_num(c.used), short_num(c.capacity), RESET)
}

fn progress_bar(frac: f64, width: usize) -> String {
    let blocks = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
    let total_eighths = (frac.clamp(0.0, 1.0) * width as f64 * 8.0).round() as usize;
    let full = total_eighths / 8;
    let rem = total_eighths % 8;
    let mut s = String::new();
    for _ in 0..full {
        s.push('█');
    }
    if full < width && rem > 0 {
        s.push(blocks[rem - 1]);
    }
    while s.chars().count() < width {
        s.push(' ');
    }
    s
}

fn seg_last_turn(ctx: &Ctx) -> String {
    let c = ctx.cache;
    let total = c.last_turn_input + c.last_turn_output + c.last_turn_cache_read + c.last_turn_cache_creation;
    if total == 0 {
        return String::new();
    }
    let hit_base = c.last_turn_input + c.last_turn_cache_read + c.last_turn_cache_creation;
    let hit = if hit_base > 0 {
        (c.last_turn_cache_read as f64 / hit_base as f64 * 100.0).round() as u32
    } else {
        0
    };
    let mut s = format!("↑{} ↓{}", short_num(c.last_turn_input + c.last_turn_cache_read + c.last_turn_cache_creation), short_num(c.last_turn_output));
    if ctx.cfg.segments.last_turn.show_cache_creation && c.last_turn_cache_creation > 0 {
        s.push_str(&format!(" +{}", short_num(c.last_turn_cache_creation)));
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
    let Some(ms) = ctx.cache.last_cache_read_ms else { return String::new() };
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
    format!("{}🔥 {}/min{}", DIM, short_num(rate_per_min as u64), RESET)
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
