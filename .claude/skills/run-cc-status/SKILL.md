---
name: run-cc-status
description: Help the user customize, debug, and extend the cc-status status-line for Claude Code. Use this when the user says things like "switch my status line", "change my ccs mode", "add a segment to my status bar", "make my status line show cost / git / cache", "build a custom plugin / segment", "my status line is broken / blank / missing X", "what does this status-line glyph mean", "preview my status line", or any request that mentions ccs, cc-status, or `{plugin:...}`. The skill walks Claude through the full ccs CLI surface so the user never has to read docs or hand-type commands.
allowed-tools: Bash, Read, Edit, Write
---

# Driving cc-status from a Claude Code conversation

cc-status is a Rust binary `ccs` that renders Claude Code's status line.
This skill teaches the agent how to **operate it on the user's behalf**:
switch modes, build plugins, diagnose a broken bar — without making the
user copy commands out of a README.

Paths in this document are relative to the repo root
(`/Users/I547149/hanke-dev/cc-status` during development; users running
the installed `ccs` binary just use `ccs` directly on their PATH).

## When to engage this skill

Activate when the user wants to *change something about their Claude Code
status line*. Common phrasings:

- "switch my status line to detailed / cost / minimal"
- "add today's cost to my bar" / "show git in my status line"
- "make me a plugin that shows X" / "add a custom segment"
- "my status line is blank" / "it's not showing my plugin"
- "what does 🎯 / 🔥 / cache 4:21 mean?"
- "show me my session dashboard"

If the user just asks "what's in this repo," this skill is **not** what
they want — point them at the README instead.

## How to drive it

You have one binary and a few sub-commands. Always prefer running them
yourself with the `Bash` tool over telling the user to type. The user
came here so they wouldn't have to.

```sh
ccs --version                     # confirm install + version
ccs mode list                     # see configured modes (* marks active)
ccs mode <name>                   # switch active mode
ccs mode append [seg ...]         # add a segment without rewriting the mode
ccs mode add <name> -l "<line>"   # define a brand-new mode
ccs mode edit                     # open current mode in $EDITOR
ccs segments                      # list every built-in segment
ccs explain                       # legend for every glyph
ccs status                        # full session dashboard
ccs cost --days 7                 # cross-session cost breakdown
ccs plugin new <name>             # scaffold a sh template
ccs plugin new <name> --lang python
ccs plugin list                   # what's installed + executable status
ccs plugin run <name> [--warm]    # debug-run, see exit/elapsed/preview
ccs plugin doctor                 # health-check every plugin
ccs setup                         # wire ccs into ~/.claude/settings.json
```

All commands are local, reversible, and idempotent. You're allowed to
run them without confirming each one with the user. The exceptions are
**`ccs setup`** (touches `~/.claude/settings.json`) and **`ccs plugin
new --force`** (overwrites a user file) — confirm those.

## Smoke test (run this first if anything looks wrong)

The skill ships with a driver that walks every workflow on a throwaway
config. Use it to confirm the binary works before debugging the user's
real install:

```sh
sh .claude/skills/run-cc-status/driver.sh
```

It builds nothing — assumes `./target/release/ccs` exists. If it
doesn't, run `cargo build --release` first.

## Workflows

### A. "Switch / change my status line mode"

```sh
ccs mode list                     # show current + available, with mock-data preview
ccs mode <name>                   # switch
ccs status                        # render the dashboard against the user's real session
```

`ccs mode list` already paints an `example:` line under every template
using **synthetic data** — that's enough to let the user pick. After
switching, prefer `ccs status` (which auto-locates the user's real
transcript) over feeding mock JSON to `ccs render`. If the user is
inside a fresh session with no transcript yet, fall back to:

```sh
echo '{"cwd":"'$PWD'","model":{"display_name":"Test"},"context_window":{"remaining_percentage":75},"session_id":"x","transcript_path":""}' | ccs render
```

### B. "Add a segment to my status line"

```sh
ccs segments                      # what's available
ccs mode append cost_today        # append a new line to current mode
ccs mode append --line 1 hit_rate # append at the end of an existing line
```

Bare segment names work (`ccs mode append ctx burn`). Curly-wrapped
names (`{ctx}`) and literal text (`cache:`) also pass through. After
appending, run `ccs mode list` again to confirm the result the user
expects.

### C. "Make me a plugin that shows X"

```sh
ccs plugin new <name>             # sh template (recommended)
ccs plugin new <name> --lang python
$EDITOR <plugin-path>             # path is printed by `plugin new`
ccs plugin run <name> --warm      # debug: stdout, stderr, exit, elapsed
ccs mode append plugin:<name>     # wire it into the active mode
```

The template is a hello-world. To make it useful, **edit the file** —
use the `Edit` tool, don't paste a fresh one over it (preserves the
contract comments at the top).

The plugin's `stdin` is Claude Code's status-line JSON
(`{cwd, model.{id,display_name}, context_window.remaining_percentage,
session_id, transcript_path}`). Its `stdout` becomes the segment value:
clipped to 80 chars, newlines collapsed to spaces, ANSI SGR colors
allowed. Hard 250 ms timeout.

