# CLAUDE.md

Project-level context for Claude Code agents working on **cc-status**.

## What this is

A status-line tool for Claude Code itself. Single Rust binary (`ccs`) with subcommands: `render`, `status`, `explain`, `mode`, `init`, `config-path`. Lives at <https://github.com/hankeGui/cc-status>.

## Common commands

```sh
# Build
cargo build --release

# Install locally (most users do this after building)
cp target/release/ccs ~/.local/bin/ccs

# Run a synthetic render (verify changes without restarting Claude Code)
echo '{"cwd":"'$PWD'","model":{"display_name":"Test"},"context_window":{"remaining_percentage":62},"session_id":"smoke","transcript_path":"/path/to/some.jsonl"}' \
  | ./target/release/ccs render

# Run the dashboard (auto-locates a transcript by cwd)
./target/release/ccs status
```

There are **no automated tests** in the tree. Verify behavior with synthetic stdin (above) and real transcripts under `~/.claude/projects/`.

## Architecture, in brief

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for full data flow. Module summary:

- `src/main.rs` — CLI dispatch.
- `src/render.rs` — `ccs render` (the status-line entry point).
- `src/status.rs` — `ccs status` (multi-line dashboard).
- `src/explain.rs` — `ccs explain` (static legend).
- `src/segments.rs` — per-segment renderers + `compute_ctx()` (capacity backsolve).
- `src/transcript.rs` — incremental JSONL parsing.
- `src/cache.rs` — per-session disk cache (file offset + counters).
- `src/config.rs` — TOML schema, load/save, mode switching.

## Conventions

- **No new dependencies without good reason.** The crate has 7 deps; keep it slim.
- **No comments unless WHY is non-obvious.** Identifiers should explain WHAT.
- **No emoji in code or docs unless the user asks.** (The status line itself uses 🎯 / 🔥 — that's content, not authoring style.)
- **ANSI color constants live at the top of each file that uses them.** Don't introduce a "colors" crate.
- **Add new top-level config fields with `#[serde(default = "...")]` and a real `Default` impl.** Deriving `Default` on `Theme` once gave silent zeros — see commit history.
- **Add new `SessionCache` fields with `#[serde(default)]`.** Otherwise existing on-disk caches fail to deserialize and stats reset for users mid-session.

## Things to avoid

- **Don't fetch network / call APIs.** All data must come from CC's stdin JSON or from `~/.claude/*` on disk. cc-status is offline.
- **Don't hardcode model context windows.** Use the backsolve in `segments::compute_ctx`. Hardcoding will break the day Anthropic ships a new size.
- **Don't add a daemon yet.** It's on the roadmap, but the cold-start budget is fine. Don't optimize prematurely.
- **Don't write to `~/.claude/`.** That directory belongs to Claude Code. cc-status reads transcripts from there but never writes. Our writes go to `$XDG_CACHE_HOME/cc-status/` and `$XDG_CONFIG_HOME/cc-status/`.
- **Don't break the JSON schema CC sends to `render`.** It's an external contract. New fields = optional with sensible fallbacks.
- **Don't use `unwrap()` in hot paths.** Status-line render must always print something; a panic is a regression even if no one sees the stack trace. Use `?` + bubble up, or fall back to empty string for a single segment.

## When adding a segment

Workflow:

1. Implement `fn seg_<name>(ctx: &Ctx) -> String` in `src/segments.rs`.
2. Add a match arm in `pub fn render(name, ctx)` in the same file.
3. Update three docs in lockstep:
   - `README.md` — Segments table.
   - `docs/USAGE.zh.md` — 状态栏每段含义.
   - `src/explain.rs` — the legend printed by `ccs explain`.
4. If the segment surfaces a new metric, also surface it in `src/status.rs` (the dashboard).
5. Smoke-test with synthetic stdin (see "Common commands"). Empty/missing data must yield `""` (the renderer collapses surrounding whitespace).

## When changing capacity / context display

`segments::compute_ctx` is the single source of truth. The dashboard in `status.rs` duplicates the formula inline — keep them in sync. If you need to change the formula, change both.

The relevant env vars (read at render time, not start-up):

| Env var | Read by | Purpose |
|---|---|---|
| `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` | `compute_ctx`, `status::run` | Auto-compact threshold (default 95) |
| `CLAUDE_CODE_AUTO_COMPACT_WINDOW` | (not read by us — CC uses it) | What window CC pretends the model has |

`CLAUDE_CODE_AUTO_COMPACT_WINDOW` affects what CC sends in `remaining_percentage`, so we observe its effect indirectly through the backsolve. Don't read it ourselves — that would double-count.

## Release flow

```sh
# Bump version in Cargo.toml
# Commit
git tag -a vX.Y.Z -m "vX.Y.Z — <short description>"
git push origin main
git push origin vX.Y.Z

# Optional: GitHub Release (visible in repo sidebar)
gh release create vX.Y.Z --title "vX.Y.Z — ..." --notes-from-tag
```
