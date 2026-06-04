# Architecture

This document explains how cc-status is put together — for anyone reading the source or considering a contribution.

## Design goals

1. **Render in well under 300 ms** (Claude Code's status-line timeout). Aim for ~20 ms.
2. **No background process.** Each render is a fresh process invocation; state persists on disk.
3. **Honest numbers.** Backsolve real values rather than hardcoding model windows; let env vars override behavior.
4. **Self-explanatory.** Every glyph in the status line should be explainable without reading the source.

## Data flow

```
                  ┌─────────────────────────┐
                  │  Claude Code (parent)   │
                  │  every prompt refresh   │
                  └────────────┬────────────┘
                               │ spawns
                               ▼  stdin: status JSON
                       ┌───────────────┐
                       │  ccs render   │  (single binary)
                       └───────┬───────┘
                               │ reads
                  ┌────────────┴────────────┐
                  │                         │
                  ▼                         ▼
         transcript JSONL         per-session cache
   ~/.claude/projects/.../*.jsonl    ~/Library/Caches/.../session-*.json
   (CC writes; we read incrementally)  (cc-status writes; stores file offset)
                  │                         │
                  └────────────┬────────────┘
                               ▼
                       ┌───────────────┐
                       │ apply config  │   ~/Library/Application Support/.../config.toml
                       │ render mode   │
                       └───────┬───────┘
                               │
                               ▼
                       single ANSI line(s) → stdout
```

The detail panel (`ccs status`) and legend (`ccs explain`) reuse the same transcript-parsing path but write a multi-line, human-readable report instead of an ANSI status line.

## Module map

| File | Lines | Responsibility |
|---|---|---|
| `src/main.rs` | 52 | CLI dispatch via `clap`. Subcommands: `render` / `status` / `explain` / `mode` / `init` / `config-path`. |
| `src/render.rs` | 86 | The `render` subcommand. Reads stdin JSON, runs transcript update, looks up the active mode, expands `{segment}` placeholders, prints lines. |
| `src/segments.rs` | 344 | Each segment's renderer (`seg_dir`, `seg_git`, ...). All ANSI coloring lives here. |
| `src/transcript.rs` | 149 | Incremental JSONL parsing. Tracks per-session file offset + inode (rotation detection). Extracts `usage`, `tool_use`, timestamps. |
| `src/cache.rs` | 63 | Per-session JSON cache (file offset, aggregated counters, last-turn fields). |
| `src/config.rs` | 147 | TOML config: defaults (`Default for Config`), load/save, mode switching. |
| `src/status.rs` | 258 | The `status` subcommand. Standalone full dashboard, with auto-locate-transcript fallback. |
| `src/explain.rs` | 70 | Static legend printer (the `explain` subcommand). |

## Transcript parsing

Claude Code writes one JSON object per line under `~/.claude/projects/<sanitized-cwd>/<session-id>.jsonl`. The "sanitized cwd" is the absolute path with every non-alphanumeric char replaced by `-`.

Relevant entry shape:

```json
{
  "type": "assistant",
  "timestamp": "2026-06-04T15:30:00.123Z",
  "message": {
    "usage": {
      "input_tokens": 6,
      "output_tokens": 185,
      "cache_read_input_tokens": 153700,
      "cache_creation_input_tokens": 850
    },
    "content": [
      { "type": "tool_use", "name": "Skill", "input": { "skill": "jira" } },
      { "type": "tool_use", "name": "mcp__github__list_issues", ... }
    ]
  }
}
```

`transcript::update()` does:

1. `stat` the file. If `inode != cache.inode` or `size < cache.file_offset`, the file was rotated/replaced — wipe everything and re-read from scratch.
2. `seek` to `cache.file_offset`, read remaining bytes line by line.
3. For each `assistant` entry: update `last_turn_*`, accumulate `total_*`, set `first_turn_ms` once, always update `last_turn_ms` and (when `cache_read > 0`) `last_cache_read_ms`.
4. Walk the `content` array; bucket `tool_use` blocks by `name == "Skill"` (use `input.skill`) or `name.starts_with("mcp__")` (extract server name from `mcp__<server>__<tool>`).
5. Save `cache.file_offset += bytes_consumed`.

Worst case (first run on a 1 GB transcript) is O(file size). Steady state is O(bytes appended this turn) — typically a few KB.

## Capacity calculation

The trickiest piece. Claude Code's `context_window.remaining_percentage` is **not** a percentage of the model's physical context window — it's the percentage remaining before auto-compact triggers. Hardcoding 200k or 1M based on model name is fragile because:

- Model aliases (`claude-opus-latest[1m]`) vary by deployment.
- Auto-compact threshold is configurable (`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`).
- `CLAUDE_CODE_AUTO_COMPACT_WINDOW` lets users override the assumed window entirely.

Instead, cc-status backsolves:

```rust
let used = last_turn_input + last_turn_cache_read + last_turn_cache_creation;
let physical = used as f64 / (1.0 - remaining_percentage / 100.0);
let pct_override = env::var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
    .and_then(|s| s.parse::<f64>().ok())
    .unwrap_or(95.0)
    .clamp(1.0, 100.0);
let capacity = physical * pct_override / 100.0;
let used_frac = used / capacity;          // drives bar + color
let remaining_pct = 100 * (1 - used_frac); // drives % display
```

This means:

- If a user sets `CLAUDE_CODE_AUTO_COMPACT_WINDOW=1000000` and `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE=95`, capacity displays as 950k.
- If they tighten `PCT_OVERRIDE` to 80, capacity drops to 800k *automatically* — no cc-status reconfiguration needed.
- The "physical window" line in `ccs status` reveals what cc-status backsolved, useful for debugging proxy / beta-header issues.

## Config

Single TOML at `$XDG_CONFIG_HOME/cc-status/config.toml`. Schema is defined entirely by `Config` / `Mode` / `Theme` / `SegmentSettings` structs in `src/config.rs`. Adding a new top-level setting:

1. Add a field with `#[serde(default = "...")]`.
2. Provide a `Default` impl (don't rely on `derive(Default)` if you want non-zero defaults for primitives).
3. Update `init_default()` if it should appear in fresh configs.

## Cache

Per-session JSON at `$XDG_CACHE_HOME/cc-status/session-<sanitized-id>.json`. Schema:

```rust
struct SessionCache {
    file_offset: u64,            // bytes read so far from transcript
    inode: u64,                  // detect file replacement
    last_turn_*: u64,            // most recent assistant turn's tokens
    last_cache_read_ms: Option<i64>,
    skill_counts: HashMap<String, u32>,
    mcp_counts: HashMap<String, u32>,
    total_*: u64,                // cumulative across the session
    first_turn_ms / last_turn_ms: Option<i64>,
}
```

Add new fields with `#[serde(default)]` so existing cache files continue to load. Schema breakage = silently broken state for in-flight sessions.

## Adding a segment

1. Add a renderer in `src/segments.rs`:

   ```rust
   fn seg_my_thing(ctx: &Ctx) -> String {
       // ctx.stdin = parsed CC JSON
       // ctx.cache = SessionCache (already updated)
       // ctx.cfg   = Config
       // return "" if no data; render() collapses spaces around empty segments.
       format!("{}label{}", DIM, RESET)
   }
   ```

2. Wire it into the dispatcher in `render(name, ctx)`.

3. Document it in `README.md` "Segments" table, `docs/USAGE.zh.md`, and `src/explain.rs`.

That's it — no config registration needed. Users can put `{my_thing}` in any mode template.

## Why no daemon (yet)

Sub-binary commands like `ccs render` cold-start in 5–10 ms. The bulk of the 20 ms render budget is `git status` + `rev-parse` forks. A daemon (e.g. via Unix socket) would help here, but adds:

- Lifecycle complexity (start/stop/health/zombie).
- IPC protocol design.
- Per-session multiplexing.

For now: not worth it. If someday the bar visibly lags or someone really needs <2 ms, the roadmap entry is real.

## Testing strategy

Currently there are no automated tests — coverage relies on:

- Manual smoke tests with synthesized stdin (see `README.md` and `docs/USAGE.zh.md`).
- Real Claude Code transcripts under `~/.claude/projects/`.

A reasonable test suite would:

- Snapshot test segments against fixed `Ctx` inputs.
- Property test `progress_bar` and `short_num`.
- Replay a known transcript through `transcript::update` and assert final `SessionCache` state.

Pull requests welcome.
