# cc-status

> Multi-line, mode-switchable status line for [Claude Code](https://docs.claude.com/en/docs/claude-code) — written in Rust. Token cost tracking, prompt-cache TTL, hit rate, burn rate, Skill/MCP usage, daemon mode, all in one binary.

[![npm](https://img.shields.io/npm/v/@cc-status-line/cli?label=npm&color=brightgreen)](https://www.npmjs.com/package/@cc-status-line/cli)
[![GitHub release](https://img.shields.io/github/v/release/hankeGui/cc-status?display_name=tag&sort=semver)](https://github.com/hankeGui/cc-status/releases)
[![License](https://img.shields.io/github/license/hankeGui/cc-status)](LICENSE)

🌐 **Site**: <https://hankegui.github.io/cc-status>
🐙 **Source**: <https://github.com/hankeGui/cc-status>
📦 **npm**: `npm install -g @cc-status-line/cli`

```
~/hanke-dev/cc-status main  Claude Opus 4.7  ctx 86% █████▏  154.6k/950k
```

```
~/hanke-dev/cc-status main  Claude Opus 4.7
ctx 86% █████▏ 154.6k/950k  ↑154.6k ↓185 +687 🎯99%  cache 4:47  hit 96%  🔥 32.4k/min
skills: jira×3 wiki×1   mcp: github×2
```

> Run `bash scripts/screenshots.sh` (with `freeze` on PATH) to regenerate the PNG/SVG screenshots embedded above.

## Why

Most Claude Code status lines stop at "model name + ctx percentage." That hides what actually drives cost and behavior:

- How many tokens did the **last turn** burn, and how many were a cache hit (10× cheaper)?
- Is the **prompt cache** about to expire (5-min TTL) — and how much will the next turn cost if it does?
- What's my **session-wide hit rate** — and how fast am I burning tokens?
- Which **Skills / MCP servers** has this session called?
- On a 1M-context model, is my percentage actually computed against 1M, or am I about to be auto-compacted at 200k?
- **What did this session actually cost in USD** — and how does today / this week compare?

cc-status answers all of these in three lines and lets you switch modes with a single command.

## Highlights

- **7 built-in modes** (`compact` / `minimal` / `detailed` / `cost` / `tokens` / `tools` / `debug`) plus user-defined modes via `ccs mode add` / `append` / `edit`.
- **17 segments** ranging from `{dir}` and `{git}` to `{cost_today}` / `{burn}` / `{cache_ttl}`. Mix and match.
- **Per-model USD pricing** with built-in defaults for Opus / Sonnet / Haiku 4.x and a 1M-context tier multiplier. Override per model in `config.toml`.
- **`ccs cost`** prints a multi-day ASCII dashboard with per-day bars, per-model breakdowns, and `--debug` per-file reconciliation when numbers don't match another tool.
- **`ccs status`** is a self-explanatory dashboard for the current session — every number labeled, every unit annotated.
- **`ccs setup`** writes the `statusLine` block to `~/.claude/settings.json` for you (with backup), so users don't have to hand-edit JSON.
- **Optional Unix-socket daemon** for sub-millisecond renders on slow git repos. `ccs render` falls back to inline rendering if the daemon isn't running.
- **Streaming-aware token accounting**: matches `ccusage` and `cc-switch`'s `(message.id, requestId)` dedupe so we don't double-count partial / final stream events.
- **Local-time day buckets** matching what the Anthropic console shows.
- **Fault-isolated rendering**: a panicking or stuck segment becomes empty space, never blanks the whole status line.

## Install

cc-status is a Rust binary, but you don't need a Rust toolchain — pick whichever channel suits your machine.

> **Heads-up: `npx` is for trying it once. Don't use `npx` to install — Claude Code's `statusLine` runs on every prompt refresh, so you want the binary on PATH.** Use `npm install -g`, `curl`, or Homebrew below.

### npm (recommended for Claude Code users)

You already have Node.js if you use Claude Code.

```sh
npm install -g @cc-status-line/cli
ccs --version
ccs setup           # writes the statusLine block into ~/.claude/settings.json (with backup)
```

In China? Use a faster mirror once:

```sh
npm install -g --registry=https://registry.npmmirror.com @cc-status-line/cli
ccs setup
```

To **try it without installing** (binary won't stay on PATH afterwards):

```sh
npx -y @cc-status-line/cli --version
```

This is fine for kicking the tires, but for actual Claude Code use you want the global install above.

### curl install script

```sh
curl -fsSL https://raw.githubusercontent.com/hankeGui/cc-status/main/install.sh | sh
ccs setup
```

Installs to `~/.local/bin/ccs`. Pass `--bin-dir` or `--version` to customize.

### Homebrew (macOS / Linux)

```sh
brew install hankeGui/tap/ccs
ccs setup
```

### Manual download

Pick a tarball from [releases](https://github.com/hankeGui/cc-status/releases), unpack, drop `ccs` somewhere on PATH. Then `ccs setup`.

### From source (requires a Rust toolchain)

```sh
cargo install --git https://github.com/hankeGui/cc-status --locked
```

> **Note**: `cargo install` puts the binary at `~/.cargo/bin/ccs`. If `~/.cargo/bin` is not on your PATH, add it (rustup's installer normally does this for you):
>
> ```sh
> export PATH="$HOME/.cargo/bin:$PATH"
> ```
>
> `ccs setup` writes an absolute path either way, so Claude Code finds it even if your shell doesn't.

Or clone and build:

```sh
git clone https://github.com/hankeGui/cc-status
cd cc-status
cargo build --release
cp target/release/ccs ~/.local/bin/
```

## After installing — wire it into Claude Code

```sh
ccs setup
```

This inspects `~/.claude/settings.json`, shows the proposed `statusLine` change, prompts y/N, and writes a backup before saving. Pass `--yes` to skip the prompt or `--check` to inspect without modifying. To remove the line later: `ccs setup --uninstall`.

If you'd rather edit by hand:

```json
{
  "statusLine": {
    "type": "command",
    "command": "/absolute/path/to/ccs render"
  }
}
```

Use absolute paths — Claude Code's status-line shell does not always inherit your login `PATH`.

Restart Claude Code and the bar appears at the top of every prompt.

## Three ways to interact

cc-status follows a "passive bar + on-demand panel" design. You don't need to read every metric every turn.

### 1. The status line itself

The bar at the top of every Claude Code prompt. Choose a layout:

```sh
ccs mode compact     # 1 line, the basics (default)
ccs mode minimal     # 1 line, just dir + ctx
ccs mode detailed    # 3 lines, all metrics
ccs mode cost        # 2 lines focused on $ spent
ccs mode tokens      # 3 lines focused on token flow
ccs mode tools       # 3 lines focused on Skill / MCP usage
ccs mode debug       # 8 lines, one metric per line with labels
```

Or build your own without rewriting:

```sh
ccs segments                                      # see all segments
ccs mode append cost_today                        # append a new line to current mode
ccs mode append hit_rate burn                     # append several segments together
ccs mode append --line 1 git                      # append to an existing line
ccs mode append --mode detailed cost_today        # target a specific mode
ccs mode edit                                     # full edit in $EDITOR
ccs mode add mine -l "{dir} {git}" \
                  -l "{last_turn} {hit_rate}"     # define a brand-new mode
ccs mode list                                     # show all modes
ccs mode rm mine                                  # delete one
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
| `{cost_last}` | `last $0.012` | USD cost of the last turn (current model price) |
| `{cost_session}` | `sess $1.42` | Cumulative USD cost for the current session |
| `{cost_today}` | `today $4.18` | USD cost across all sessions today |
| `{cost_week}` | `7d $24.50` | USD cost over the last 7 days |
| `{cost}` | `last $0.012 · today $4.18` | Combo: `cost_last + cost_today` |
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

## Pricing & cost segments

The `cost*` segments compute USD by multiplying per-model token counts
against built-in Anthropic prices (Opus, Sonnet, Haiku, with the 1M
context tier doubling input/output when the model name contains
`[1m]` / `(1m)`). Override or extend the table in your config:

```toml
[pricing.opus]
input  = 12.00     # $/M tokens
output = 60.00

# Or pin an exact model id (substring match, case-insensitive)
[pricing."anthropic--claude-opus-latest"]
input  = 13.00
output = 65.00
```

`{cost_today}` and `{cost_week}` walk every transcript under
`~/.claude/projects/`, incrementally folded into a rollup at
`$XDG_CACHE_HOME/cc-status/rollup.json`. First scan touches every
file; subsequent renders only read newly-appended bytes.

> **Caveat**: today/7d cost relies on the model id Claude Code writes
> into the transcript (`message.model`), which is the canonical id
> like `claude-opus-4-7` without the `[1m]` suffix. If you run on a
> 1M-context tier, today/7d under-counts unless you add a matching
> `[pricing.opus]` override that bakes the doubled price in.

### Reconciling against another tool

Numbers off vs ccusage / cc-switch / your Anthropic invoice? Run:

```sh
ccs cost --days 1 --debug
```

It prints every transcript file that contributed to the window, sorted
by cost — with raw entry count → unique-after-dedupe count, % duplicated,
date-window misses, the four token buckets, and the model. Lets you
pinpoint exactly where the gap is in seconds.

## Performance

- **Render latency**: ~20 ms inline (mostly forking `git`); ~2 ms via the optional daemon. Well under Claude Code's 300 ms status-line timeout either way.
- **Transcript parsing**: incremental — a per-session JSON cache stores the file offset and aggregated counters. First-run scan is the worst case; subsequent renders only read newly-appended bytes.
- **Cross-session rollup**: `~/Library/Caches/dev.hanke.cc-status/rollup.json` holds per-day, per-model token totals + a per-message dedupe set. Loaded only when the active mode references `{cost_today}` / `{cost_week}` / `{cost}`.
- **Fault isolation**: each segment is wrapped in `catch_unwind`; the git helper has a 150 ms hard timeout. A bad segment becomes `""`; the rest of the line still renders.

## Documentation

- [中文使用说明](docs/USAGE.zh.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Releasing](docs/RELEASING.md) — distribution setup (npm, brew, GitHub Releases)
- [CLAUDE.md](CLAUDE.md) — project-level context for Claude Code agents

## Roadmap

Done in 0.3.x:

- [x] Daemon mode (Unix socket) for sub-ms cold start
- [x] Cost segments (`{cost_last}` / `{cost_session}` / `{cost_today}` / `{cost_week}` / `{cost}`)
- [x] Per-model pricing with built-in Opus / Sonnet / Haiku rates and 1M-tier multiplier
- [x] Cross-session cost dashboard (`ccs cost --days N`)
- [x] `ccs cost --debug` per-file reconciliation
- [x] Streaming `(message.id, requestId)` dedupe matching ccusage / cc-switch
- [x] Local-time day buckets
- [x] Auto-update (`ccs upgrade`) detecting npm / curl / brew / cargo install
- [x] Shell completions (`ccs completions <shell>`)
- [x] Fault-isolated segment rendering + git timeout
- [x] `ccs setup` for one-shot `~/.claude/settings.json` wiring
- [x] GitHub Pages site

Still on the list:

- [ ] Re-enable Windows builds (rollup file-rotation detection needs a robust signal there)
- [ ] Per-session colors / titles for parallel CC instances
- [ ] Pace-aware quota burn warning
- [ ] Plugin segments (custom shell commands)
- [ ] Homebrew tap published

## License

MIT — see [LICENSE](LICENSE).
