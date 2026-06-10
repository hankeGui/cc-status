//! Single source of truth for the list of segments and their metadata.
//! Used by `ccs segments` (the listing command) and as a registry for
//! validating segment names in user-defined modes.

pub struct SegmentInfo {
    pub name: &'static str,
    pub example: &'static str,
    pub description: &'static str,
}

pub const SEGMENTS: &[SegmentInfo] = &[
    SegmentInfo {
        name: "dir",
        example: "~/hanke-dev/cc-status",
        description: "Current working directory (last 3 path segments; HOME shown as `~`).",
    },
    SegmentInfo {
        name: "git",
        example: "wt:foo main ⇡2⇣1 [+!?]",
        description: "Git status: worktree name (when in a worktree), branch, ahead/behind upstream (⇡ ahead, ⇣ behind), and dirty flags — `+` staged, `!` modified, `?` untracked.",
    },
    SegmentInfo {
        name: "model",
        example: "claude-opus-4-7 [1m]",
        description: "Model id Claude Code is actually invoking (resolved from transcript first, then stdin, then settings.json). `[1m]` suffix flags the 1M-context tier.",
    },
    SegmentInfo {
        name: "ctx",
        example: "ctx 54% ▰▰▰▰▰▱▱▱▱▱ 504.1k/1.0M",
        description: "Context window: percent remaining before auto-compact + battery bar (▰ remaining, ▱ used) + tokens used / capacity. Color = green ≥50%, yellow ≥20%, red <20%.",
    },
    SegmentInfo {
        name: "ctx_tokens",
        example: "ctx-used 504.1k/1.0M",
        description: "Token numbers only (used / capacity), no percent or bar. Use this when `{ctx}` is too wide.",
    },
    SegmentInfo {
        name: "last_turn",
        example: "↑504.1k ↓618 cache+581 🎯100%",
        description: "Most recent assistant turn: ↑ input sent (incl. cache hits + writes), ↓ output, `cache+N` tokens written into the prompt cache (toggleable), 🎯 cache-hit rate of *this turn alone*.",
    },
    SegmentInfo {
        name: "cache_ttl",
        example: "cache 4:42",
        description: "Time left on the prompt cache's 5-minute TTL since the last cache_read. Red when <1 min / expired — this is when your next turn will pay full input price instead of cache_read price.",
    },
    SegmentInfo {
        name: "hit_rate",
        example: "hit 96%",
        description: "Session-wide cumulative cache hit rate (all turns aggregated). Different from `last_turn`'s 🎯 which is just the most recent turn.",
    },
    SegmentInfo {
        name: "burn",
        example: "🔥 54.3k tok/min",
        description: "Session-average token throughput (total tokens / minutes elapsed since first turn). Useful as a sanity-check rate.",
    },
    SegmentInfo {
        name: "session_age",
        example: "1h23m",
        description: "Wall-clock duration since the first assistant turn — `42s` / `12m` / `1h23m` / `2d3h`.",
    },
    SegmentInfo {
        name: "skills",
        example: "skills: jira×3 wiki×1",
        description: "Top-4 Claude Code Skill invocations this session (skill name × count, sorted by count). Hidden when no skill has been called.",
    },
    SegmentInfo {
        name: "mcp",
        example: "mcp: github×2",
        description: "MCP-server tool calls this session, aggregated by server name. Hidden when no MCP tool has been called.",
    },
    SegmentInfo {
        name: "cost_last",
        example: "last $0.012",
        description: "USD cost of the most recent assistant turn (current model's prices, including cache discounts).",
    },
    SegmentInfo {
        name: "cost_session",
        example: "sess $1.42",
        description: "Cumulative USD cost of the current session.",
    },
    SegmentInfo {
        name: "cost_today",
        example: "today $4.18",
        description: "Cost across all sessions today (local-time day buckets), aggregated over every model used.",
    },
    SegmentInfo {
        name: "cost_week",
        example: "7d $24.50",
        description: "Cost over the last 7 days (rolling), all models.",
    },
    SegmentInfo {
        name: "cost",
        example: "last $0.012 | today $4.18",
        description: "Combo of `cost_last` and `cost_today` — the two costs you most often want to see together.",
    },
    SegmentInfo {
        name: "mode",
        example: "[balanced]",
        description: "Current display-mode name in brackets — handy when experimenting with multiple modes.",
    },
    SegmentInfo {
        name: "plugin:NAME",
        example: "{plugin:weather}",
        description: "Run `<config>/plugins/NAME` and use its stdout as the segment value. 250ms hard timeout, 80-char output cap, ANSI SGR allowed. Use `ccs plugin new` to scaffold.",
    },
];

pub fn is_known(name: &str) -> bool {
    // Dynamic plugin segments: `{plugin:NAME}` resolves at render time
    // by execing `<config>/plugins/NAME`. We can't validate NAME against
    // a static list, so accept any non-empty name here.
    if let Some(rest) = name.strip_prefix("plugin:") {
        return !rest.is_empty();
    }
    SEGMENTS.iter().any(|s| s.name == name)
}

pub fn names() -> Vec<&'static str> {
    SEGMENTS.iter().map(|s| s.name).collect()
}
