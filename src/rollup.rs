//! Cross-session rollup: aggregate token counts (per model, per local
//! day) by walking every transcript under `~/.claude/projects/`.
//!
//! State persists at `$XDG_CACHE_HOME/cc-status/rollup.json`. We track
//! each transcript's inode + file_offset so subsequent renders only
//! pay for newly-appended bytes, just like `transcript::update` does
//! per session.

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{File, Metadata};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Aggregated counts for one (day, model) bucket.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DayBucket {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FileState {
    pub offset: u64,
    pub file_id: u64,
    /// Last assistant model name we saw in this file. Persists across
    /// runs so we can attribute newly-appended turns even if the line
    /// itself doesn't repeat the model id.
    pub last_model: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Rollup {
    /// "YYYY-MM-DD" → model id → bucket.
    #[serde(default)]
    pub by_day: HashMap<String, HashMap<String, DayBucket>>,
    /// Absolute jsonl path → state.
    #[serde(default)]
    pub files: HashMap<String, FileState>,
    /// Per-message dedupe map. Key = (message.id + "|" + requestId).
    /// Value = (day, model, token totals already credited to that bucket).
    /// When the same message reappears (streaming partial → final),
    /// we replace the credited totals so we don't double-count, mirroring
    /// ccusage's `should_replace_deduped_entry` logic.
    #[serde(default)]
    pub seen: HashMap<String, SeenEntry>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SeenEntry {
    pub day: String,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
}

fn rollup_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("dev", "hanke", "cc-status").context("project dirs")?;
    Ok(dirs.cache_dir().join("rollup.json"))
}

pub fn load() -> Rollup {
    rollup_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(r: &Rollup) -> Result<()> {
    let path = rollup_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string(r)?)?;
    Ok(())
}

fn projects_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude/projects"))
}

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

/// Walk every transcript under `~/.claude/projects/`, incrementally
/// folding new lines into the rollup. Quiet on per-file failures so a
/// single corrupt jsonl doesn't break the whole status line.
pub fn refresh(r: &mut Rollup) {
    let Some(root) = projects_dir() else { return };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };

    for proj in entries.flatten() {
        let proj_path = proj.path();
        if !proj_path.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&proj_path) else {
            continue;
        };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let _ = update_one_file(&path, r);
        }
    }
}

fn update_one_file(path: &Path, r: &mut Rollup) -> Result<()> {
    let meta = std::fs::metadata(path)?;
    let id = file_id(&meta);
    let size = meta.len();

    let key = path.to_string_lossy().to_string();
    let mut state = r.files.remove(&key).unwrap_or_default();

    // Detect rotation/truncation. On Unix the inode change is the
    // canonical signal; on other platforms `file_id` is a hash of
    // (mtime, len) and changes on every append, so we can't use it
    // to detect rotation — fall back to "did the file shrink below
    // our last offset?".
    let rotated = if cfg!(unix) {
        state.file_id != 0 && state.file_id != id
    } else {
        false
    } || size < state.offset;

    if rotated {
        // File was replaced or truncated — start over for this path.
        // (We don't subtract previously-folded counts; that's a
        // known accounting drift.)
        state = FileState::default();
    }
    if size == state.offset {
        state.file_id = id;
        r.files.insert(key, state);
        return Ok(());
    }

    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(state.offset))?;
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
        process_entry(&v, &mut state, r);
    }

    state.file_id = id;
    state.offset += bytes_consumed;
    r.files.insert(key, state);
    Ok(())
}

fn process_entry(v: &Value, state: &mut FileState, r: &mut Rollup) {
    if v.get("type").and_then(|x| x.as_str()) != Some("assistant") {
        return;
    }
    if let Some(m) = v
        .pointer("/message/model")
        .and_then(|x| x.as_str())
        .or_else(|| v.pointer("/model/id").and_then(|x| x.as_str()))
        .or_else(|| v.pointer("/model").and_then(|x| x.as_str()))
    {
        state.last_model = Some(m.to_string());
    }
    let Some(usage) = v.pointer("/message/usage") else {
        return;
    };
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
    if input + output + cache_read + cache_creation == 0 {
        return;
    }

    let day = v
        .get("timestamp")
        .and_then(|x| x.as_str())
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| {
            let now = Local::now();
            format!("{:04}-{:02}-{:02}", now.year(), now.month(), now.day())
        });

    let model = state.last_model.clone().unwrap_or_else(|| "unknown".into());

    // Dedupe key: (message.id + requestId). When CC streams a turn, the
    // same message.id can appear multiple times in the JSONL (partial /
    // final / sidechain replay). ccusage handles this by keeping the
    // copy with the largest token total. Mirror that.
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

    let new_total = input + output + cache_read + cache_creation;

    if let Some(key) = &dedupe_key {
        if let Some(prev) = r.seen.get(key).cloned() {
            let prev_total = prev.input + prev.output + prev.cache_read + prev.cache_creation;
            if new_total <= prev_total {
                // Existing entry is at least as complete — drop this one.
                return;
            }
            // New entry has more tokens (streaming finalized after a
            // partial). Subtract the old contribution before adding the
            // new one, so the running totals reflect the larger version.
            if let Some(by_model) = r.by_day.get_mut(&prev.day) {
                if let Some(b) = by_model.get_mut(&prev.model) {
                    b.input = b.input.saturating_sub(prev.input);
                    b.output = b.output.saturating_sub(prev.output);
                    b.cache_read = b.cache_read.saturating_sub(prev.cache_read);
                    b.cache_creation = b.cache_creation.saturating_sub(prev.cache_creation);
                }
            }
        }
    }

    let bucket = r
        .by_day
        .entry(day.clone())
        .or_default()
        .entry(model.clone())
        .or_default();
    bucket.input += input;
    bucket.output += output;
    bucket.cache_read += cache_read;
    bucket.cache_creation += cache_creation;

    if let Some(key) = dedupe_key {
        r.seen.insert(
            key,
            SeenEntry {
                day,
                model,
                input,
                output,
                cache_read,
                cache_creation,
            },
        );
    }
}

