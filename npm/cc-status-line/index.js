#!/usr/bin/env node
// Resolves the platform-specific native binary and execs it, forwarding
// argv / stdin / stdout / stderr / exit code transparently.
//
// Critical: this must be fast. Claude Code calls `ccs render` on every
// status-line refresh; spawning Node + resolving + execing must stay
// well under 100 ms.

"use strict";

const { spawnSync } = require("child_process");
const path = require("path");
const fs = require("fs");

function resolveBinary() {
  const platform = process.platform;
  const arch = process.arch;

  const map = {
    "darwin-arm64": "@cc-status-line/darwin-arm64",
    "darwin-x64": "@cc-status-line/darwin-x64",
    "linux-x64": "@cc-status-line/linux-x64",
    "linux-arm64": "@cc-status-line/linux-arm64",
    "win32-x64": "@cc-status-line/win32-x64",
  };

  const key = `${platform}-${arch}`;
  const pkgName = map[key];
  if (!pkgName) {
    fail(
      `Unsupported platform: ${platform}-${arch}.\n` +
        `Supported: ${Object.keys(map).join(", ")}\n` +
        `Build from source: https://github.com/hankeGui/cc-status`
    );
  }

  const binName = platform === "win32" ? "ccs.exe" : "ccs";

  // Resolve via require.resolve so we use Node's actual module-lookup
  // path (handles workspaces, pnpm, yarn pnp, monorepos correctly).
  let pkgRoot;
  try {
    pkgRoot = path.dirname(require.resolve(`${pkgName}/package.json`));
  } catch (e) {
    fail(
      `The platform package ${pkgName} was not installed.\n` +
        `This usually means optionalDependencies were skipped during install.\n` +
        `Try: npm install --include=optional cc-status-line\n` +
        `Or:  npm install --force cc-status-line`
    );
  }

  const binPath = path.join(pkgRoot, "bin", binName);
  if (!fs.existsSync(binPath)) {
    fail(`Binary not found at ${binPath}`);
  }

  return binPath;
}

function fail(msg) {
  process.stderr.write(`cc-status-line: ${msg}\n`);
  process.exit(1);
}

function main() {
  const bin = resolveBinary();
  const result = spawnSync(bin, process.argv.slice(2), {
    stdio: "inherit",
    windowsHide: true,
  });
  if (result.error) {
    fail(`Failed to spawn ${bin}: ${result.error.message}`);
  }
  process.exit(result.status ?? 1);
}

main();
