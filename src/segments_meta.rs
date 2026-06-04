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
        description: "当前目录（最后 3 段路径，~ 代表 HOME）",
    },
    SegmentInfo {
        name: "git",
        example: "wt:foo main ⇡2⇣1 [+!?]",
        description: "git worktree / 分支 / 领先落后 / + 暂存 ! 修改 ? 未跟踪",
    },
    SegmentInfo {
        name: "model",
        example: "Claude Opus 4.7",
        description: "当前对话的模型名",
    },
    SegmentInfo {
        name: "ctx",
        example: "ctx 86% █████▏ 154.6k/950k",
        description: "context 剩余 % + 进度条 + 已用/可用容量",
    },
    SegmentInfo {
        name: "ctx_tokens",
        example: "154.6k/950k",
        description: "只显示 token 数（已用/可用），不带百分比和进度条",
    },
    SegmentInfo {
        name: "last_turn",
        example: "↑12.3k ↓2.1k +865 🎯89%",
        description: "上一轮 input↑ output↓ 写入cache+ 命中率🎯",
    },
    SegmentInfo {
        name: "cache_ttl",
        example: "cache 3:42",
        description: "prompt cache 5min TTL 倒计时（红色 < 1min / cache expired）",
    },
    SegmentInfo {
        name: "hit_rate",
        example: "hit 96%",
        description: "整会话累计 cache 命中率",
    },
    SegmentInfo {
        name: "burn",
        example: "🔥 32.4k/min",
        description: "会话平均 token 速率",
    },
    SegmentInfo {
        name: "skills",
        example: "skills: jira×3 wiki×1",
        description: "Skill 调用次数（按次数倒序，最多 4 个）",
    },
    SegmentInfo {
        name: "mcp",
        example: "mcp: github×2",
        description: "MCP 服务器调用次数（按服务器聚合）",
    },
    SegmentInfo {
        name: "mode",
        example: "[detailed]",
        description: "当前模式名（暗色）",
    },
];

pub fn is_known(name: &str) -> bool {
    SEGMENTS.iter().any(|s| s.name == name)
}

pub fn names() -> Vec<&'static str> {
    SEGMENTS.iter().map(|s| s.name).collect()
}