/// Sum buckets over the last `days` days (ending today, local time).
pub fn sum_last_days(r: &Rollup, days: i64) -> HashMap<String, DayBucket> {
    let today = Local::now();
    let mut out: HashMap<String, DayBucket> = HashMap::new();
    for offset in 0..days {
        let d = today - chrono::Duration::days(offset);
        let key = format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day());
        let Some(by_model) = r.by_day.get(&key) else {
            continue;
        };
        for (model, b) in by_model {
            let acc = out.entry(model.clone()).or_default();
            acc.input += b.input;
            acc.output += b.output;
            acc.cache_read += b.cache_read;
            acc.cache_creation += b.cache_creation;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture(tmp: &tempfile::TempDir, lines: &[&str]) -> PathBuf {
        let p = tmp.path().join("a.jsonl");
        let mut f = std::fs::File::create(&p).unwrap();
        for l in lines {
            writeln!(f, "{}", l).unwrap();
        }
        p
    }

    #[test]
    fn folds_one_assistant_turn() {
        let tmp = tempfile::tempdir().unwrap();
        let path = fixture(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-04T10:00:00Z","message":{"model":"claude-opus-4-7","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":100}}}"#,
            ],
        );
        let mut r = Rollup::default();
        update_one_file(&path, &mut r).unwrap();
        let day = r.by_day.get("2026-06-04").unwrap();
        let b = day.get("claude-opus-4-7").unwrap();
        assert_eq!(b.input, 10);
        assert_eq!(b.output, 20);
        assert_eq!(b.cache_read, 100);
    }

    #[test]
    fn aggregates_across_days() {
        let tmp = tempfile::tempdir().unwrap();
        let path = fixture(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-04T10:00:00Z","message":{"model":"claude-opus-4-7","usage":{"input_tokens":10,"output_tokens":20}}}"#,
                r#"{"type":"assistant","timestamp":"2026-06-05T10:00:00Z","message":{"model":"claude-opus-4-7","usage":{"input_tokens":7,"output_tokens":3}}}"#,
            ],
        );
        let mut r = Rollup::default();
        update_one_file(&path, &mut r).unwrap();
        assert_eq!(
            r.by_day
                .get("2026-06-04")
                .unwrap()
                .get("claude-opus-4-7")
                .unwrap()
                .input,
            10
        );
        assert_eq!(
            r.by_day
                .get("2026-06-05")
                .unwrap()
                .get("claude-opus-4-7")
                .unwrap()
                .input,
            7
        );
    }

    #[test]
    fn incremental_only_processes_new_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = fixture(
            &tmp,
            &[
                r#"{"type":"assistant","timestamp":"2026-06-04T10:00:00Z","message":{"model":"claude-opus-4-7","usage":{"input_tokens":10,"output_tokens":20}}}"#,
            ],
        );
        let mut r = Rollup::default();
        update_one_file(&path, &mut r).unwrap();
        let after_first = r.files[&path.to_string_lossy().to_string()].offset;

        // append a second line
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(f, r#"{{"type":"assistant","timestamp":"2026-06-04T11:00:00Z","message":{{"model":"claude-opus-4-7","usage":{{"input_tokens":3,"output_tokens":5}}}}}}"#).unwrap();
        }

        update_one_file(&path, &mut r).unwrap();
        assert!(r.files[&path.to_string_lossy().to_string()].offset > after_first);
        assert_eq!(
            r.by_day
                .get("2026-06-04")
                .unwrap()
                .get("claude-opus-4-7")
                .unwrap()
                .input,
            13
        );
    }

    #[test]
    fn sum_last_days_works() {
        let mut r = Rollup::default();
        let today = chrono::Local::now();
        let key = format!(
            "{:04}-{:02}-{:02}",
            today.year(),
            today.month(),
            today.day()
        );
        let mut day = HashMap::new();
        day.insert(
            "claude-opus-4-7".into(),
            DayBucket {
                input: 100,
                output: 200,
                cache_read: 0,
                cache_creation: 0,
            },
        );
        r.by_day.insert(key, day);

        let s = sum_last_days(&r, 1);
        assert_eq!(s.get("claude-opus-4-7").unwrap().input, 100);
    }
}
