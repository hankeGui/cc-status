# CLAUDE.md

Project-level context for Claude Code agents working on **cc-status**.

## What this is

A status-line tool for Claude Code itself. Single Rust binary (`ccs`) with subcommands: `render`, `status`, `explain`, `segments`, `cost`, `setup`, `upgrade`, `daemon`, `completions`, `mode`, `plugin`, `init`, `config-path`. Lives at <https://github.com/hankeGui/cc-status>.

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

# Tests
cargo test                           # unit + integration; isolated via TempDir + env overrides
cargo test --bin ccs <module>::      # narrow to one module's unit tests
cargo test --test cli <name>         # narrow to one integration test
```

There **are** automated tests — both unit (next to source under `#[cfg(test)] mod tests`) and integration (`tests/cli.rs`, full subcommand coverage with `XDG_*` / `HOME` / `APPDATA` overrides per-test). Don't ship a feature without tests; don't claim "no test framework here" — there is.

## Architecture, in brief

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for full data flow. Module summary:

- `src/main.rs` — CLI dispatch.
- `src/render.rs` — `ccs render` (the status-line entry point; tries daemon, falls back inline).
- `src/status.rs` — `ccs status` (multi-line dashboard).
- `src/explain.rs` — `ccs explain` (static legend).
- `src/segments.rs` — per-segment renderers + `compute_ctx()` (capacity backsolve) + plugin segment dispatcher.
- `src/segments_meta.rs` — single source of truth for the segment catalog.
- `src/transcript.rs` — incremental JSONL parsing + token / tool-use dedupe.
- `src/cache.rs` — per-session disk cache (file offset + counters + dedupe sets).
- `src/config.rs` — TOML schema, load/save, mode switching, `mode add/append/edit/rm`.
- `src/pricing.rs`, `src/rollup.rs`, `src/cost.rs` — cost segments + cross-session rollup + `ccs cost`.
- `src/setup.rs`, `src/upgrade.rs`, `src/daemon.rs` — `ccs setup`, `ccs upgrade`, optional Unix-socket daemon.
- `src/plugin.rs` — `ccs plugin` (scaffold / list / path / run / doctor) for `{plugin:NAME}` segments.

## Conventions

