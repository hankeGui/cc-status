# cc-status

> Multi-line, mode-switchable status line for [Claude Code](https://docs.claude.com/en/docs/claude-code) — written in Rust, no daemon, ~20 ms render.

```
~/hanke-dev/cc-status main  Claude Opus 4.7  ctx 86% █████▏  154.6k/950k
```

```
~/hanke-dev/cc-status main  Claude Opus 4.7
ctx 86% █████▏ 154.6k/950k  ↑154.6k ↓185 +687 🎯99%  cache 4:47  hit 96%  🔥 32.4k/min
skills: jira×3 wiki×1   mcp: github×2
```

## Why

Most Claude Code status lines stop at "model name + ctx percentage." That hides what actually drives cost and behavior:

- How many tokens did the **last turn** burn, and how many were a cache hit (10× cheaper)?
- Is the **prompt cache** about to expire (5-min TTL)?
- What's my **session-wide hit rate** — and how fast am I burning tokens?
- Which **Skills / MCP servers** has this session called?
- On a 1M-context model, is my percentage actually computed against 1M, or am I about to be auto-compacted at 200k?

cc-status answers all of these in three lines and lets you switch modes with a single command.

## Install

Pick the easiest path for your machine:

### npm / npx (recommended for Claude Code users)

You already have Node.js if you use Claude Code:

```sh
# One-shot, no install
npx -y cc-status-line --version

# Or install globally so the binary is on PATH
npm install -g cc-status-line
ccs --version
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

### Homebrew (macOS / Linux)

```sh
brew install hankeGui/tap/ccs
```

### curl install script

```sh
curl -fsSL https://raw.githubusercontent.com/hankeGui/cc-status/main/install.sh | sh
```

Installs to `~/.local/bin/ccs`. Pass `--bin-dir` or `--version` to customize.

### Manual download

Pick a tarball from [releases](https://github.com/hankeGui/cc-status/releases), unpack, drop `ccs` somewhere on PATH.

### From source (requires Rust)

```sh
cargo install --git https://github.com/hankeGui/cc-status --locked
```

Or clone and build:

```sh
git clone https://github.com/hankeGui/cc-status
cd cc-status
cargo build --release
cp target/release/ccs ~/.local/bin/
```

After installing, point Claude Code at it:

```json
{
  "statusLine": {
    "type": "command",
    "command": "/absolute/path/to/ccs render"
  }
}
```

Use absolute paths — Claude Code's status-line shell does not always inherit your login `PATH`.

## Three ways to interact

cc-status follows a "passive bar + on-demand panel" design. You don't need to read every metric every turn.

### 1. The status line itself

The bar at the top of every Claude Code prompt. Choose a layout:

```sh
ccs mode compact     # one line, the basics
ccs mode detailed    # three lines, all metrics
ccs mode debug       # six lines, one metric per line with labels
```

Or build your own:

```sh
ccs segments                                    # see all available segments
ccs mode add mine -l "{dir} {git} {ctx}" \
                  -l "{last_turn} {hit_rate}"   # define a 2-line mode
ccs mode mine                                   # switch to it
ccs mode list                                   # show all modes
ccs mode rm mine                                # delete one
```

### 2. The detail panel (`ccs status`)

Print a self-explanatory dashboard for the *current session* — every number labeled, every unit annotated:

```sh
ccs status
```

The panel auto-locates the transcript by `cwd`, so it works from anywhere — even outside Claude Code's status-line invocation context.

### 3. The legend (`ccs explain`)

Forgot what `+865` or `🎯99%` means?

```sh
ccs explain
```

Prints a colored cheat-sheet of every segment, every color, every glyph.

## Segments

| Token | Output | Meaning |
|---|---|---|
| `{dir}` | `~/hanke-dev/cc-status` | Last 3 path components, `~` for HOME |
| `{git}` | `wt:NAME branch ⇡2⇣1 [+!?]` | Worktree, branch, ahead/behind, dirty flags |
| `{model}` | `Claude Opus 4.7` | CC's reported model |
| `{ctx}` | `ctx 86% █████▏ 154.6k/950k` | Remaining %, bar, used / capacity |
| `{ctx_tokens}` | `154.6k/950k` | Just the token numbers |
| `{last_turn}` | `↑12.3k ↓2.1k +865 🎯89%` | Last turn input ↑ / output ↓ / cache write + / hit rate 🎯 |
| `{cache_ttl}` | `cache 3:42` | Prompt-cache 5-min TTL countdown (red < 1 min) |
| `{hit_rate}` | `hit 96%` | Session-wide cache hit rate |
| `{burn}` | `🔥 32.4k/min` | Session-average token rate |
| `{skills}` | `skills: jira×3 wiki×1` | Skill calls, top 4 by count |
| `{mcp}` | `mcp: github×2` | MCP-server calls, top 4 |
| `{mode}` | `[detailed]` | Current mode label |

## Configuration

Lives at `$XDG_CONFIG_HOME/cc-status/config.toml` (macOS: `~/Library/Application Support/dev.hanke.cc-status/config.toml`).

```sh
ccs init             # write defaults if missing
ccs init --force     # overwrite existing
ccs config-path      # print resolved path
```

Example:

```toml
current_mode = "detailed"

[modes.compact]
lines = ["{dir} {git} {model} {ctx}"]

[modes.detailed]
lines = [
  "{dir} {git} {model}",
  "{ctx} {last_turn} {cache_ttl} {hit_rate} {burn}",
  "{skills} {mcp}",
]

[theme]
ctx_low = 20    # red below this remaining %
ctx_med = 50    # yellow below this
```

Add your own modes — any TOML key under `[modes.X]` becomes selectable via `ccs mode X`.

## Capacity calculation (1M context support)

`{ctx}` reports `used / capacity` where:

```
physical_window = used_tokens / (1 - CC_remaining_percentage / 100)
capacity        = physical_window × CLAUDE_AUTOCOMPACT_PCT_OVERRIDE / 100
```

In plain English: cc-status backsolves the *real* context window from CC's reported percentage, then applies your auto-compact threshold to show the *usable* capacity — i.e., the point at which Claude Code will trigger auto-compact.

Two relevant Claude Code env vars (set in `~/.claude/settings.json` `env` block):

| Env var | Meaning | Default |
|---|---|---|
| `CLAUDE_CODE_AUTO_COMPACT_WINDOW` | Treat the window as N tokens (e.g. `1000000` for 1M-context models) | model-detected |
| `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` | Compact at N % of window | `95` |

cc-status reads `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` from the process env at render time, so the bar always matches Claude Code's actual compact behavior.

## Performance

- **Render latency**: ~20 ms (mostly forking `git` for status). Well under Claude Code's 300 ms status-line timeout.
- **Transcript parsing**: incremental — a per-session JSON cache stores the file offset and aggregated counters. Parsing 1 GB of transcript on the first run is the worst case; every subsequent render reads only newly-appended bytes.
- **No daemon, no socket, no IPC**: a single binary, invoked anew each render. State persists via `$XDG_CACHE_HOME/cc-status/session-*.json`.

## Documentation

- [中文使用说明](docs/USAGE.zh.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Releasing](docs/RELEASING.md) — distribution setup (npm, brew, GitHub Releases)
- [CLAUDE.md](CLAUDE.md) — project-level context for Claude Code agents

## Roadmap

- [ ] Daemon mode (Unix socket) for sub-ms cold start
- [ ] Cost segment with per-model pricing
- [ ] Per-session colors / titles (parallel CC instances)
- [ ] Pace-aware quota burn warning
- [ ] Plugin segments (custom shell commands)

## License

MIT — see [LICENSE](LICENSE).
