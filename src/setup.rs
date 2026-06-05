//! `ccs setup` — write the `statusLine` block into `~/.claude/settings.json`.
//!
//! Behavior:
//!   - Detects whether ccs is being invoked through npx, a global npm
//!     install, or a plain binary install, and picks the right command
//!     to write into settings.json accordingly.
//!   - Backs up settings.json before any write.
//!   - Refuses to overwrite an existing statusLine that points somewhere
//!     other than ccs unless --yes is passed.

use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::io::{self, Write};
use std::path::PathBuf;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[0;33m";
const RED: &str = "\x1b[0;31m";
const DIM: &str = "\x1b[2m";

#[derive(Default)]
pub struct Args {
    pub yes: bool,
    pub check: bool,
    pub uninstall: bool,
}

pub fn run(args: Args) -> Result<()> {
    println!("{B}cc-status setup{R}", B = BOLD, R = RESET);
    println!("{}─────────────────────────────────────{}", DIM, RESET);

    let claude_dir = home_dir()?.join(".claude");
    let settings_path = claude_dir.join("settings.json");

    if !claude_dir.exists() {
        eprintln!(
            "{}✗{} Claude Code config dir not found: {}",
            RED,
            RESET,
            claude_dir.display()
        );
        eprintln!("  Install Claude Code first: https://docs.claude.com/en/docs/claude-code");
        anyhow::bail!("Claude Code is not installed");
    }
    println!(
        "{}✓{} Claude Code config dir: {}",
        GREEN,
        RESET,
        claude_dir.display()
    );

    let mut settings: Map<String, Value> = if settings_path.exists() {
        let text = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("read {}", settings_path.display()))?;
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(m)) => {
                println!("{}✓{} settings.json exists, valid JSON", GREEN, RESET);
                m
            }
            Ok(_) => {
                eprintln!(
                    "{}✗{} settings.json exists but is not a JSON object",
                    RED, RESET
                );
                anyhow::bail!("invalid settings.json");
            }
            Err(e) => {
                eprintln!(
                    "{}✗{} settings.json exists but does not parse: {}",
                    RED, RESET, e
                );
                eprintln!("  Fix the syntax error first, then re-run `ccs setup`.");
                anyhow::bail!("invalid settings.json");
            }
        }
    } else {
        println!(
            "{}○{} settings.json does not exist — will create it",
            YELLOW, RESET
        );
        Map::new()
    };

    if args.uninstall {
        return uninstall(&settings_path, settings);
    }

    let desired_command = pick_command()?;

    let existing = settings.get("statusLine").cloned();
    let already_ours = match &existing {
        Some(Value::Object(o)) => o
            .get("command")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("ccs") || s.contains("@cc-status-line/cli"))
            .unwrap_or(false),
        _ => false,
    };

    if let Some(ref ex) = existing {
        if already_ours {
            println!("{}✓{} statusLine already points to ccs:", GREEN, RESET);
            println!(
                "    {}",
                ex.get("command").and_then(|v| v.as_str()).unwrap_or("")
            );
            if args.check {
                return Ok(());
            }
            if !args.yes {
                print!("\nReplace with `{}`? [y/N]: ", desired_command);
                io::stdout().flush()?;
                let mut buf = String::new();
                io::stdin().read_line(&mut buf)?;
                if !buf.trim().eq_ignore_ascii_case("y") {
                    println!("{}No changes made.{}", DIM, RESET);
                    return Ok(());
                }
            }
        } else {
            println!(
                "{}!{} statusLine is configured but points elsewhere:",
                YELLOW, RESET
            );
            println!(
                "    {}",
                ex.get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(non-string)")
            );
            if args.check {
                return Ok(());
            }
            if !args.yes {
                print!("\nReplace with `{}`? [y/N]: ", desired_command);
                io::stdout().flush()?;
                let mut buf = String::new();
                io::stdin().read_line(&mut buf)?;
                if !buf.trim().eq_ignore_ascii_case("y") {
                    println!("{}No changes made.{}", DIM, RESET);
                    return Ok(());
                }
            }
        }
    } else {
        println!("{}○{} statusLine not configured", YELLOW, RESET);
        if args.check {
            return Ok(());
        }
        println!();
        println!("Proposed change to {}:", settings_path.display());
        println!();
        println!("  + \"statusLine\": {{");
        println!("  +   \"type\": \"command\",");
        println!("  +   \"command\": \"{}\"", desired_command);
        println!("  + }}");
        println!();
        if !args.yes {
            print!("Apply? [Y/n]: ");
            io::stdout().flush()?;
            let mut buf = String::new();
            io::stdin().read_line(&mut buf)?;
            let s = buf.trim();
            if !s.is_empty() && !s.eq_ignore_ascii_case("y") {
                println!("{}No changes made.{}", DIM, RESET);
                return Ok(());
            }
        }
    }

    if args.check {
        return Ok(());
    }

    if settings_path.exists() {
        let backup = backup_path(&settings_path);
        std::fs::copy(&settings_path, &backup)
            .with_context(|| format!("backup {}", backup.display()))?;
        println!("{}✓{} backup: {}", GREEN, RESET, backup.display());
    }

    let mut block = Map::new();
    block.insert("type".into(), Value::String("command".into()));
    block.insert("command".into(), Value::String(desired_command.clone()));
    settings.insert("statusLine".into(), Value::Object(block));

    let new_text = serde_json::to_string_pretty(&Value::Object(settings))? + "\n";
    std::fs::create_dir_all(&claude_dir).ok();
    std::fs::write(&settings_path, new_text)
        .with_context(|| format!("write {}", settings_path.display()))?;
    println!("{}✓{} settings.json updated", GREEN, RESET);
    println!();
    println!("Restart Claude Code to see the new status line.");
    println!();
    println!("{}Useful next steps:{}", DIM, RESET);
    println!("  ccs explain          — what every segment means");
    println!("  ccs status           — full session dashboard");
    println!("  ccs mode list        — available display modes");
    println!("  ccs mode detailed    — switch to a 3-line layout");
    Ok(())
}

