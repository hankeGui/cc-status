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
    /// Install / uninstall the conversational helper skill at
    /// `~/.claude/skills/cc-status/`. None = ask the user (when interactive)
    /// or follow defaults (skip on --check, install on --yes).
    pub skill: Option<bool>,
}

/// Bundled at compile time so the same skill content ships with every
/// install method (npm, curl, brew). `include_str!` paths are relative
/// to this source file.
const SKILL_MD: &str = include_str!("../.claude/skills/run-cc-status/SKILL.md");
const SKILL_DRIVER: &str = include_str!("../.claude/skills/run-cc-status/driver.sh");

/// User-facing skill location. We use a stable, human-readable name
/// (`cc-status`) rather than the dev-time `run-cc-status` because the
/// installed skill is for *operating* cc-status, not "running this
/// repo as a unit." The driver path inside SKILL.md is only correct
/// during repo development; the installed copy of SKILL.md gets a
/// short header pointing at the new path.
const INSTALLED_SKILL_DIRNAME: &str = "cc-status";

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
        let r = uninstall(&settings_path, settings);
        // Always attempt to remove the skill on uninstall — quiet no-op
        // if it isn't there.
        let _ = uninstall_skill(&claude_dir);
        return r;
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

    // Skill installation. Default: ask the user when interactive.
    let install_skill = decide_skill_install(&args)?;
    if install_skill {
        install_skill_files(&claude_dir)?;
    }

    println!("Restart Claude Code to see the new status line.");
    println!();
    println!("{}Useful next steps:{}", DIM, RESET);
    println!("  ccs explain          — what every segment means");
    println!("  ccs status           — full session dashboard");
    println!("  ccs mode list        — available display modes");
    println!("  ccs mode detailed    — switch to a 3-line layout");
    if install_skill {
        println!();
        println!(
            "{}Conversational helper installed.{} Inside Claude Code, just say things like",
            DIM, RESET
        );
        println!("  \"switch my status line to detailed\"");
        println!("  \"add today's cost to my status bar\"");
        println!("  \"build me a plugin that shows the unread issue count\"");
        println!("Claude will pick up the skill and run the right commands for you.");
    }
    Ok(())
}

/// Return true if the user wants the skill installed. Honors:
///   * Explicit `--skill / --no-skill` flag (`args.skill = Some(...)`)
///   * `--yes` (non-interactive: install)
///   * Otherwise prompt y/N, default = yes.
fn decide_skill_install(args: &Args) -> Result<bool> {
    if let Some(b) = args.skill {
        return Ok(b);
    }
    if args.yes {
        return Ok(true);
    }
    print!(
        "Install the conversational helper skill (~/.claude/skills/{}/)?\n  Lets Claude help users switch modes / add segments / build plugins from\n  inside a Claude Code conversation. [Y/n]: ",
        INSTALLED_SKILL_DIRNAME
    );
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    let s = buf.trim();
    Ok(s.is_empty() || s.eq_ignore_ascii_case("y"))
}

fn install_skill_files(claude_dir: &std::path::Path) -> Result<()> {
    let skill_dir = claude_dir.join("skills").join(INSTALLED_SKILL_DIRNAME);
    std::fs::create_dir_all(&skill_dir)
        .with_context(|| format!("create {}", skill_dir.display()))?;

    // The bundled SKILL.md is written for someone working *inside the
    // repo* — its driver path is `./target/release/ccs` (and `sh
    // .claude/skills/run-cc-status/driver.sh`). On a user's machine
    // those paths don't exist; the binary is on PATH as `ccs` and the
    // driver lives next to SKILL.md. Rewrite both references.
    let driver_user_path = skill_dir.join("driver.sh");
    let user_skill_md = SKILL_MD
        .replace(
            "sh .claude/skills/run-cc-status/driver.sh",
            &format!("sh {}", driver_user_path.display()),
        )
        .replace("CCS=/path/to/ccs", "CCS=ccs")
        .replace(
            "default uses ./target/release/ccs (build with `cargo build --release`)",
            "uses `ccs` from the user's PATH (installed by npm / curl / brew / cargo)",
        )
        // Catch any stragglers — repo-internal paths that survive the
        // targeted replaces above. The user runs `ccs` from PATH; the
        // skill must never tell them to look under `target/release/`.
        .replace("./target/release/ccs", "ccs")
        .replace(
            "`cargo build --release` (in the repo root)",
            "reinstall `ccs` (e.g. `npm install -g @cc-status-line/cli` or `ccs upgrade`)",
        );

    let skill_md = skill_dir.join("SKILL.md");
    std::fs::write(&skill_md, user_skill_md)
        .with_context(|| format!("write {}", skill_md.display()))?;

    // Driver also needs its `CCS=...` default to point at PATH and
    // its usage example to reference the installed location.
    let user_driver = SKILL_DRIVER
        .replace(
            "CCS=\"${CCS:-./target/release/ccs}\"",
            "CCS=\"${CCS:-ccs}\"",
        )
        .replace(
            "sh .claude/skills/run-cc-status/driver.sh",
            &format!("sh {}", driver_user_path.display()),
        )
        .replace(
            "By default uses ./target/release/ccs (build with `cargo build --release`).",
            "By default uses `ccs` from your PATH (installed by npm / curl / brew / cargo).",
        );
    std::fs::write(&driver_user_path, user_driver)
        .with_context(|| format!("write {}", driver_user_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&driver_user_path)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&driver_user_path, perm)?;
    }
    println!(
        "{}✓{} skill installed at {}",
        GREEN,
        RESET,
        skill_dir.display()
    );
    Ok(())
}

fn uninstall_skill(claude_dir: &std::path::Path) -> Result<()> {
    let skill_dir = claude_dir.join("skills").join(INSTALLED_SKILL_DIRNAME);
    if !skill_dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&skill_dir)
        .with_context(|| format!("remove {}", skill_dir.display()))?;
    println!(
        "{}✓{} skill removed from {}",
        GREEN,
        RESET,
        skill_dir.display()
    );
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
    if exe_str.contains("/node_modules/@cc-status-line/") && exe_str.contains("/bin/ccs") {
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
