use crate::cache::SessionCache;
use anyhow::Result;
use chrono::DateTime;
use serde_json::Value;
use std::fs::{File, Metadata};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Get a stable per-file identifier for rotation detection. On Unix this
/// is the inode (cheap and rotation-proof). On Windows / unknown
/// platforms we hash (modified time, len) — good enough to detect
/// "the file got replaced" in practice without needing nightly APIs.
fn file_id(meta: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return meta.ino();
    }
    #[cfg(not(unix))]
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
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(());
    };
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
        cache.last_model = None;
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
        let Ok(v): Result<Value, _> = serde_json::from_str(&line) else {
            continue;
        };
        process_entry(&v, cache);
    }
    cache.file_offset += bytes_consumed;
    Ok(())
}

fn process_entry(v: &Value, cache: &mut SessionCache) {
    let entry_type = v.get("type").and_then(|x| x.as_str()).unwrap_or("");

    if entry_type == "assistant" {
        // Capture the canonical model id Claude Code actually called.
        // Claude Code sometimes injects synthetic assistant entries
        // (slash-command output, system replies) with `model:
        // "<synthetic>"` or other angle-bracketed sentinels — those
        // aren't real model invocations, skip them.
        if let Some(m) = v.pointer("/message/model").and_then(|x| x.as_str()) {
            if !m.is_empty() && !m.starts_with('<') {
                cache.last_model = Some(m.to_string());
            }
        }

        if let Some(usage) = v.pointer("/message/usage") {
            let input = usage
                .get("input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("output_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let cache_read = usage
                .get("cache_read_input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let cache_creation = usage
                .get("cache_creation_input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);

            if input + output + cache_read + cache_creation > 0 {
                // Dedupe TOKEN COUNTS by (message.id, requestId): the
                // same assistant message can be written multiple times
                // (streaming partial → final, or sidechain replay), and
                // each copy carries the same `usage` object — naively
                // summing them double-counts. Mirrors ccusage's
                // adapter/claude::should_replace_deduped_entry.
                //
                // **Important**: this dedupe applies to TOKEN totals
                // only. tool_use blocks have their own `id` (toolu_*)
                // and are dedupe'd separately below — otherwise the
                // `return` here would skip a Skill / MCP call that
                // appeared in a later copy of the same message but
                // not the first one we saw.
                let dedupe_key = v
                    .pointer("/message/id")
                    .and_then(|x| x.as_str())
                    .map(|mid| {
                        let req = v
                            .get("requestId")
                            .or_else(|| v.get("request_id"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("");
                        format!("{}|{}", mid, req)
                    });

                let mut credited_input = input;
                let mut credited_output = output;
                let mut credited_cache_read = cache_read;
                let mut credited_cache_creation = cache_creation;
                let mut skip_tokens = false;

                if let Some(key) = &dedupe_key {
                    let new_total = input + output + cache_read + cache_creation;
                    if let Some(prev) = cache.seen.get(key) {
                        let prev_total =
                            prev.input + prev.output + prev.cache_read + prev.cache_creation;
                        if new_total <= prev_total {
                            // existing copy is at least as complete — skip token credit
                            // (but still process tool_use blocks below).
                            skip_tokens = true;
                        } else {
                            // upgrade: subtract old contribution, add new
                            credited_input = input.saturating_sub(prev.input);
                            credited_output = output.saturating_sub(prev.output);
                            credited_cache_read = cache_read.saturating_sub(prev.cache_read);
                            credited_cache_creation =
                                cache_creation.saturating_sub(prev.cache_creation);
                        }
                    }
                }

                if !skip_tokens {
                    cache.last_turn_input = input;
                    cache.last_turn_output = output;
                    cache.last_turn_cache_read = cache_read;
                    cache.last_turn_cache_creation = cache_creation;

                    cache.total_input += credited_input;
                    cache.total_output += credited_output;
                    cache.total_cache_read += credited_cache_read;
                    cache.total_cache_creation += credited_cache_creation;

                    if let Some(key) = dedupe_key {
                        cache.seen.insert(
                            key,
                            crate::cache::SeenMessage {
                                input,
                                output,
                                cache_read,
                                cache_creation,
                            },
                        );
                    }

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
        }

        // tool_use blocks always processed, regardless of token-level
        // dedupe. Dedupe at the per-block level using the block's own
        // `id` (toolu_*) so the same call isn't counted twice if the
        // entry is replayed.
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
                // Per-call dedupe: each tool_use has a unique id (toolu_*).
                if let Some(tool_id) = block.get("id").and_then(|x| x.as_str()) {
                    if !cache.seen_tools.insert(tool_id.to_string()) {
                        // already counted this exact tool call
                        continue;
                    }
                }
                if name == "Skill" {
                    let skill_name = block
                        .pointer("/input/skill")
                        .and_then(|x| x.as_str())
                        .unwrap_or("?");
                    *cache
                        .skill_counts
                        .entry(skill_name.to_string())
                        .or_insert(0) += 1;
                } else if let Some(rest) = name.strip_prefix("mcp__") {
                    let server = rest.split("__").next().unwrap_or(rest);
                    *cache.mcp_counts.entry(server.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_lines(tmp: &tempfile::TempDir, lines: &[&str]) -> std::path::PathBuf {
        let path = tmp.path().join("transcript.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
        path
    }

    fn append_lines(path: &std::path::Path, lines: &[&str]) {
        let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
    }

    #[test]
    fn parses_assistant_usage() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lines(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-04T10:00:00Z","message":{"usage":{"input_tokens":5,"output_tokens":100,"cache_read_input_tokens":1000,"cache_creation_input_tokens":50}}}"#,
            ],
        );
        let mut cache = SessionCache::default();
        update(&path, &mut cache).unwrap();
        assert_eq!(cache.last_turn_input, 5);
        assert_eq!(cache.last_turn_output, 100);
        assert_eq!(cache.last_turn_cache_read, 1000);
        assert_eq!(cache.last_turn_cache_creation, 50);
        assert_eq!(cache.total_input, 5);
        assert_eq!(cache.total_output, 100);
        assert!(cache.first_turn_ms.is_some());
        assert!(cache.last_cache_read_ms.is_some());
    }

    #[test]
    fn accumulates_across_turns() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lines(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-04T10:00:00Z","message":{"usage":{"input_tokens":10,"output_tokens":20}}}"#,
                r#"{"type":"assistant","timestamp":"2026-06-04T10:01:00Z","message":{"usage":{"input_tokens":30,"output_tokens":40}}}"#,
            ],
        );
        let mut cache = SessionCache::default();
        update(&path, &mut cache).unwrap();
        assert_eq!(cache.total_input, 40);
        assert_eq!(cache.total_output, 60);
        assert_eq!(cache.last_turn_input, 30);
        assert_eq!(cache.last_turn_output, 40);
    }

    #[test]
    fn incremental_only_reads_new_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lines(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-04T10:00:00Z","message":{"usage":{"input_tokens":10,"output_tokens":20}}}"#,
            ],
        );
        let mut cache = SessionCache::default();
        update(&path, &mut cache).unwrap();
        let after_first = cache.file_offset;

        append_lines(
            &path,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-04T10:01:00Z","message":{"usage":{"input_tokens":5,"output_tokens":7}}}"#,
            ],
        );
        update(&path, &mut cache).unwrap();
        assert!(cache.file_offset > after_first);
        assert_eq!(cache.total_input, 15);
        assert_eq!(cache.total_output, 27);
    }

    #[test]
    fn rotation_resets_state() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lines(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-04T10:00:00Z","message":{"usage":{"input_tokens":10,"output_tokens":20}}}"#,
            ],
        );
        let mut cache = SessionCache::default();
        update(&path, &mut cache).unwrap();
        assert_eq!(cache.total_input, 10);

        std::fs::remove_file(&path).unwrap();
        let _ = write_lines(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-04T11:00:00Z","message":{"usage":{"input_tokens":7,"output_tokens":3}}}"#,
            ],
        );
        update(&path, &mut cache).unwrap();
        assert_eq!(cache.total_input, 7, "totals should reset on rotation");
        assert_eq!(cache.total_output, 3);
    }

    #[test]
    fn counts_skill_and_mcp_tool_uses() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lines(
            &tmp,
            &[
                r#"{"type":"assistant","message":{"usage":{"input_tokens":1},"content":[{"type":"tool_use","name":"Skill","input":{"skill":"jira"}},{"type":"tool_use","name":"Skill","input":{"skill":"jira"}}]}}"#,
                r#"{"type":"assistant","message":{"usage":{"input_tokens":1},"content":[{"type":"tool_use","name":"mcp__github__list_issues"},{"type":"tool_use","name":"mcp__github__create_pr"}]}}"#,
            ],
        );
        let mut cache = SessionCache::default();
        update(&path, &mut cache).unwrap();
        assert_eq!(cache.skill_counts.get("jira"), Some(&2));
        assert_eq!(cache.mcp_counts.get("github"), Some(&2));
    }

    #[test]
    fn skips_malformed_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lines(
            &tmp,
            &[
                "not json",
                r#"{"type":"assistant","message":{"usage":{"input_tokens":3,"output_tokens":4}}}"#,
                "",
                "{",
            ],
        );
        let mut cache = SessionCache::default();
        update(&path, &mut cache).unwrap();
        assert_eq!(cache.total_input, 3);
        assert_eq!(cache.total_output, 4);
    }

    #[test]
    fn dedupe_does_not_drop_skill_in_smaller_copy() {
        // Regression: the same assistant message can appear multiple times
        // in the JSONL (streaming partial → final, sidechain replay). The
        // earlier token-dedupe path used `return` to skip duplicate copies,
        // which also skipped any tool_use blocks present only in those
        // copies — silently dropping Skill / MCP counts. Layout:
        //
        //   line A: msg_X, total=29195, no tool_use            → seen[X]=29195
        //   line B: msg_X, total=29195, Skill(sap-jira)        → must still count Skill
        //   line C: msg_X, total=29415 (upgrade), no tool_use  → must credit only delta
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lines(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-04T10:00:00Z","requestId":"req_1","message":{"id":"msg_X","usage":{"input_tokens":29195,"output_tokens":0}}}"#,
                r#"{"type":"assistant","timestamp":"2026-06-04T10:00:01Z","requestId":"req_1","message":{"id":"msg_X","usage":{"input_tokens":29195,"output_tokens":0},"content":[{"type":"tool_use","id":"toolu_A","name":"Skill","input":{"skill":"sap-jira"}}]}}"#,
                r#"{"type":"assistant","timestamp":"2026-06-04T10:00:02Z","requestId":"req_1","message":{"id":"msg_X","usage":{"input_tokens":29415,"output_tokens":0}}}"#,
            ],
        );
        let mut cache = SessionCache::default();
        update(&path, &mut cache).unwrap();
        assert_eq!(
            cache.skill_counts.get("sap-jira"),
            Some(&1),
            "Skill in a duplicate copy must still be counted"
        );
        assert_eq!(
            cache.total_input, 29415,
            "duplicate must not double-count tokens; upgrade must credit only the delta"
        );
    }

    #[test]
    fn missing_file_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("does-not-exist.jsonl");
        let mut cache = SessionCache::default();
        update(&path, &mut cache).expect("missing file should not error");
        assert_eq!(cache.total_input, 0);
        assert!(cache.last_model.is_none());
    }

    #[test]
    fn captures_message_model_from_transcript() {
        // The status line resolves the displayed model id from
        // cache.last_model first — making sure transcripts populate it
        // is the whole point.
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lines(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-09T10:00:00Z","message":{"model":"claude-opus-4-7","usage":{"input_tokens":1,"output_tokens":1}}}"#,
            ],
        );
        let mut cache = SessionCache::default();
        update(&path, &mut cache).unwrap();
        assert_eq!(cache.last_model.as_deref(), Some("claude-opus-4-7"));
    }

    #[test]
    fn skips_synthetic_model_marker() {
        // Claude Code injects entries like `model: "<synthetic>"` for
        // slash-command outputs. Those aren't real model invocations
        // and must not overwrite a real model id we've already seen.
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lines(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-09T10:00:00Z","message":{"model":"claude-opus-4-7","usage":{"input_tokens":1,"output_tokens":1}}}"#,
                r#"{"type":"assistant","timestamp":"2026-06-09T10:00:01Z","message":{"model":"<synthetic>","usage":{"input_tokens":0,"output_tokens":0}}}"#,
            ],
        );
        let mut cache = SessionCache::default();
        update(&path, &mut cache).unwrap();
        assert_eq!(
            cache.last_model.as_deref(),
            Some("claude-opus-4-7"),
            "real model id from line 1 must survive line 2's <synthetic>"
        );
    }
}