fn uninstall(path: &PathBuf, mut settings: Map<String, Value>) -> Result<()> {
    if !settings.contains_key("statusLine") {
        println!(
            "{}○{} statusLine is not configured — nothing to do",
            YELLOW, RESET
        );
        return Ok(());
    }
    let backup = backup_path(path);
    std::fs::copy(path, &backup)?;
    println!("{}✓{} backup: {}", GREEN, RESET, backup.display());

    settings.remove("statusLine");
    let new_text = serde_json::to_string_pretty(&Value::Object(settings))? + "\n";
    std::fs::write(path, new_text)?;
    println!(
        "{}✓{} statusLine removed from {}",
        GREEN,
        RESET,
        path.display()
    );
    Ok(())
}

fn backup_path(path: &PathBuf) -> PathBuf {
    let ts = chrono::Local::now().format("%Y%m%dT%H%M%S");
    path.with_extension(format!("json.bak-{}", ts))
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME not set")
}

/// Decide what command string to put in settings.json.
///
/// Priority:
///   1. If we live inside an npm-installed `@cc-status-line/<platform>`
///      sub-package, walk up the tree to find the npm `bin/ccs` symlink
///      (the JS wrapper in the prefix). That path is stable across
///      reinstalls/version bumps for a given Node version.
///   2. If we appear to be running via npx ephemerally (npm cache
///      directory or npm env vars), write `npx -y @cc-status-line/cli
///      render` so the user doesn't depend on a transient cache path.
///   3. Otherwise (curl install, brew, manual), use the absolute path
///      of the current binary.
fn pick_command() -> Result<String> {
    let exe = std::env::current_exe().context("locate current executable")?;
    let exe_str = exe.to_string_lossy();

    // Case 1: npm global install. We're at:
    //   <prefix>/lib/node_modules/@cc-status-line/cli/node_modules/@cc-status-line/<plat>/bin/ccs
    // The `ccs` JS wrapper lives at <prefix>/bin/ccs (a symlink).
    if exe_str.contains("/node_modules/@cc-status-line/")
        && exe_str.contains("/bin/ccs")
    {
        if let Some(prefix) = npm_prefix_from_module_path(&exe) {
            let wrapper = prefix.join("bin").join("ccs");
            if wrapper.exists() {
                return Ok(format!("{} render", wrapper.display()));
            }
        }
    }

    // Case 2: ephemeral npx run (cache dir or npm env vars).
    let in_npm_cache = exe_str.contains("/_npx/")
        || exe_str.contains("/.npm/_npx/")
        || exe_str.contains("/npm-cache/")
        || exe_str.contains("/_cacache/")
        || exe_str.contains("\\npm-cache\\")
        || exe_str.contains("\\_npx\\");
    let from_npm_env = std::env::var("npm_config_user_agent").is_ok()
        || std::env::var("npm_lifecycle_event").is_ok()
        || std::env::var("npm_package_name").is_ok();
    if in_npm_cache || from_npm_env {
        return Ok("npx -y @cc-status-line/cli render".into());
    }

    // Case 3: plain binary install.
    Ok(format!("{} render", exe.display()))
}

/// Given an exe path inside `<prefix>/lib/node_modules/.../bin/ccs`,
/// return `<prefix>` so we can reach the wrapper at `<prefix>/bin/ccs`.
fn npm_prefix_from_module_path(exe: &std::path::Path) -> Option<PathBuf> {
    // Walk up until we find a directory whose parent contains `lib/node_modules`.
    let mut cur = exe.parent()?.to_path_buf();
    while cur.parent().is_some() {
        let candidate = cur.join("lib").join("node_modules");
        if candidate.is_dir() {
            return Some(cur);
        }
        cur = cur.parent()?.to_path_buf();
    }
    None
}