After editing, **always run `ccs plugin run <name> --warm`** before
wiring it into a mode. The "As shown in the status line" section of
the output is byte-for-byte what Claude Code will display — that's
the verification the user came here for.

### D. "My status line is blank / not showing X"

Run the diagnostic ladder in this order:

```sh
ccs status                        # session dashboard — shows what data ccs has
ccs plugin doctor                 # checks every installed plugin
ccs explain                       # legend (in case the glyph is just unknown)
ccs config-path                   # show where the config file lives
```

Common causes and fixes:

| Symptom | Likely cause | Fix |
|---|---|---|
| Whole bar is blank | Claude Code can't find `ccs` | `ccs setup --check`; if it points at a wrong binary, `ccs setup` |
| `{ctx}` / `{last_turn}` empty | No transcript yet (fresh session) | Wait one turn, or `ccs status` to confirm the transcript path |
| `{plugin:foo}` empty | Plugin failed | `ccs plugin doctor` then `ccs plugin run foo` |
| Plugin doctor says "exceeds 250 ms" | Logic too slow | Drop network calls, cache files, prefer sh/native binary over python/node |
| Plugin doctor says "not executable" | Missing `chmod +x` | `chmod +x <path>`, or `ccs plugin new --force` to rewrite from template |
| `{cost_today}` looks low on 1M tier | Built-in pricing maps to canonical id | Add `[pricing.opus]` override in config |

If the user added a plugin that doesn't show up: read the plugin file
(`Read` tool), check the shebang, ensure it `printf`s without trailing
newline (or accept the newline; both work), and run `ccs plugin run`
to see the actual stdout.

### E. "What does X mean on my status line"

```sh
ccs explain                       # legend
ccs status                        # current values for each metric
```

`explain` is static; `status` is the live data for the current
session. If the user asks "why is `🎯` 99%", show them both — explain
defines the glyph, status shows where the number came from.

## Templates the skill can reuse

When the user says "build me a plugin that shows my unread email count
/ a stock price / a Linux load average," lean on these starting points
**after** scaffolding via `ccs plugin new`:

| Recipe | Approach |
|---|---|
| Counter from a file | `wc -l < ~/.todo.txt` |
| Read CC's stdin field | `json=$(cat); printf '%s' "$json" \| jq -r .cwd` (note: `jq` adds ~30 ms; prefer pure sh when you can) |
| Color the value | `printf '\033[33m%s\033[0m' "warn"` (ANSI SGR is preserved through sanitize) |
| Cached metric | Have a cron / launchd job write to `/tmp/foo`, plugin just `cat /tmp/foo` |

Anti-patterns to refuse to write:

- Network calls (`curl`, `gh api`, `aws ...`) — almost always blow the
  250 ms budget. Cache to a file from a separate process instead.
- Anything that imports a heavy Python lib (`pandas`, `requests`,
  `boto3`). Cold start alone exceeds 80 ms.

## Gotchas

- **macOS plugin path is not under `~/.config`.** It's
  `~/Library/Application Support/dev.hanke.cc-status/plugins/`. Always
  use `ccs plugin path` to find it; never hand-construct it.
- **`ccs plugin run` has a 5-second debug timeout, render-time has 250 ms.**
  A plugin that finishes in 800 ms during `plugin run` will be **killed**
  during real status-line refresh. Always check the elapsed line and
  encourage `--warm` for the second-run timing.
- **Token segments need a transcript.** `{ctx}`, `{last_turn}`,
  `{burn}`, `{hit_rate}` all return `""` until Claude Code has written
  at least one assistant turn to the JSONL transcript. Surrounding
  whitespace collapses, so a fresh session looks sparse — that's
  expected, not a bug.
- **`mode append` does NOT modify the line for you.** It appends a
  *new* line by default; use `--line N` to extend an existing line.
- **Segments outside the built-in list aren't errors, just warnings.**
  `mode append plugin:foo` works even if no plugin file `foo` exists
  yet — it'll render as `""` until the user creates one.
- **`{plugin:NAME}` runs the executable on every status-line refresh.**
  That's every prompt. Plugins must be fast and idempotent.

## Troubleshooting (errors actually hit during driver verification)

| Error | Fix |
|---|---|
| `cargo: command not found` (running driver) | `source $HOME/.cargo/env` before driving |
| `./target/release/ccs not found` | `cargo build --release` (in the repo root) |
| `plugin '<name>' already exists` | Pass `--force` to overwrite, or pick another name |
| `mode '<name>' does not exist` | `ccs mode list` to see what's actually configured |
| `unknown segment(s): <name>` | `ccs segments` for the catalog. Plugin segments use the `plugin:` prefix |
| Driver fails on `mode list \| grep '^\* detailed'` | Check that the user hasn't aliased `grep` to `rg` or set `GREP_OPTIONS` |

## When to stop and ask the user

- **Before `ccs setup` writes settings.json** — show what it'll write,
  ask before applying. (`ccs setup --check` is the dry-run.)
- **Before overwriting an existing plugin file** — `ccs plugin new
  --force` is destructive. Confirm.
- **Before `ccs mode rm <name>`** — losing a mode definition can be
  annoying. Confirm.
- **When the user's request implies a plugin that needs network or a
  paid API** — propose the cached-file pattern first; explain why a
  direct call breaks the budget.
