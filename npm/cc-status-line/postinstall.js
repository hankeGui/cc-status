#!/usr/bin/env node
// Friendly post-install hint. Does NOT touch ~/.claude/settings.json —
// users must run `ccs setup` (or set `command: "npx -y @cc-status-line/cli render"`
// manually) so the change is explicit and auditable.

"use strict";

if (process.env.CI || process.env.npm_config_silent) {
  process.exit(0);
}

const dim = (s) => `\x1b[2m${s}\x1b[0m`;
const bold = (s) => `\x1b[1m${s}\x1b[0m`;
const green = (s) => `\x1b[0;32m${s}\x1b[0m`;
const cyan = (s) => `\x1b[1;36m${s}\x1b[0m`;

const lines = [
  "",
  bold("cc-status-line installed."),
  "",
  `Next step: wire it into Claude Code's ${cyan("~/.claude/settings.json")}.`,
  "",
  "  " + green("ccs setup") + dim("       — interactive: shows the change, prompts for y/N"),
  "  " + green("ccs setup --yes") + dim(" — non-interactive (for scripts)"),
  "  " + green("ccs explain") + dim("     — what every status-line segment means"),
  "  " + green("ccs status") + dim("      — full session dashboard"),
  "",
  dim("Docs: https://github.com/hankeGui/cc-status"),
  "",
];

try {
  process.stdout.write(lines.join("\n"));
} catch (_e) {
  // best-effort; never let a postinstall hint fail an install
}
