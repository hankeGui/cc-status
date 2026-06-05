#!/usr/bin/env bash
# Generate the demo screenshots embedded in README and the GitHub
# Pages site. Requires a working `ccs` on PATH.
#
# Strategy: spit out colored bytes to a real terminal session and use
# `freeze` (https://github.com/charmbracelet/freeze) to render them as
# PNG/SVG. If `freeze` isn't installed, falls back to ASCII captures
# you can paste into README directly.

set -euo pipefail

OUT="${1:-docs/site/img}"
mkdir -p "$OUT"

# Reusable synthetic stdin that hits every interesting segment.
TS="${TRANSCRIPT:-}"
if [ -z "$TS" ]; then
  TS=$(ls -t ~/.claude/projects/*/*.jsonl 2>/dev/null | head -1 || echo "")
fi

stdin_for() {
  local pct="$1" model_name="$2"
  printf '{"cwd":"%s","model":{"display_name":"%s","id":"claude-opus-4-7"},"context_window":{"remaining_percentage":%s},"session_id":"demo","transcript_path":"%s"}' \
    "$PWD" "$model_name" "$pct" "$TS"
}

shot() {
  local mode="$1" name="$2" pct="${3:-86}" model="${4:-Claude Opus 4.7}"
  ccs mode "$mode" >/dev/null
  echo
  echo "==== mode: $mode ===="
  stdin_for "$pct" "$model" | ccs render

  # If `freeze` is on PATH, render to PNG too.
  if command -v freeze >/dev/null 2>&1; then
    local png="$OUT/$name.png"
    stdin_for "$pct" "$model" | ccs render \
      | freeze --output "$png" --language ansi --window=false --padding 14,14 \
      || true
    echo "    → $png"
  fi
}

echo "Building screenshots in $OUT/"
shot compact compact 86
shot detailed detailed 84
shot debug debug 92
shot cost cost 76
shot tokens tokens 88
shot tools tools 82
shot minimal minimal 95

echo
echo "=== ccs status (full panel) ==="
stdin_for 86 "Claude Opus 4.7" | ccs status
if command -v freeze >/dev/null 2>&1; then
  stdin_for 86 "Claude Opus 4.7" | ccs status \
    | freeze --output "$OUT/status.png" --language ansi --window=false --padding 14,14 \
    || true
fi

echo
echo "=== ccs cost --days 7 ==="
ccs cost --days 7
if command -v freeze >/dev/null 2>&1; then
  ccs cost --days 7 \
    | freeze --output "$OUT/cost.png" --language ansi --window=false --padding 14,14 \
    || true
fi

echo
echo "Done. If you don't have \`freeze\` installed:"
echo "  brew install charmbracelet/tap/freeze   # mac"
echo "  go install github.com/charmbracelet/freeze@latest"
