# @cc-status-line/cli

Multi-line, mode-switchable status line for [Claude Code](https://docs.claude.com/en/docs/claude-code).

Pure Rust binary, distributed through npm so any Claude Code user can install it without a Rust toolchain.

## Quick start

```sh
npx -y @cc-status-line/cli --version
```

Then in `~/.claude/settings.json`:

```json
{
  "statusLine": {
    "type": "command",
    "command": "npx -y @cc-status-line/cli render"
  }
}
```

For best performance install globally so the binary is on PATH:

```sh
npm install -g @cc-status-line/cli
```

Then:

```json
{
  "statusLine": {
    "type": "command",
    "command": "ccs render"
  }
}
```

## Documentation

Full README, architecture, and Chinese usage guide:
<https://github.com/hankeGui/cc-status>
