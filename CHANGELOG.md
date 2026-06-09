# Changelog

All notable user-facing changes to **cc-status** are recorded here.
Versions follow [SemVer](https://semver.org/) (pre-1.0: minor bumps for
new features, patch bumps for fixes).

## [0.4.1] — 2026-06-09

Theme readability and skill freshness fixes.

### Theme: brighter dim, yellow cost

The `\x1b[2m` (faint) SGR rendered almost invisibly on some terminals
(notably iTerm 2 dark backgrounds), making the second line of
`balanced` mode unreadable. Every `DIM` constant in the codebase now
uses `\x1b[90m` (bright black / light gray) which is consistent
across terminals and visibly brighter while still reading as
"secondary information."

Cost segments (`cost_last` / `cost_session` / `cost_today` /
`cost_week`) now render in **yellow** — money deserves to stand out,
and the dollar sign matches the yellow tone.

### Skill auto-refresh on `ccs upgrade`

`SKILL.md` ships baked into the binary via `include_str!`, but on
disk it lives at `~/.claude/skills/cc-status/SKILL.md`. Before this
release, upgrading cc-status replaced the binary but left a stale
copy of the skill — meaning Claude wouldn't learn about new commands
introduced by the new version (e.g. `ccs config edit` or
`ccs plugin doctor`).

Now `ccs upgrade` overwrites the on-disk skill with the binary's
bundled copy after a successful version bump. Users who never
installed the skill see no change. **Behavior contract:** the refresh
is destructive — any user edits to `SKILL.md` or `driver.sh` are
overwritten. Users who want a customized skill should put it in a
directory other than `cc-status/` (the dirname is checked exactly).

### Other

- New unit tests around `refresh_skill_if_installed_at()` (overwrites
  stale copy, no-ops when not installed).
- New integration test: `setup --yes --with-skill` overwrites a
  pre-existing `SKILL.md` with the bundled copy (the same path
  `ccs upgrade` exercises).

## [0.4.0] — 2026-06-09

A big release that turns cc-status from a status-line renderer into a
small toolkit for *building, debugging, and operating* your status
line. Seven user-visible additions; nothing breaks for existing users
(your `config.toml` and on-disk cache continue to load without touching).

### Custom segments via `{plugin:NAME}`

Drop an executable at `<config>/plugins/<NAME>` and reference it in any
mode as `{plugin:NAME}`. cc-status pipes Claude Code's stdin JSON in,
captures stdout, sanitizes ANSI, clips to 80 chars, hard 250 ms timeout.

A full plugin command family ships with it:

```sh
ccs plugin new <name>            # scaffold a sh template (or --lang python)
ccs plugin run <name> --warm     # debug-run, see exit/elapsed/preview
ccs plugin doctor                # health-check every plugin (orphan / chmod / timing)
ccs plugin list                  # what's installed and is it executable
ccs plugin path                  # the plugins directory
```

The scaffolded templates ship with the contract embedded as comments
(stdin schema, 250 ms budget, 80-char output cap, ANSI SGR allowed).
`plugin run` shows the *exact* sanitized value the status line will
display, so you never restart Claude Code to verify.

### Conversational helper skill

`ccs setup` now offers to install a [Claude Code skill][skills] at
`~/.claude/skills/cc-status/`. Once it's there, you can just *talk*:

- "switch my status line to detailed"
- "add today's cost to my bar"
- "build me a plugin that shows the unread issue count"
- "my status line is blank — figure out why"

Claude reads the skill, runs the right `ccs` commands for you, and
confirms before destructive operations. Force/skip with
`ccs setup --with-skill` / `--no-skill`. Removed by `ccs setup --uninstall`.

[skills]: https://docs.claude.com/en/docs/claude-code/skills

### Drag-and-drop mode editor (`ccs config edit`)

A short-lived 127.0.0.1 web server with a single-page editor:

- Click any mode tab to edit it
- Drag segments from the bottom palette into the mode's lines
- Drag chips between lines to reorder; drag back to palette to delete
- Rename modes by editing the title; mark any mode active
- "Save" writes `config.toml`, server exits 5s later
- 30-min idle timeout; pure stdlib HTTP, no new dependencies

Auth is via random per-launch token in the URL; `Host:` header
allowlist guards against DNS rebinding. Preview is mock-data so you
see what each segment will look like without needing a live transcript.

### Three-stage `{model}` resolution

The model name displayed in `{model}` (and surfaced in `ccs status`)
now resolves through a precedence ladder so intermediaries — Bedrock,
Vertex, OpenRouter, custom proxies — can't lie about what model
Claude Code actually called:

1. **Transcript** — `message.model` from the JSONL (most authoritative)
2. **Stdin** — `model.display_name` then `model.id` from CC
3. **Configured** — `model` field in `~/.claude/settings.json`, then `$CLAUDE_MODEL`

`[1m]` / `(1m)` tier suffix is preserved across stages — if the user
has the 1M-context tier configured anywhere, the displayed label
keeps it. `anthropic--` / `anthropic/` vendor prefixes are stripped;
deployment prefixes (`bedrock/`, `vertex_ai/`, `openrouter/...`) are
preserved because they're meaningful information. **Model names
themselves are never renamed**, so proxies that map to non-Anthropic
ids (`gpt-4-turbo-via-claude-proxy`) still show what they really are.

`ccs status` shows the data source on the model line:

```
│   model        claude-opus-4-7  (transcript)
```

### `{session_age}` segment

How long has this conversation been running?

```
1h23m
```

Format buckets: `42s` / `12m` / `1h23m` / `2d3h`. Returns empty until
the first assistant turn lands.

### `balanced` — new default mode

New installs start with a 3-line layout that covers the things people
glance at every turn:

```
~/repo main  claude-opus-4-7  ctx 86% █████▏ 154.6k/950k
5m  ↑12.3k ↓2.1k +865 🎯89%  last $0.012  sess $1.42  hit 96%  🔥 32.4k/min
skills: jira×3 wiki×1  mcp: github×2
```

Line 3 vanishes when no skill or MCP has been called this session.
Existing installs keep whichever mode they had active.

### Prettier `ccs mode list`

Every template line now gets a synthetic-data preview underneath:

```
▸ balanced  (active)
    template  {dir} {git} {model} {ctx}
    example   ~/cc-status main  claude-opus-4-7  ctx 74% ████▍ 167.2k/635.2k
    template  {session_age} {last_turn} {cost_last} ...
    example   5m  ↑167.2k ↓2.1k +865 🎯92%  last $0.196  sess $0.469  hit 71%  🔥 36.0k/min
```

The active mode is marked with `▸` + `(active)`; the footer prints all
the next-step commands (`switch`, `append`, `edit`, `add`, `explain`,
`plugins`).

### Other improvements

- `cache.last_model` cached per session; `<synthetic>` placeholders skipped
- `ccs setup --uninstall` now also removes the skill if present
- `ccs config edit` honors a 30-minute idle timeout + 5s post-save grace
- `SessionCache` schema bumped (new `last_model` field, backward-compatible via `serde(default)`)

### Test growth

60 unit + 41 integration → **81 unit + 41 integration** (~ 30 new tests).
Coverage focuses on the new surfaces: plugin scaffolder, plugin doctor
severity merging, model resolution precedence + tier preservation,
web server token / Host validation, and `session_age` bucket boundaries.

---

## [0.3.4] — earlier

See git log: `git log v0.3.3..v0.3.4`.
