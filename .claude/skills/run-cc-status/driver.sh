#!/bin/sh
# driver.sh — agent-facing smoke driver for cc-status.
#
# This script proves the cc-status binary works end-to-end on a fresh
# config. It walks the four workflows the SKILL.md teaches Claude to
# guide users through:
#
#   1. switch mode
#   2. append a segment to a mode
#   3. scaffold + debug-run a plugin
#   4. render with synthetic stdin
#
# Run from anywhere. Uses an isolated $HOME (XDG_*) so the developer's
# real config is untouched. Exits non-zero if any step fails.
#
# Usage:
#   sh .claude/skills/run-cc-status/driver.sh
#   CCS=/path/to/ccs sh .claude/skills/run-cc-status/driver.sh
#
# By default uses ./target/release/ccs (build with `cargo build --release`).

set -eu

CCS="${CCS:-./target/release/ccs}"
if [ ! -x "$CCS" ]; then
    echo "✗ $CCS not found or not executable. Run: cargo build --release" >&2
    exit 1
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# Force every config / cache / data dir into the temp tree so we never
# touch the developer's real settings. Covers macOS (HOME/Library),
# Linux (XDG_*), and Windows-ish (APPDATA).
export HOME="$TMP"
export XDG_CONFIG_HOME="$TMP/config"
export XDG_CACHE_HOME="$TMP/cache"
export XDG_DATA_HOME="$TMP/data"
export APPDATA="$TMP/appdata"
export LOCALAPPDATA="$TMP/localappdata"

step() { printf '\n=== %s ===\n' "$1"; }
ok()   { printf '✓ %s\n' "$1"; }
fail() { printf '✗ %s\n' "$1" >&2; exit 1; }

# --- 1. version + init -----------------------------------------------
step "version + init"
"$CCS" --version
"$CCS" init >/dev/null
ok "fresh config initialized at $($CCS config-path)"

# --- 2. flow A: switch mode ------------------------------------------
step "flow A — switch mode"
"$CCS" mode list | head -3
"$CCS" mode detailed
# Active mode is rendered as `▸ detailed (active)` in the new pretty
# listing. Match on the `(active)` half on the same line as `detailed`
# so we don't depend on terminal escape sequences.
"$CCS" mode list | grep -F 'detailed' | grep -F '(active)' >/dev/null \
    || fail "mode switch did not stick"
ok "switched to detailed"

# --- 3. flow B: append a segment to a mode ---------------------------
step "flow B — append a segment"
"$CCS" mode append --mode compact cost_today
"$CCS" mode list | grep -F '{cost_today}' >/dev/null \
    || fail "appended segment did not appear in mode"
ok "appended {cost_today} to compact"

# --- 4. flow C: scaffold + debug-run a plugin ------------------------
step "flow C — scaffold + debug a plugin"
"$CCS" plugin new ccs-driver-demo --force >/dev/null
"$CCS" plugin list | grep ccs-driver-demo >/dev/null \
    || fail "plugin not listed after scaffold"
out=$("$CCS" plugin run ccs-driver-demo --warm 2>&1)
echo "$out" | grep -F 'As shown in the status line' >/dev/null \
    || fail "plugin run output missing 'As shown in the status line' section"
echo "$out" | grep 'exit:' | grep '0' >/dev/null \
    || fail "scaffolded plugin should exit 0"
ok "scaffolded plugin runs and shows sanitized preview"

# --- 5. flow D: full doctor health check -----------------------------
step "flow D — doctor"
"$CCS" plugin doctor | tee "$TMP/doctor.out"
grep '1 plugin(s):' "$TMP/doctor.out" >/dev/null \
    || fail "doctor summary line missing"
ok "doctor reports the scaffolded plugin"

# --- 6. flow E: render with synthetic stdin --------------------------
step "flow E — render with mock CC stdin"
RENDER_OUT=$(printf '{"cwd":"%s","model":{"display_name":"Claude Opus 4.7"},"context_window":{"remaining_percentage":75},"session_id":"smoke","transcript_path":""}' "$PWD" \
    | "$CCS" render)
[ -n "$RENDER_OUT" ] || fail "render produced empty output"
echo "rendered: $RENDER_OUT"
ok "render produced ANSI status line"

# --- 7. cleanup verified by trap -------------------------------------
printf '\n✓ all flows passed\n'
