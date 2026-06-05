//! `ccs status` — print a multi-line, self-explanatory dashboard for the
//! current session. Reads the same JSON-on-stdin payload that Claude Code
//! sends to `ccs render`.

use crate::{cache, config, transcript};
use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use std::io::Read;
use std::path::PathBuf;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[0;31m";
const YELLOW: &str = "\x1b[0;33m";
const GREEN: &str = "\x1b[0;32m";

pub fn run() -> Result<()> {
    let mut buf = String::new();
    // stdin is optional: CC's statusline pipes JSON, but a user invoking
    // `ccs status` from a slash command typically doesn't.
    let _ = std::io::stdin().read_to_string(&mut buf);
    let stdin: Value = if buf.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&buf).unwrap_or(Value::Null)
    };

    let cfg = config::load()?;
    let session_id = stdin
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    // Resolve transcript: prefer the path CC handed us; otherwise fall
    // back to "newest jsonl under ~/.claude/projects/<sanitized cwd>".
    let cwd_for_lookup = stdin
        .pointer("/cwd")
        .or_else(|| stdin.pointer("/workspace/current_dir"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .and_then(|p| p.to_str().map(String::from))
        });

    let transcript_path: Option<PathBuf> = stdin
        .get("transcript_path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .or_else(|| {
            cwd_for_lookup
                .as_deref()
                .and_then(transcript::find_latest_for_cwd)
        });

    let mut sess = cache::load(&session_id);
    if let Some(path) = &transcript_path {
        if path.exists() {
            let _ = transcript::update(path, &mut sess);
            let _ = cache::save(&session_id, &sess);
        }
    }

    let cwd = stdin
        .pointer("/cwd")
        .or_else(|| stdin.pointer("/workspace/current_dir"))
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown)");
    let model = stdin
        .pointer("/model/display_name")
        .or_else(|| stdin.pointer("/model/id"))
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown)");
    let ctx_pct = stdin
        .pointer("/context_window/remaining_percentage")
        .and_then(|v| v.as_f64());

    println!("{B}┌─ cc-status · current session{R}", B = BOLD, R = RESET);

    // --- environment ---
    println!("{B}│ Environment{R}", B = BOLD, R = RESET);
    println!("│   cwd          {}", cwd);
    println!("│   model        {}", model);
    println!("│   session id   {}{}{}", DIM, session_id, RESET);
    println!("│   mode         {}", cfg.current_mode);

    // --- context ---
    println!("{B}│ Context window{R}", B = BOLD, R = RESET);
    let used = sess.last_turn_input + sess.last_turn_cache_read + sess.last_turn_cache_creation;
    if let (Some(p), true) = (
        ctx_pct,
        used > 0 && ctx_pct.map_or(false, |p| p < 100.0 && p >= 0.0),
    ) {
        let physical = (used as f64) / (1.0 - p / 100.0);
        let pct_override: f64 = std::env::var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(95.0)
            .clamp(1.0, 100.0);
        let capacity = (physical * pct_override / 100.0) as u64;
        let used_pct =
            (used as f64 / (physical * pct_override / 100.0) * 100.0).clamp(0.0, 100.0) as i64;
        let remaining_pct = (100 - used_pct).max(0);

        let color = if (remaining_pct as u8) < cfg.theme.ctx_low {
            RED
        } else if (remaining_pct as u8) < cfg.theme.ctx_med {
            YELLOW
        } else {
            GREEN
        };
        let label = if (remaining_pct as u8) < cfg.theme.ctx_low {
            "danger — clean up or start a fresh session"
        } else if (remaining_pct as u8) < cfg.theme.ctx_med {
            "warning — watch the long context"
        } else {
            "healthy"
        };
        println!(
            "│   used / cap    {} / {} tokens  ({}{}%{} remaining)",
            short_num(used),
            short_num(capacity),
            color,
            remaining_pct,
            RESET
        );
        println!("│   status       {}{}{}", color, label, RESET);
        println!(
            "│   {}physical window {}, auto-compact at {}% (CLAUDE_AUTOCOMPACT_PCT_OVERRIDE){}",
            DIM,
            short_num(physical.round() as u64),
            pct_override.round() as i64,
            RESET
        );
        let _ = p;
    } else if let Some(p) = ctx_pct {
        let pct_i = p.round() as i64;
        println!(
            "│   CC reports   {}% remaining  {}(no last-turn token data, can't backsolve capacity){}",
            pct_i, DIM, RESET
        );
    } else {
        println!("│   remaining    {}(no data){}", DIM, RESET);
    }

    // --- last turn ---
    println!(
        "{B}│ Last turn (most recent assistant reply){R}",
        B = BOLD,
        R = RESET
    );
    let lt_total = sess.last_turn_input
        + sess.last_turn_output
        + sess.last_turn_cache_read
        + sess.last_turn_cache_creation;
    if lt_total == 0 {
        println!("│   {}no data yet (session just started?){}", DIM, RESET);
    } else {
        let total_in =
            sess.last_turn_input + sess.last_turn_cache_read + sess.last_turn_cache_creation;
        let hit_base = total_in;
        let hit = if hit_base > 0 {
            (sess.last_turn_cache_read as f64 / hit_base as f64 * 100.0).round() as u32
        } else {
            0
        };
        println!(
            "│   sent input   {} tokens (incl. cache hits)",
            short_num(total_in)
        );
        println!(
            "│     ├ cache hit       {} {}(0.1× discount){}",
            short_num(sess.last_turn_cache_read),
            DIM,
            RESET
        );
        println!(
            "│     ├ cache write     {} {}(1.25× premium, hit-able next turn){}",
            short_num(sess.last_turn_cache_creation),
            DIM,
            RESET
        );
        println!(
            "│     └ fresh input     {} {}(normal input price){}",
            short_num(sess.last_turn_input),
            DIM,
            RESET
        );
        println!(
            "│   model output {} tokens",
            short_num(sess.last_turn_output)
        );
        println!("│   hit rate     {}{}%{}", color_for_hit(hit), hit, RESET);
    }

    // --- prompt cache ---
    println!("{B}│ Prompt Cache (5-min TTL){R}", B = BOLD, R = RESET);
    if let Some(ms) = sess.last_cache_read_ms {
        let now_ms = Utc::now().timestamp_millis();
        let elapsed_s = ((now_ms - ms) / 1000).max(0);
        let ttl_s = 5 * 60;
        if elapsed_s >= ttl_s {
            println!(
                "│   status       {}expired — next turn will rebuild the cache{}",
                DIM, RESET
            );
        } else {
            let remaining = ttl_s - elapsed_s;
            let mm = remaining / 60;
            let ss = remaining % 60;
            let color = if remaining < 60 { RED } else { GREEN };
            let hint = if remaining < 60 {
                "send your next message before it expires"
            } else {
                "cache is fresh"
            };
            println!(
                "│   remaining    {}{}:{:02}{}  {}{}{}",
                color, mm, ss, RESET, DIM, hint, RESET
            );
        }
    } else {
        println!(
            "│   status       {}cache has never been hit this session{}",
            DIM, RESET
        );
    }

    // --- session cumulative ---
    println!("{B}│ Session totals{R}", B = BOLD, R = RESET);
    let tot_base = sess.total_input + sess.total_cache_read + sess.total_cache_creation;
    if sess.total_input + sess.total_output + sess.total_cache_read + sess.total_cache_creation == 0
    {
        println!("│   {}no data yet{}", DIM, RESET);
    } else {
        let hit = if tot_base > 0 {
            (sess.total_cache_read as f64 / tot_base as f64 * 100.0).round() as u32
        } else {
            0
        };
        println!(
            "│   total input    {} tokens",
            short_num(sess.total_input + sess.total_cache_read + sess.total_cache_creation)
        );
        println!("│   total output   {} tokens", short_num(sess.total_output));
        println!("│   overall hit    {}{}%{}", color_for_hit(hit), hit, RESET);
        if let (Some(first), Some(last)) = (sess.first_turn_ms, sess.last_turn_ms) {
            let elapsed_ms = (last - first).max(0);
            if elapsed_ms >= 1000 {
                let total = sess.total_input
                    + sess.total_output
                    + sess.total_cache_read
                    + sess.total_cache_creation;
                let rate = total as f64 * 60_000.0 / elapsed_ms as f64;
                let mins = elapsed_ms / 60_000;
                let secs = (elapsed_ms / 1000) % 60;
                println!("│   duration       {}m{:02}s", mins, secs);
                println!("│   avg rate       {} tokens/min", short_num(rate as u64));
            }
        }
    }

    // --- tool calls ---
    println!("{B}│ Tool calls{R}", B = BOLD, R = RESET);
    if sess.skill_counts.is_empty() && sess.mcp_counts.is_empty() {
        println!("│   {}no Skill / MCP calls in this session{}", DIM, RESET);
    } else {
        if sess.skill_counts.is_empty() {
            println!("│   Skills    {}—{}", DIM, RESET);
        } else {
            println!("│   Skills    {}", fmt_counts_inline(&sess.skill_counts));
        }
        if sess.mcp_counts.is_empty() {
            println!("│   MCP       {}—{}", DIM, RESET);
        } else {
            println!("│   MCP       {}", fmt_counts_inline(&sess.mcp_counts));
        }
    }

    println!("{B}└─{R}", B = BOLD, R = RESET);
    println!(
        "{}Run `ccs explain` for the status-line legend.{}",
        DIM, RESET
    );
    Ok(())
}

fn color_for_hit(hit: u32) -> &'static str {
    if hit >= 70 {
        GREEN
    } else if hit >= 30 {
        YELLOW
    } else {
        RED
    }
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

fn fmt_counts_inline(counts: &std::collections::HashMap<String, u32>) -> String {
    let mut entries: Vec<(&String, &u32)> = counts.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    entries
        .iter()
        .map(|(k, v)| format!("{}×{}", k, v))
        .collect::<Vec<_>>()
        .join("  ")
}
