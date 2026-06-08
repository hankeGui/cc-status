use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SessionCache {
    pub file_offset: u64,
    pub inode: u64,
    pub last_turn_input: u64,
    pub last_turn_output: u64,
    pub last_turn_cache_read: u64,
    pub last_turn_cache_creation: u64,
    pub last_cache_read_ms: Option<i64>,
    #[serde(default)]
    pub skill_counts: HashMap<String, u32>,
    #[serde(default)]
    pub mcp_counts: HashMap<String, u32>,
    #[serde(default)]
    pub total_input: u64,
    #[serde(default)]
    pub total_output: u64,
    #[serde(default)]
    pub total_cache_read: u64,
    #[serde(default)]
    pub total_cache_creation: u64,
    #[serde(default)]
    pub first_turn_ms: Option<i64>,
    #[serde(default)]
    pub last_turn_ms: Option<i64>,
    /// Per-message dedupe (key = "message_id|requestId"). Value is the
    /// largest token totals we've credited so far. Mirrors ccusage's
    /// streaming-partial → streaming-final replacement strategy so we
    /// don't double-count the same assistant message.
    #[serde(default)]
    pub seen: HashMap<String, SeenMessage>,
    /// Per-tool-call dedupe by tool_use `id` (toolu_*). When the same
    /// assistant message is replayed across multiple JSONL entries
    /// (sidechain or streaming), the same tool_use block can show up
    /// more than once; without this set we'd over-count Skill / MCP
    /// calls. Token totals use a different dedupe path; see `seen`.
    #[serde(default)]
    pub seen_tools: HashSet<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SeenMessage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
}

fn cache_dir() -> Result<PathBuf> {
    let dirs =
        ProjectDirs::from("dev", "hanke", "cc-status").context("cannot determine cache dir")?;
    Ok(dirs.cache_dir().to_path_buf())
}

fn cache_file(session_id: &str) -> Result<PathBuf> {
    Ok(cache_dir()?.join(format!("session-{}.json", sanitize(session_id))))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn load(session_id: &str) -> SessionCache {
    let Ok(path) = cache_file(session_id) else {
        return SessionCache::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return SessionCache::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(session_id: &str, c: &SessionCache) -> Result<()> {
    let path = cache_file(session_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string(c)?)?;
    Ok(())
}