- **No new dependencies without good reason.** The crate has 7 deps; keep it slim.
- **No comments unless WHY is non-obvious.** Identifiers should explain WHAT.
- **No emoji in code or docs unless the user asks.** (The status line itself uses 🎯 / 🔥 — that's content, not authoring style.)
- **ANSI color constants live at the top of each file that uses them.** Don't introduce a "colors" crate.
- **Add new top-level config fields with `#[serde(default = "...")]` and a real `Default` impl.** Deriving `Default` on `Theme` once gave silent zeros — see commit history.
- **Add new `SessionCache` fields with `#[serde(default)]`.** Otherwise existing on-disk caches fail to deserialize and stats reset for users mid-session.

## Things to avoid

- **Don't fetch network / call APIs.** All data must come from CC's stdin JSON or from `~/.claude/*` on disk. cc-status is offline. (Plugins are user code and may technically do whatever they want, but the docs explicitly call out that network calls will blow the 250 ms budget.)
- **Don't hardcode model context windows.** Use the backsolve in `segments::compute_ctx`. Hardcoding will break the day Anthropic ships a new size.
- **Don't write to `~/.claude/`.** That directory belongs to Claude Code. cc-status reads transcripts from there but never writes. Our writes go to `$XDG_CACHE_HOME/cc-status/` and `$XDG_CONFIG_HOME/cc-status/` (which on macOS is `~/Library/Caches/...` and `~/Library/Application Support/...`).
- **Don't break the JSON schema CC sends to `render`.** It's an external contract. New fields = optional with sensible fallbacks.
- **Don't use `unwrap()` in hot paths.** Status-line render must always print something; a panic is a regression even if no one sees the stack trace. Use `?` + bubble up, or fall back to empty string for a single segment.
- **Don't relax `valid_plugin_name` to allow `/` or leading `.`.** That's the only thing keeping `{plugin:../../etc/passwd}` from escaping the plugins directory.
- **Don't lengthen the plugin timeout past 250 ms** without rethinking the render budget — Claude Code's status-line ceiling is 300 ms, and `git status` already eats 150 ms in monorepos.

## When adding a segment

Workflow:

1. Implement `fn seg_<name>(ctx: &Ctx) -> String` in `src/segments.rs`.
2. Add a match arm in `pub fn render(name, ctx)` in the same file.
3. Add a `SegmentInfo` entry in `src/segments_meta.rs` (this drives `ccs segments` output and validation in `mode add`).
4. Update three docs in lockstep:
   - `README.md` — Segments table.
   - `docs/USAGE.zh.md` — 状态栏每段含义.
   - `src/explain.rs` — the legend printed by `ccs explain`.
5. If the segment surfaces a new metric, also surface it in `src/status.rs` (the dashboard).
6. Smoke-test with synthetic stdin (see "Common commands"). Empty/missing data must yield `""` (the renderer collapses surrounding whitespace).
7. Add a unit test in `src/segments.rs::tests` for the renderer; add an integration test in `tests/cli.rs` if behavior is observable end-to-end.

## When working on the conversational helper skill

`.claude/skills/run-cc-status/` is shipped two ways: (1) live in this repo for development; (2) bundled into the binary via `include_str!` in `src/setup.rs` and installed to `~/.claude/skills/cc-status/` when the user runs `ccs setup`. That dual life has a few rules:

- **The bundled SKILL.md uses repo-internal paths** (`./target/release/ccs`, `sh .claude/skills/run-cc-status/driver.sh`). `setup::install_skill_files` rewrites those to user-facing paths during install. If you add a new repo-internal path string to SKILL.md, also add a `.replace(...)` for it in `install_skill_files`, **and** assert in the integration test (`setup_with_skill_installs_skill_files`) that the rewritten copy doesn't contain it. Forgetting this means users get a SKILL.md whose driver instructions point at non-existent paths.
- **The driver script (`driver.sh`) must keep working in the repo too** — running `sh .claude/skills/run-cc-status/driver.sh` from the repo root is part of CI / smoke-testing and how a future agent verifies the skill from this codebase. Keep `${CCS:-./target/release/ccs}` as the in-repo default; the user-side rewrite swaps it for `${CCS:-ccs}`.
- **The skill's `description:` frontmatter is the only thing Claude semantically matches against** when deciding whether to auto-load it. If you add a new workflow (say, "renaming a mode"), add the user's likely phrasing to the description. Generic descriptions ("helps with cc-status") won't get loaded.
- **Don't introduce new `allowed-tools` casually.** The current set is `Bash, Read, Edit, Write` — that's enough to drive every `ccs` command and edit plugin files. Adding e.g. `WebFetch` would expand what Claude could do under this skill in surprising ways.

## When working on `{plugin:NAME}` segments

The plugin segment is the only place where cc-status execs arbitrary user code at render time. Two non-obvious invariants you must preserve:

- **Path validation runs before exec.** `valid_plugin_name` (in both `src/segments.rs` and `src/plugin.rs`) keeps `{plugin:../../foo}` from escaping the plugins directory. Both copies must agree — if you tighten one, tighten the other. Do not pass user-supplied path components straight to `Command::new`.
- **The 250 ms hard timeout uses `try_wait` polling, not just `recv_timeout` with a leaked thread.** When the deadline hits we call `child.kill()` then `wait()`. Don't replace this with a `mpsc::recv_timeout` style ("just abandon the thread") — `git` does that because git is fast; a plugin can spin a 30-second background process and we must not leave it scheduled forever.

When changing the runtime sanitizer (`segments::sanitize_plugin_output`), update `segments::sanitize_plugin_output_for_debug` (the public shim that `plugin run` uses) — they share the implementation, but if you ever decouple them, "what `plugin run` shows" and "what the status line shows" silently diverge. That's the worst class of bug for a debug tool.

When changing templates in `src/plugin.rs`:

- The `# debug: ccs plugin run <name>` line and the `stdout: 80 chars / 250 ms / ANSI SGR allowed` lines are the user-facing contract — keep them in sync with the actual implementation.
- Don't add a third language template casually. We picked sh + python because their cold-start fits the budget; node already pushes 70–150 ms cold, ruby/perl are similar. Adding more is a maintenance tax and nudges users toward runtimes that won't fit.

When changing `plugin doctor`:

- Each check returns a `Severity` (`Ok` / `Warn` / `Fail`). The reported severity is the **worst** of all checks via `Severity::merge`. Don't shortcut and report only the first hit — users want to see every issue at once.
- The orphan check requires loading `Config`. If config load fails, doctor must continue (silently skip the orphan check) rather than abort — users may be debugging a broken config file, and that's exactly when doctor is most useful.

## When changing capacity / context display

`segments::compute_ctx` is the single source of truth. The dashboard in `status.rs` duplicates the formula inline — keep them in sync. If you need to change the formula, change both.

The relevant env vars (read at render time, not start-up):

| Env var | Read by | Purpose |
|---|---|---|
| `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` | `compute_ctx`, `status::run` | Auto-compact threshold (default 95) |
| `CLAUDE_CODE_AUTO_COMPACT_WINDOW` | (not read by us — CC uses it) | What window CC pretends the model has |

`CLAUDE_CODE_AUTO_COMPACT_WINDOW` affects what CC sends in `remaining_percentage`, so we observe its effect indirectly through the backsolve. Don't read it ourselves — that would double-count.

## Pricing / cost segments

`pricing.rs` owns the per-model rate table and the cost formula. Keep
the formula symmetric with `cache_creation = 1.25× input` and
`cache_read = 0.1× input` — those are Anthropic API constants, not
ours to tune. The 1M-context tier doubles input/output when the model
name contains `[1m]` or `(1m)` (substring match).

`rollup.rs` walks every transcript under `~/.claude/projects/` and
folds new lines into `$XDG_CACHE_HOME/cc-status/rollup.json`, keyed
by (UTC day, model id). Known limitation: the `message.model` field
in CC transcripts is the canonical id (`claude-opus-4-7`) without
the `[1m]` suffix, so `cost_today` / `cost_week` under-count when a
user is on the high tier. Today, the recommended workaround is a
user-side `[pricing.opus]` override; a future fix could attribute
the tier from `stdin.model.display_name` at render time and persist
it into the rollup, but that requires schema-bumping the rollup file.

The rollup is loaded only when the active mode references
`{cost_today}`, `{cost_week}`, or `{cost}` — pure session-local
costs (`cost_last`, `cost_session`) skip the scan entirely. Don't
unconditionally load the rollup in `render::run`; that breaks the
fast path for users who don't care about cross-session costs.

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
