use crate::cache::SessionCache;
use anyhow::Result;
use chrono::DateTime;
use serde_json::Value;
use std::fs::{File, Metadata};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Get a stable per-file identifier for rotation detection. On Unix this
/// is the inode; on Windows it's the NT file index. On unknown platforms
/// we fall back to a hash of (modified time, len) — good enough to
/// detect "the file got replaced" in practice.
fn file_id(meta: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return meta.ino();
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        return meta.file_index().unwrap_or(0);
    }
    #[cfg(not(any(unix, windows)))]
    {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        if let Ok(t) = meta.modified() {
            if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                d.as_nanos().hash(&mut h);
            }
        }
        meta.len().hash(&mut h);
        h.finish()
    }
}

/// Locate the most recently modified transcript JSONL for `cwd`.
///
/// Claude Code stores transcripts at
/// `~/.claude/projects/<sanitized-cwd>/<session-id>.jsonl`, where the cwd
/// has every non-alphanumeric char turned into '-'. We pick the most
/// recently modified file as the "current" session — works as a fallback
/// when CC didn't pass `transcript_path` on stdin (e.g. when invoked
/// from a slash command).
pub fn find_latest_for_cwd(cwd: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let sanitized: String = cwd
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let dir = PathBuf::from(home).join(".claude/projects").join(sanitized);
    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        if newest.as_ref().map_or(true, |(_, t)| mtime > *t) {
            newest = Some((path, mtime));
        }
    }
    newest.map(|(p, _)| p)
}

/// Update `cache` by reading new lines appended to the transcript at `path`.
///
/// Claude Code transcripts are JSONL. Each assistant message contains a
/// `message.usage` object with input/output/cache token counts. Tool-use
/// blocks (`type: "tool_use"`) tell us which Skill/MCP was invoked.
pub fn update(path: &Path, cache: &mut SessionCache) -> Result<()> {
    let Ok(meta) = std::fs::metadata(path) else { return Ok(()) };
    let inode = file_id(&meta);
    let size = meta.len();

    if inode != cache.inode || size < cache.file_offset {
        cache.inode = inode;
        cache.file_offset = 0;
        cache.skill_counts.clear();
        cache.mcp_counts.clear();
        cache.total_input = 0;
        cache.total_output = 0;
        cache.total_cache_read = 0;
        cache.total_cache_creation = 0;
        cache.first_turn_ms = None;
        cache.last_turn_ms = None;
    }
    if size == cache.file_offset {
        return Ok(());
    }

    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(cache.file_offset))?;
    let reader = BufReader::new(&f);

    let mut bytes_consumed: u64 = 0;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        bytes_consumed += line.len() as u64 + 1;
        if line.is_empty() {
            continue;
        }
        let Ok(v): Result<Value, _> = serde_json::from_str(&line) else { continue };
        process_entry(&v, cache);
    }
    cache.file_offset += bytes_consumed;
    Ok(())
}

fn process_entry(v: &Value, cache: &mut SessionCache) {
    let entry_type = v.get("type").and_then(|x| x.as_str()).unwrap_or("");

    if entry_type == "assistant" {
        if let Some(usage) = v.pointer("/message/usage") {
            let input = usage.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let output = usage.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let cache_read = usage
                .get("cache_read_input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let cache_creation = usage
                .get("cache_creation_input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);

            if input + output + cache_read + cache_creation > 0 {
                cache.last_turn_input = input;
                cache.last_turn_output = output;
                cache.last_turn_cache_read = cache_read;
                cache.last_turn_cache_creation = cache_creation;

                cache.total_input += input;
                cache.total_output += output;
                cache.total_cache_read += cache_read;
                cache.total_cache_creation += cache_creation;

                if let Some(ts) = v.get("timestamp").and_then(|x| x.as_str()) {
                    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
                        let ms = dt.timestamp_millis();
                        if cache.first_turn_ms.is_none() {
                            cache.first_turn_ms = Some(ms);
                        }
                        cache.last_turn_ms = Some(ms);
                        if cache_read > 0 {
                            cache.last_cache_read_ms = Some(ms);
                        }
                    }
                }
            }
        }

        if let Some(content) = v.pointer("/message/content").and_then(|x| x.as_array()) {
            for block in content {
                let block_type = block.get("type").and_then(|x| x.as_str()).unwrap_or("");
                if block_type != "tool_use" {
                    continue;
                }
                let name = block.get("name").and_then(|x| x.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                if name == "Skill" {
                    let skill_name = block
                        .pointer("/input/skill")
                        .and_then(|x| x.as_str())
                        .unwrap_or("?");
                    *cache.skill_counts.entry(skill_name.to_string()).or_insert(0) += 1;
                } else if let Some(rest) = name.strip_prefix("mcp__") {
                    let server = rest.split("__").next().unwrap_or(rest);
                    *cache.mcp_counts.entry(server.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
}
