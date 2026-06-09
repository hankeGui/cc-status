//! `ccs upgrade` — detect how the running binary was installed and run
//! the matching update command. Best-effort: prints what it would do
//! and asks for confirmation, unless `--yes`.

use anyhow::{Context, Result};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[0;33m";
const DIM: &str = "\x1b[90m";

#[derive(Default)]
pub struct Args {
    pub yes: bool,
    pub check: bool,
}

#[derive(Debug, Clone)]
pub enum Source {
    Npm,
    Cargo,
    LocalBin,
    Brew,
    Unknown,
}

impl Source {
    fn label(&self) -> &'static str {
        match self {
            Source::Npm => "npm global install",
            Source::Cargo => "cargo install",
            Source::LocalBin => "curl install.sh / manual binary",
            Source::Brew => "Homebrew",
            Source::Unknown => "unknown",
        }
    }
}

pub fn run(args: Args) -> Result<()> {
    println!("{B}cc-status upgrade{R}", B = BOLD, R = RESET);
    println!("{}─────────────────{}", DIM, RESET);

    let exe = std::env::current_exe().context("locate current executable")?;
    let exe_str = exe.to_string_lossy().to_string();
    println!("{}current binary: {}{}", DIM, exe_str, RESET);

    let source = detect_source(&exe);
    println!("{}detected source: {}{}", GREEN, source.label(), RESET);
    println!();

    let recipe = upgrade_recipe(&source);
    let Some((shell_cmd, description)) = recipe else {
        println!(
            "{}Don't know how to upgrade this install. Re-run the original install command:{}",
            YELLOW, RESET
        );
        println!("  npm install -g @cc-status-line/cli@latest");
        println!("  curl -fsSL https://raw.githubusercontent.com/hankeGui/cc-status/main/install.sh | sh");
        println!("  brew upgrade hankeGui/tap/ccs");
        println!("  cargo install --git https://github.com/hankeGui/cc-status --locked --force");
        return Ok(());
    };

    println!("Will run:");
    println!("  {}", shell_cmd);
    println!("  {}({}){}", DIM, description, RESET);
    println!();

    if args.check {
        return Ok(());
    }

    if !args.yes {
        print!("Proceed? [Y/n]: ");
        io::stdout().flush()?;
        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        let s = buf.trim();
        if !s.is_empty() && !s.eq_ignore_ascii_case("y") {
            println!("{}aborted.{}", DIM, RESET);
            return Ok(());
        }
    }

    let status = Command::new("sh")
        .arg("-c")
        .arg(&shell_cmd)
        .status()
        .with_context(|| format!("failed to run: {}", shell_cmd))?;
    if !status.success() {
        anyhow::bail!("upgrade command exited with {}", status);
    }

    // Verify
    if let Ok(out) = Command::new(&exe).arg("--version").output() {
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        println!();
        println!("{}✓{} now: {}", GREEN, RESET, v);
    }
    Ok(())
}

/// Heuristic detection of how the binary was installed, based on its
/// path. Robust enough for the common cases without prying into
/// package manager state.
pub fn detect_source(exe: &PathBuf) -> Source {
    let s = exe.to_string_lossy();

    // npm global install: lives under .../node_modules/@cc-status-line/<plat>/bin/
    // OR under <prefix>/bin/ccs as a wrapper symlink. Either way we
    // expect "@cc-status-line" or "node_modules" in the path.
    if s.contains("/node_modules/@cc-status-line/")
        || s.contains("/node_modules/.bin/ccs")
        || (s.contains("/node/") && s.contains("/bin/ccs"))
    {
        return Source::Npm;
    }

    if s.contains("/.cargo/bin/") {
        return Source::Cargo;
    }

    if s.contains("/Cellar/") || s.contains("/Homebrew/") || s.contains("/opt/homebrew/") {
        return Source::Brew;
    }

    if s.contains("/.local/bin/") || s.contains("/usr/local/bin/") || s.ends_with("/bin/ccs") {
        return Source::LocalBin;
    }

    Source::Unknown
}

fn upgrade_recipe(source: &Source) -> Option<(String, &'static str)> {
    match source {
        Source::Npm => Some((
            "npm install -g @cc-status-line/cli@latest".into(),
            "uses your active npm registry — set CCS_REGISTRY env to override",
        )),
        Source::Cargo => Some((
            "cargo install --git https://github.com/hankeGui/cc-status --locked --force".into(),
            "rebuilds from source at HEAD; pass --tag vX.Y.Z to pin",
        )),
        Source::Brew => Some((
            "brew upgrade hankeGui/tap/ccs".into(),
            "uses your Homebrew tap",
        )),
        Source::LocalBin => Some((
            "curl -fsSL https://raw.githubusercontent.com/hankeGui/cc-status/main/install.sh | sh"
                .into(),
            "re-runs the install.sh which always pulls latest",
        )),
        Source::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detect_npm_global() {
        let p = PathBuf::from(
            "/Users/me/.nvm/versions/node/v22.0.0/lib/node_modules/@cc-status-line/cli/bin/ccs.js",
        );
        assert!(matches!(detect_source(&p), Source::Npm));
    }

    #[test]
    fn detect_npm_wrapper() {
        let p = PathBuf::from("/Users/me/.nvm/versions/node/v22.0.0/bin/ccs");
        assert!(matches!(detect_source(&p), Source::Npm));
    }

    #[test]
    fn detect_cargo() {
        let p = PathBuf::from("/Users/me/.cargo/bin/ccs");
        assert!(matches!(detect_source(&p), Source::Cargo));
    }

    #[test]
    fn detect_brew_arm() {
        let p = PathBuf::from("/opt/homebrew/Cellar/ccs/0.2.0/bin/ccs");
        assert!(matches!(detect_source(&p), Source::Brew));
    }

    #[test]
    fn detect_local_bin() {
        let p = PathBuf::from("/Users/me/.local/bin/ccs");
        assert!(matches!(detect_source(&p), Source::LocalBin));
    }

    #[test]
    fn detect_unknown() {
        let p = PathBuf::from("/random/path/somewhere/ccs");
        assert!(matches!(detect_source(&p), Source::Unknown));
    }
}
