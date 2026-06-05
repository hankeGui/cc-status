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
        description: "Current directory (last 3 path components, ~ for HOME)",
    },
    SegmentInfo {
        name: "git",
        example: "wt:foo main ⇡2⇣1 [+!?]",
        description: "Git worktree / branch / ahead-behind / + staged ! modified ? untracked",
    },
    SegmentInfo {
        name: "model",
        example: "Claude Opus 4.7",
        description: "Model name reported by Claude Code",
    },
    SegmentInfo {
        name: "ctx",
        example: "ctx 86% █████▏ 154.6k/950k",
        description: "Context remaining % + bar + used/capacity",
    },
    SegmentInfo {
        name: "ctx_tokens",
        example: "154.6k/950k",
        description: "Token numbers only (used/capacity), no percent or bar",
    },
    SegmentInfo {
        name: "last_turn",
        example: "↑12.3k ↓2.1k +865 🎯89%",
        description: "Last turn: input↑ output↓ cache write+ hit rate🎯",
    },
    SegmentInfo {
        name: "cache_ttl",
        example: "cache 3:42",
        description: "Prompt-cache 5-min TTL countdown (red < 1 min / cache expired)",
    },
    SegmentInfo {
        name: "hit_rate",
        example: "hit 96%",
        description: "Session-wide cumulative cache hit rate",
    },
    SegmentInfo {
        name: "burn",
        example: "🔥 32.4k/min",
        description: "Session-average token rate",
    },
    SegmentInfo {
        name: "skills",
        example: "skills: jira×3 wiki×1",
        description: "Skill calls (top 4, sorted by count)",
    },
    SegmentInfo {
        name: "mcp",
        example: "mcp: github×2",
        description: "MCP-server calls (aggregated by server name)",
    },
    SegmentInfo {
        name: "cost_last",
        example: "last $0.012",
        description: "Cost of the last assistant turn (USD, current model)",
    },
    SegmentInfo {
        name: "cost_session",
        example: "sess $1.42",
        description: "Cumulative cost of the current session",
    },
    SegmentInfo {
        name: "cost_today",
        example: "today $4.18",
        description: "Cost across all sessions today (UTC), all models",
    },
    SegmentInfo {
        name: "cost_week",
        example: "7d $24.50",
        description: "Cost over the last 7 days, all models",
    },
    SegmentInfo {
        name: "cost",
        example: "last $0.012 · today $4.18",
        description: "Combo: cost_last + cost_today",
    },
    SegmentInfo {
        name: "mode",
        example: "[detailed]",
        description: "Current mode label (dim)",
    },
];

pub fn is_known(name: &str) -> bool {
    SEGMENTS.iter().any(|s| s.name == name)
}

pub fn names() -> Vec<&'static str> {
    SEGMENTS.iter().map(|s| s.name).collect()
}
