# cc-status

Multi-line, mode-switchable status line for Claude Code. Single Rust binary, no daemon.

## Features (MVP)

- Multi-line layout (Claude Code natively supports it; almost no one uses it)
- **Last-turn token usage**: input / output / cache creation / cache hit rate
- **Prompt cache TTL** countdown (5-minute window)
- **Per-session Skill / MCP call counts**
- Git: branch, dirty flags, ahead/behind, **worktree detection**
- Quick **mode switching** via `ccs mode <name>` — three preset modes (`compact` / `full` / `debug`), or define your own in TOML
- Incremental transcript parsing (file-offset cache per session) — typical render ~20ms

## Install

```sh
cargo build --release
cp target/release/ccs ~/.local/bin/   # or anywhere on PATH
```

Then in `~/.claude/settings.json`:

```json
{
  "statusLine": {
    "type": "command",
    "command": "ccs render"
  }
}
```

## Usage

```sh
ccs init            # write default config
ccs config-path     # show its location
ccs mode compact    # switch mode
ccs mode full
ccs mode debug
```

## Config

Located at `$XDG_CONFIG_HOME/cc-status/config.toml`
(macOS: `~/Library/Application Support/dev.hanke.cc-status/config.toml`).

```toml
current_mode = "full"

[modes.compact]
lines = ["{dir} {git} {model} {ctx}"]

[modes.full]
lines = [
  "{dir} {git} {model}",
  "{ctx} {last_turn} {cache_ttl}",
  "{skills} {mcp}",
]

[theme]
ctx_low = 20
ctx_med = 50
```

### Available segments

| Token | Output |
|---|---|
| `{dir}` | last 3 path components, `~` for HOME |
| `{git}` | `wt:NAME branch ⇡N⇣N [+!?]` |
| `{model}` | model display name |
| `{ctx}` | `ctx 84% █████` (color: red <20, yellow <50, green ≥50) |
| `{last_turn}` | `↑12.3k ↓2.1k +865 🎯89%` |
| `{cache_ttl}` | `cache 3:42` (red when <1min) |
| `{skills}` | `skills: jira×3 wiki×1` |
| `{mcp}` | `mcp: github×2` |
| `{mode}` | `[full]` |

## Roadmap

- [ ] Daemon mode (Unix socket) for sub-ms cold start
- [ ] Cost segment with model pricing
- [ ] Per-session colors / titles
- [ ] Pace-aware quota burn warning
- [ ] Plugin segments (custom commands / scripts)
