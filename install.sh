#!/usr/bin/env sh
# cc-status installer.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/hankeGui/cc-status/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/hankeGui/cc-status/main/install.sh | sh -s -- --version v0.1.0
#   curl -fsSL https://raw.githubusercontent.com/hankeGui/cc-status/main/install.sh | sh -s -- --bin-dir ~/bin
#
# Detects platform, downloads the matching tarball from GitHub Releases,
# unpacks it, and installs `ccs` into ~/.local/bin (or --bin-dir).

set -eu

REPO="hankeGui/cc-status"
VERSION="latest"
BIN_DIR="${HOME}/.local/bin"

usage() {
  cat <<EOF
cc-status installer

Options:
  --version <tag>     Install a specific version (default: latest)
  --bin-dir <path>    Install directory (default: ~/.local/bin)
  -h, --help          Show this help

Environment:
  CCS_VERSION         Same as --version
  CCS_BIN_DIR         Same as --bin-dir
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --bin-dir) BIN_DIR="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

[ -n "${CCS_VERSION:-}" ] && VERSION="$CCS_VERSION"
[ -n "${CCS_BIN_DIR:-}" ] && BIN_DIR="$CCS_BIN_DIR"

# --- Detect platform -------------------------------------------------------
detect_target() {
  uname_s=$(uname -s)
  uname_m=$(uname -m)
  case "${uname_s}_${uname_m}" in
    Darwin_arm64)         echo "aarch64-apple-darwin" ;;
    Darwin_x86_64)        echo "x86_64-apple-darwin" ;;
    Linux_x86_64)         echo "x86_64-unknown-linux-gnu" ;;
    Linux_aarch64|Linux_arm64) echo "aarch64-unknown-linux-gnu" ;;
    *)
      echo "unsupported platform: ${uname_s} ${uname_m}" >&2
      echo "build from source: https://github.com/${REPO}#install-from-source" >&2
      exit 1
      ;;
  esac
}

TARGET=$(detect_target)
ARCHIVE="ccs-${TARGET}.tar.gz"

# --- Resolve version -------------------------------------------------------
if [ "$VERSION" = "latest" ]; then
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep -o '"tag_name":\s*"[^"]\+"' \
    | head -n1 \
    | cut -d'"' -f4)
  if [ -z "$VERSION" ]; then
    echo "could not resolve latest version. set --version explicitly." >&2
    exit 1
  fi
fi

URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"

# --- Download + extract ----------------------------------------------------
echo "Downloading ${URL}"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

if ! curl -fsSL "$URL" -o "${TMP}/${ARCHIVE}"; then
  echo "download failed: ${URL}" >&2
  exit 1
fi

tar -xzf "${TMP}/${ARCHIVE}" -C "${TMP}"

if [ ! -f "${TMP}/ccs" ]; then
  echo "binary not found in archive" >&2
  exit 1
fi

# --- Install ---------------------------------------------------------------
mkdir -p "${BIN_DIR}"
install -m 0755 "${TMP}/ccs" "${BIN_DIR}/ccs"

echo
echo "Installed ${VERSION} to ${BIN_DIR}/ccs"
"${BIN_DIR}/ccs" --version || true

# --- PATH hint -------------------------------------------------------------
case ":${PATH}:" in
  *":${BIN_DIR}:"*) ;;
  *)
    echo
    echo "NOTE: ${BIN_DIR} is not on your PATH."
    echo "Add this line to your shell rc file:"
    echo "  export PATH=\"${BIN_DIR}:\$PATH\""
    ;;
esac

echo
echo "─────────────────────────────────────────────────────────"
echo "Next: wire ccs into Claude Code's ~/.claude/settings.json."
echo "─────────────────────────────────────────────────────────"
echo
echo "Run this now:"
echo "  ${BIN_DIR}/ccs setup"
echo
echo "It will:"
echo "  • check ~/.claude/settings.json"
echo "  • show the proposed change"
echo "  • back the file up before writing"
echo "  • prompt for y/N (or pass --yes to skip)"
echo
echo "Other handy commands:"
echo "  ${BIN_DIR}/ccs explain     — what every status-line segment means"
echo "  ${BIN_DIR}/ccs status      — full session dashboard"
echo "  ${BIN_DIR}/ccs --help      — show all subcommands"
echo
echo "Docs: https://github.com/${REPO}"
