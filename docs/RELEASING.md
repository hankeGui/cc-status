# Release & Distribution

How `ccs` is built and shipped, and what one-time setup the maintainer
needs to do before the first release.

## Distribution channels

After a `git tag vX.Y.Z && git push --tags`, GitHub Actions
(`.github/workflows/release.yml`) does:

1. **Build** the binary on 5 targets (matrix job):
   - `aarch64-apple-darwin` (mac M-series)
   - `x86_64-apple-darwin` (mac Intel)
   - `x86_64-unknown-linux-gnu`
   - `aarch64-unknown-linux-gnu` (cross-compiled)
   - `x86_64-pc-windows-msvc`
2. **Upload** to GitHub Releases as `ccs-<target>.tar.gz` / `.zip` plus `checksums.txt`.
3. **Publish** to npm:
   - 5 platform packages: `@cc-status-line/<platform>`
   - 1 main wrapper: `cc-status-line` (with `optionalDependencies` pinned to the same version)
4. **Bump** the Homebrew tap formula (skipped if `HOMEBREW_TAP_REPO` repo variable is unset).

End user install paths (no Rust toolchain needed):

| Channel | Command |
|---|---|
| npm / npx | `npx -y cc-status-line` / `npm i -g cc-status-line` |
| Homebrew | `brew install hankeGui/tap/ccs` |
| curl | `curl -fsSL https://raw.githubusercontent.com/hankeGui/cc-status/main/install.sh \| sh` |
| Direct tarball | from <https://github.com/hankeGui/cc-status/releases> |
| `cargo install --git` | requires Rust toolchain (fallback) |

## One-time maintainer setup

### 1. npm account + token

```sh
# If you don't already have an npm account
npm adduser

# Create an Automation token (scoped to packages, no expiration recommended)
# https://www.npmjs.com/settings/<your-user>/tokens/granular-access-tokens/new
# Permissions:
#   Packages and scopes -> "Read and write"
#   Allowed packages -> cc-status-line, @cc-status-line/*
```

In GitHub: **Settings → Secrets and variables → Actions → New repository secret**
- Name: `NPM_TOKEN`
- Value: the token above

The first `npm publish` will create both the unscoped `cc-status-line`
and the `@cc-status-line/*` scope automatically — npm requires the
scope to be free or owned by you (`hankeGui` is fine, scopes default to
the publishing user).

### 2. (optional) Homebrew tap

```sh
gh repo create hankeGui/homebrew-tap --public --description "Homebrew formulas"
```

In GitHub repo settings:

- Add a **repository variable** `HOMEBREW_TAP_REPO` = `hankeGui/homebrew-tap`
- Add a **secret** `HOMEBREW_TAP_TOKEN` = a fine-grained PAT with `Contents: Read/Write` on the `homebrew-tap` repo

The first time, manually copy `Formula/ccs.rb` to that tap (path
`Formula/ccs.rb`); subsequent releases get bumped automatically.

If `HOMEBREW_TAP_REPO` is unset the brew job is skipped — the rest of
the release still publishes.

### 3. (optional) crates.io

Adds `cargo install cc-status` and `cargo binstall cc-status` as
install paths. Not required for any of the channels above to work.

```sh
cargo login                 # paste your crates.io token
cargo publish --dry-run     # sanity check
cargo publish               # for real
```

Once published, future releases need `cargo publish` re-run; this is
not yet automated in the workflow because crates.io versions are
permanent (so you don't want it to fire on every workflow_dispatch).

## Cutting a release

```sh
# 1. Update version in Cargo.toml
$EDITOR Cargo.toml

# 2. Commit
git add Cargo.toml Cargo.lock
git commit -m "release: vX.Y.Z"
git push origin main

# 3. Tag and push (triggers the workflow)
git tag -a vX.Y.Z -m "vX.Y.Z — <short summary>"
git push origin vX.Y.Z

# 4. Watch the run
gh run watch
```

The npm version follows the tag with the leading `v` stripped: tag
`v0.2.0` → npm `0.2.0`.

## Smoke testing a release

After the workflow finishes:

```sh
# Verify npm
npx -y cc-status-line --version

# Verify GitHub release
curl -fsSL https://raw.githubusercontent.com/hankeGui/cc-status/main/install.sh \
  | sh -s -- --version vX.Y.Z --bin-dir /tmp/ccs-test
/tmp/ccs-test/ccs --version

# Verify Homebrew (if tap is set up)
brew tap hankeGui/tap
brew install ccs
ccs --version
```

## Troubleshooting

**npm publish fails with 403 / 404**: token lacks scope, or the package
doesn't exist yet. The first publish must succeed for each platform
package; if any one fails you'll need to either yank successful ones
(within 72h, before downloads) or just bump the patch version and retry.

**`@cc-status-line/<platform>` not found at runtime**: user installed
with `--no-optional`, or in an environment that strips
`optionalDependencies`. The npm wrapper's `index.js` prints a hint with
the correct command.

**Cross-compile (linux-arm64) fails**: usually a libc version issue.
The workflow uses `houseabsolute/actions-rust-cross` which uses `cross`
under the hood with a manylinux-style sysroot. Bump that action if you
hit a stale toolchain.

**Homebrew formula sha256 wrong**: `mislav/bump-homebrew-formula-action`
fetches the new tarball and computes sha256 itself; if it errored, the
formula stays at the previous version. Re-run the workflow.
