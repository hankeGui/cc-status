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

| File | Responsibility |
|---|---|
| `src/main.rs` | CLI dispatch via `clap`. Subcommands: `render` / `status` / `explain` / `segments` / `cost` / `setup` / `upgrade` / `daemon` / `completions` / `mode` / `plugin` / `config` / `init` / `config-path`. |
| `src/render.rs` | The `render` subcommand. Reads stdin JSON, runs transcript update, looks up the active mode, expands `{segment}` placeholders, prints lines. Tries the optional daemon first; falls back to inline. |
| `src/segments.rs` | Each segment's renderer (`seg_dir`, `seg_git`, ..., `seg_plugin`). All ANSI coloring lives here. Wraps each render in `catch_unwind` so a single bad segment can't blank out the whole line. |
| `src/segments_meta.rs` | Single source of truth for the segment catalog (drives `ccs segments` output and `is_known` validation). |
| `src/transcript.rs` | Incremental JSONL parsing. Tracks per-session file offset + inode (rotation detection). Extracts `usage`, `tool_use`, timestamps. Token dedupe by `(message.id, requestId)`; tool_use dedupe by `toolu_*` id (kept independent so streaming dupes can't drop Skill/MCP counts). |
| `src/cache.rs` | Per-session JSON cache (file offset, aggregated counters, last-turn fields, dedupe sets). |
| `src/config.rs` | TOML config: defaults (`Default for Config`), load/save, mode switching, `mode add`/`append`/`edit`/`rm`. Recognizes the `plugin:NAME` token shape when wrapping bare segment names. |
| `src/status.rs` | The `status` subcommand. Standalone full dashboard, with auto-locate-transcript fallback. |
| `src/explain.rs` | Static legend printer (the `explain` subcommand). |
| `src/list_segments.rs` | The `segments` subcommand — pretty-prints `SEGMENTS`. |
| `src/pricing.rs` | Per-model USD price table + cost formula. Built-in Opus / Sonnet / Haiku rates, 1M-tier multiplier, user override map. |
| `src/rollup.rs` | Cross-session aggregation under `~/Library/Caches/.../rollup.json`. Per-day, per-model token totals. Loaded only when a `cost_today` / `cost_week` / `cost` segment is referenced. |
| `src/cost.rs` | The `cost` subcommand — multi-day ASCII dashboard, by-day / by-model bars, `--debug` per-file reconciliation. |
| `src/setup.rs` | The `setup` subcommand — writes the `statusLine` block into `~/.claude/settings.json` (with backup). |
| `src/upgrade.rs` | The `upgrade` subcommand — detects install method (npm / cargo / homebrew / curl) and re-runs it. |
| `src/daemon.rs` | Optional Unix-socket daemon for sub-ms cold starts. Pure stdlib (no tokio); spawn-thread-per-client. |
| `src/plugin.rs` | The `plugin` subcommand — scaffolds, lists, debug-runs, and health-checks `{plugin:NAME}` files under `<config>/plugins/`. Templates for sh + python with the contract embedded. |
| `src/web.rs` | The `config edit` subcommand — short-lived 127.0.0.1 HTTP server backing a drag-and-drop mode editor at `assets/editor.html`. Pure stdlib HTTP parser (GET/POST/OPTIONS, max 64 KB body); URL-token + Host-header allowlist for security. Designed to be the **only** web entry point in cc-status. |

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

3. Add an entry to `SEGMENTS` in `src/segments_meta.rs` (drives `ccs segments` output and validation in `mode add`).

4. Document it in `README.md` "Segments" table, `docs/USAGE.zh.md`, and `src/explain.rs`.

5. If the segment surfaces a new metric, also surface it in `src/status.rs` (the dashboard).

## Plugins (`{plugin:NAME}` segments)

`{plugin:NAME}` is the user-extensibility hatch. At render time, `seg_plugin` execs `<config>/plugins/NAME` with CC's stdin JSON piped in and uses the child's stdout as the segment value. A few invariants make this safe:

- **Path validation.** The `NAME` is checked with `valid_plugin_name`: no `/`, no `\`, no `..`, no leading `.`. The plugin dispatcher refuses anything that could escape the plugins directory before it ever calls `Command::new`.
- **Hard 250 ms timeout.** `run_plugin` uses `try_wait` polling with a deadline; on timeout it calls `child.kill()` and reaps. The render thread is never blocked waiting on a hung plugin.
- **Output sanitization.** `sanitize_plugin_output` collapses `\n` / `\r` / `\t` to single spaces, strips control characters except ESC (so plugins can emit ANSI SGR colors), and clips to 80 chars. Bytes read from stdout are also capped at 4 KB.
- **Fault isolation.** Like every segment, `seg_plugin` runs inside the `catch_unwind` wrapper in `render()`. A panicking or misbehaving plugin renders as `""`; the rest of the line still ships.

The `plugin` subcommand provides four commands so users can build a plugin without leaving the terminal:

| Command | What it does |
|---|---|
| `ccs plugin new <name> [--lang sh\|python] [--force]` | Writes a heavily-commented hello-world template (the contract is in the file's docstring), `chmod +x`, prints the next-step recipe. |
| `ccs plugin list` | Lists files under `<config>/plugins/`, marks each ✓ / ✗ for the executable bit. |
| `ccs plugin path` | Prints the plugins directory path. |
| `ccs plugin run <name> [--warm]` | Debug-runs a plugin against a mock CC JSON, printing raw stdout, stderr, exit, elapsed, and the **sanitized** value the status line will display. With `--warm` it discards a first run so reported timing reflects steady state (skipping macOS Gatekeeper's 200 ms+ cold start). |
| `ccs plugin doctor` | Walks every plugin and grades each: executable bit, shebang or binary detection, warm-run timing vs the 250 ms budget, exit code / stdout, and whether any mode references it. Severity is the worst-of all checks. |

Two design notes worth knowing if you touch this code:

- **Sanitizer is shared with the live path.** `plugin::run_debug` calls `segments::sanitize_plugin_output_for_debug` — a public shim over the same private function `seg_plugin` uses. This guarantees "what `plugin run` shows" is byte-for-byte what the status line will show.
- **Header classification is heuristic, not authoritative.** `classify_header` reads the first 256 bytes and decides shebang / binary / plain text by NUL/high-byte ratio. It's good enough for a doctor warning ("no shebang — kernel may not exec this") but never blocks `seg_plugin` at render time, where the kernel's exec verdict is the ground truth.

## Web editor (`ccs config edit`)

The visual editor is the only place cc-status starts an HTTP server.
Two non-obvious invariants worth keeping intact if you touch
`src/web.rs`:

- **The TCP stream must be put back into blocking mode after `accept()`.** macOS / Linux let the stream inherit the listener's non-blocking flag. If we leave it non-blocking, browsers' speculative pre-connects (HTTP/1.1 connection pre-warming, `<link rel=preconnect>`) hit `read_line` before any bytes arrive and immediately get `EAGAIN` — which we'd interpret as "broken request" and 400. The first request appears to succeed (because the browser actually sent data) but subsequent fetches mysteriously fail. We saw this; the fix is `stream.set_nonblocking(false)` immediately after accept.
- **Token check skips the HTML landing page only.** `GET /` must succeed without a token, because the JS that reads the token from `location.search` hasn't loaded yet. Every API endpoint validates the token (URL `?token=` query, no custom header — that would force a CORS preflight on every call). The auth model is short-lived: a fresh 32-char token per launch, server exits 5 s after Save or 30 min idle.

The HTTP parser is deliberately narrow: GET/POST/OPTIONS, max 64 KB body, `Content-Length`-based body read, no chunked, no keep-alive (`Connection: close` on every response). Adding routes for unrelated features is **not** fine — write a CLI subcommand instead.

`assets/editor.html` is the single-file UI: vanilla HTML + inline CSS + native HTML5 drag-and-drop, no build step. Round-trips templates as `[{seg}, {seg}, ...]` arrays for ease of drag-and-drop, which means literal text inside templates is dropped on save (this is documented in the README and intentional — users who want literal text keep using `mode edit`).

## Why no daemon (yet)

Sub-binary commands like `ccs render` cold-start in 5–10 ms. The bulk of the 20 ms render budget is `git status` + `rev-parse` forks. A daemon (e.g. via Unix socket) would help here, but adds:

- Lifecycle complexity (start/stop/health/zombie).
- IPC protocol design.
- Per-session multiplexing.

For now: not worth it. If someday the bar visibly lags or someone really needs <2 ms, the roadmap entry is real.

## Testing strategy

The crate has both unit and integration tests; CI runs them on macOS-14 and ubuntu-22.04.

- **Unit tests** live next to the code in `#[cfg(test)] mod tests` blocks. They cover pure functions: `progress_bar`, `short_num`, `compute_ctx`, transcript dedupe (including the regression where token-dedupe used to drop Skill/MCP from duplicate copies), pricing math, plugin name validation, output sanitization, header classification, severity merging.
- **Integration tests** live in `tests/cli.rs`. Each test gets a fresh `TempDir` with `HOME` / `XDG_*` / `APPDATA` overridden so the test never touches the developer's real config or cache. Coverage spans `mode add/list/append/edit/rm`, `setup` (install / check / uninstall), `render` (synthetic stdin), and the full `plugin` family (`new` with both langs, `--force` overwrite, name validation, `list`, `path`, `run`, `doctor` against orphan / non-executable / referenced plugins).

When adding a new segment or subcommand:

- Snapshot test the renderer against a fixed `Ctx` if the output is non-trivial.
- Add an integration test if the command writes config / cache / on-disk state — those bugs are easy to ship and hard to spot in manual smoke tests.
- For new `SessionCache` fields, also add a regression test for the dedupe path; the on-disk cache outlives any single render, and silently broken state is the worst kind of bug.
