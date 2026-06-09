//! Integration tests for the `ccs` CLI.
//!
//! Each test sets `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` to a temp dir
//! so the test does not touch the developer's real config.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

fn ccs(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("ccs").unwrap();
    c.env("XDG_CONFIG_HOME", tmp.path().join("config"))
        .env("XDG_CACHE_HOME", tmp.path().join("cache"))
        .env("XDG_DATA_HOME", tmp.path().join("data"))
        // macOS picks paths from $HOME's Library; Windows picks them
        // from %APPDATA% / %LOCALAPPDATA%. Override all three so
        // every test gets a fresh, isolated config + cache directory
        // regardless of platform.
        .env("HOME", tmp.path())
        .env("APPDATA", tmp.path().join("appdata"))
        .env("LOCALAPPDATA", tmp.path().join("localappdata"))
        // Drop any user-level NPM env so setup picks the binary path branch.
        .env_remove("npm_config_user_agent")
        .env_remove("npm_lifecycle_event")
        .env_remove("npm_package_name")
        .env_remove("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE");
    c
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn version_flag() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("ccs"));
}

#[test]
fn explain_lists_segments() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp).arg("explain").assert().success().stdout(
        predicate::str::contains("legend")
            .and(predicate::str::contains("ctx"))
            .and(predicate::str::contains("cache")),
    );
}

#[test]
fn segments_command_lists_known_segments() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp).arg("segments").assert().success().stdout(
        predicate::str::contains("{dir}")
            .and(predicate::str::contains("{git}"))
            .and(predicate::str::contains("{ctx}"))
            .and(predicate::str::contains("{last_turn}"))
            .and(predicate::str::contains("{hit_rate}"))
            .and(predicate::str::contains("{burn}")),
    );
}

#[test]
fn render_with_minimal_stdin_prints_at_least_dir() {
    let tmp = TempDir::new().unwrap();
    let stdin = json!({
        "cwd": tmp.path().to_string_lossy(),
        "model": {"display_name": "TestModel"},
        "context_window": {"remaining_percentage": 80},
        "session_id": "itest"
    })
    .to_string();
    ccs(&tmp)
        .arg("render")
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::str::contains("TestModel"));
}

#[test]
fn render_uses_transcript_for_token_data() {
    let tmp = TempDir::new().unwrap();
    let transcript = fixture_path("three-turn-session.jsonl");
    let stdin = json!({
        "cwd": tmp.path().to_string_lossy(),
        "model": {"display_name": "X"},
        "context_window": {"remaining_percentage": 50},
        "session_id": "itest",
        "transcript_path": transcript.to_string_lossy()
    })
    .to_string();
    // Switch to a mode that surfaces ctx (and last_turn) so we can assert on it.
    ccs(&tmp).args(["mode", "detailed"]).assert().success();

    ccs(&tmp)
        .arg("render")
        .write_stdin(stdin)
        .assert()
        .success()
        .stdout(predicate::str::contains("ctx"));
}

#[test]
fn mode_add_list_set_rm_round_trip() {
    let tmp = TempDir::new().unwrap();

    // Add a custom mode
    ccs(&tmp)
        .args(["mode", "add", "mine", "-l", "{dir} {ctx}"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mode 'mine' saved"));

    // List should include it
    ccs(&tmp)
        .args(["mode", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mine"));

    // Switch to it
    ccs(&tmp)
        .args(["mode", "mine"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mode -> mine"));

    // Remove it (and verify auto-switch back to another mode)
    ccs(&tmp)
        .args(["mode", "rm", "mine"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed mode 'mine'"));
}

#[test]
fn mode_add_warns_on_unknown_segment() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["mode", "add", "bad", "-l", "{dir} {nonexistent}"])
        .assert()
        .success()
        .stderr(predicate::str::contains("unknown segment"));
}

#[test]
fn mode_set_rejects_unknown() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["mode", "does-not-exist"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown mode"));
}

#[test]
fn mode_append_creates_new_line() {
    let tmp = TempDir::new().unwrap();
    // Append to the current default mode and confirm the new segment
    // round-trips through `mode list`. The exact mode name / line
    // count is intentionally NOT asserted so this stays robust against
    // changes to the default config.
    ccs(&tmp)
        .args(["mode", "append", "cost_today"])
        .assert()
        .success()
        .stdout(predicate::str::contains("appended new line"));

    let assert = ccs(&tmp).args(["mode", "list"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(stdout.contains("{cost_today}"), "got: {}", stdout);
}

#[test]
fn mode_append_to_existing_line() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["mode", "append", "--line", "1", "hit_rate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("line 1"));
}

#[test]
fn mode_list_renders_example_alongside_template() {
    // The new mode-list format pairs each `template` line with an
    // `example` line rendered from synthetic data. Asserting on
    // visible labels keeps the test stable even if mock numbers drift.
    let tmp = TempDir::new().unwrap();
    let assert = ccs(&tmp).args(["mode", "list"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("template"),
        "mode list should label template lines, got: {}",
        stdout
    );
    assert!(
        stdout.contains("example"),
        "mode list should label example lines, got: {}",
        stdout
    );
    // Pretty header + footer hint should both be present so users
    // can see how to take the next action.
    assert!(stdout.contains("cc-status modes"));
    assert!(stdout.contains("ccs mode <name>"));
    assert!(stdout.contains("ccs plugin new"));
}

#[test]
fn mode_list_marks_active_mode() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp).args(["mode", "detailed"]).assert().success();
    let assert = ccs(&tmp).args(["mode", "list"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    // The active marker is `▸ <name> (active)` — assert both halves
    // appear, but on the same logical block (we don't enforce char
    // adjacency to allow ANSI to sit between them).
    assert!(
        stdout.contains("▸") && stdout.contains("(active)"),
        "active mode marker missing, got: {}",
        stdout
    );
}

#[test]
fn mode_append_multi_segments_same_line() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["mode", "append", "hit_rate", "burn", "cost_today"])
        .assert()
        .success();

    let assert = ccs(&tmp).args(["mode", "list"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("{hit_rate} {burn} {cost_today}"),
        "got: {}",
        stdout
    );
}

#[test]
fn mode_append_warns_on_unknown() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["mode", "append", "no_such_segment"])
        .assert()
        .success()
        .stderr(predicate::str::contains("unknown segment"));
}

#[test]
fn mode_append_rejects_invalid_line_index() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["mode", "append", "--line", "99", "ctx"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("out of range"));
}

#[test]
fn mode_edit_replays_lines_from_editor() {
    let tmp = TempDir::new().unwrap();
    // Fake editor: append two lines so we can verify `edit` parses them.
    let fake = tmp.path().join("fake-editor.sh");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho '{cost_today}' >> \"$1\"\necho '{cost_session}' >> \"$1\"\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&fake).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&fake, perm).unwrap();

    ccs(&tmp)
        .env("EDITOR", &fake)
        .args(["mode", "edit", "compact"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated"));

    let assert = ccs(&tmp).args(["mode", "list"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(stdout.contains("{cost_today}"), "got: {}", stdout);
    assert!(stdout.contains("{cost_session}"), "got: {}", stdout);
}

#[test]
fn presets_are_present_after_init() {
    let tmp = TempDir::new().unwrap();
    let assert = ccs(&tmp).args(["mode", "list"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    for preset in &[
        "balanced", "compact", "minimal", "detailed", "cost", "tokens", "tools", "debug",
    ] {
        assert!(
            stdout.contains(preset),
            "preset {} missing from `mode list`: {}",
            preset,
            stdout
        );
    }
}

#[test]
fn config_path_prints_resolvable_path() {
    let tmp = TempDir::new().unwrap();
    let assert = ccs(&tmp).arg("config-path").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    let path = stdout.trim();
    assert!(!path.is_empty());
    assert!(path.contains("cc-status"));
}

#[test]
fn setup_check_reports_missing_when_no_settings() {
    let tmp = TempDir::new().unwrap();
    // No ~/.claude exists yet
    ccs(&tmp)
        .args(["setup", "--check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Claude Code config dir not found"));
}

#[test]
fn setup_yes_writes_statusline_and_backs_up() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let settings = claude_dir.join("settings.json");
    std::fs::write(&settings, r#"{"someUnrelated": true}"#).unwrap();

    ccs(&tmp).args(["setup", "--yes"]).assert().success();

    let written = std::fs::read_to_string(&settings).unwrap();
    assert!(written.contains("statusLine"));
    // The command path is platform-specific (Unix: `/path/to/ccs render`,
    // Windows: `C:\\path\\to\\ccs.exe render`). Just check the shape.
    assert!(
        written.contains("\"command\""),
        "should write a command field"
    );
    assert!(
        written.contains("ccs"),
        "command should reference the ccs binary"
    );
    assert!(
        written.contains("someUnrelated"),
        "setup must preserve other keys"
    );

    // backup should exist
    let entries: Vec<_> = std::fs::read_dir(&claude_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("bak-"))
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one backup file");
}

#[test]
fn setup_uninstall_removes_statusline() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let settings = claude_dir.join("settings.json");
    std::fs::write(
        &settings,
        r#"{"statusLine":{"type":"command","command":"ccs render"},"keep":1}"#,
    )
    .unwrap();

    ccs(&tmp).args(["setup", "--uninstall"]).assert().success();

    let written = std::fs::read_to_string(&settings).unwrap();
    assert!(!written.contains("statusLine"));
    assert!(written.contains("keep"));
}

// --- skill installation -------------------------------------------------

#[test]
fn setup_with_skill_installs_skill_files() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    ccs(&tmp)
        .args(["setup", "--yes", "--with-skill"])
        .assert()
        .success();

    let skill_md = claude_dir.join("skills/cc-status/SKILL.md");
    let driver = claude_dir.join("skills/cc-status/driver.sh");
    assert!(skill_md.is_file(), "SKILL.md should be installed");
    assert!(driver.is_file(), "driver.sh should be installed");

    let body = std::fs::read_to_string(&skill_md).unwrap();
    assert!(body.starts_with("---\n"), "SKILL.md needs frontmatter");
    assert!(body.contains("name: run-cc-status"));
    assert!(body.contains("description:"));
    // The description must include verbs Claude will match against —
    // this is what makes the skill auto-load when a user asks to
    // change their status line.
    assert!(
        body.contains("switch") || body.contains("status line"),
        "description should contain user-intent keywords"
    );
    // Repo-internal driver path must be rewritten to the installed
    // location — leaving "./target/release/ccs" or
    // ".claude/skills/run-cc-status/" in the user-facing copy would
    // make the driver instructions point at non-existent paths.
    assert!(
        !body.contains("./target/release/ccs"),
        "installed SKILL.md must not reference the repo's build path"
    );
    assert!(
        !body.contains(".claude/skills/run-cc-status/driver.sh"),
        "installed SKILL.md must not reference the dev-time skill dir"
    );
    assert!(
        body.contains("driver.sh"),
        "installed SKILL.md must still reference driver.sh by name"
    );

    // Driver itself: default CCS must be `ccs` on PATH, not the repo
    // build dir.
    let driver_body = std::fs::read_to_string(&driver).unwrap();
    assert!(
        driver_body.contains("CCS=\"${CCS:-ccs}\""),
        "installed driver should default to `ccs` on PATH"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&driver).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o111,
            0o111,
            "driver.sh must be executable, got {:o}",
            mode
        );
    }
}

#[test]
fn setup_no_skill_skips_skill_install() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    ccs(&tmp)
        .args(["setup", "--yes", "--no-skill"])
        .assert()
        .success();

    let skill_dir = claude_dir.join("skills/cc-status");
    assert!(
        !skill_dir.exists(),
        "skill dir must NOT be created with --no-skill"
    );
}

#[test]
fn setup_uninstall_also_removes_skill() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    // Install with skill
    ccs(&tmp)
        .args(["setup", "--yes", "--with-skill"])
        .assert()
        .success();
    let skill_md = claude_dir.join("skills/cc-status/SKILL.md");
    assert!(skill_md.is_file());

    // Now uninstall — skill should be removed too
    ccs(&tmp).args(["setup", "--uninstall"]).assert().success();
    assert!(
        !skill_md.exists(),
        "SKILL.md must be removed by --uninstall"
    );
    assert!(
        !claude_dir.join("skills/cc-status").exists(),
        "skill dir must be removed by --uninstall"
    );
}

#[test]
fn setup_with_skill_overwrites_stale_skill_md() {
    // Simulates the upgrade flow end-to-end via the CLI: a previously
    // installed skill is replaced with the bundled copy, not merged.
    // This is the contract `ccs upgrade` relies on for
    // `refresh_skill_if_installed()`.
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    let skill_dir = claude_dir.join("skills/cc-status");
    std::fs::create_dir_all(&skill_dir).unwrap();
    let skill_md = skill_dir.join("SKILL.md");
    std::fs::write(&skill_md, "OLD VERSION\n").unwrap();

    ccs(&tmp)
        .args(["setup", "--yes", "--with-skill"])
        .assert()
        .success();

    let body = std::fs::read_to_string(&skill_md).unwrap();
    assert!(
        body.starts_with("---\n"),
        "stale SKILL.md must be overwritten with frontmatter copy"
    );
    assert!(!body.contains("OLD VERSION"));
}

#[test]
fn setup_with_skill_and_no_skill_are_mutually_exclusive() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    // clap should reject both flags together.
    ccs(&tmp)
        .args(["setup", "--yes", "--with-skill", "--no-skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used"));
}

// --- plugin subcommand ----------------------------------------------------

#[test]
fn plugin_path_prints_directory() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("plugins"));
}

#[test]
fn plugin_list_in_empty_dir_prints_quickstart() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "list"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Quickstart").and(predicate::str::contains("ccs plugin new")),
        );
}

#[test]
fn plugin_new_creates_executable_sh_file() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "new", "hello-sh"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Next steps")
                .and(predicate::str::contains("ccs plugin run hello-sh"))
                .and(predicate::str::contains("plugin:hello-sh")),
        );

    // The file must exist, start with the sh shebang, and (on Unix) be exec.
    let path = ccs_plugin_path(&tmp).join("hello-sh");
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.starts_with("#!/bin/sh\n"));
    assert!(body.contains("hello-sh"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o111,
            0o111,
            "file must be executable, got {:o}",
            mode
        );
    }
}

#[test]
fn plugin_new_python_lang_writes_python_template() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "new", "hello-py", "--lang", "python"])
        .assert()
        .success();
    let path = ccs_plugin_path(&tmp).join("hello-py");
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.starts_with("#!/usr/bin/env python3\n"));
    assert!(body.contains("import json"));
}

#[test]
fn plugin_new_refuses_existing_without_force() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "new", "twice"])
        .assert()
        .success();
    ccs(&tmp)
        .args(["plugin", "new", "twice"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
    // --force overwrites
    ccs(&tmp)
        .args(["plugin", "new", "twice", "--force", "--lang", "python"])
        .assert()
        .success();
    let body = std::fs::read_to_string(ccs_plugin_path(&tmp).join("twice")).unwrap();
    assert!(body.contains("python3"));
}

#[test]
fn plugin_new_rejects_invalid_names() {
    let tmp = TempDir::new().unwrap();
    for bad in [".hidden", "..", "a/b", "a b"] {
        ccs(&tmp)
            .args(["plugin", "new", bad])
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid plugin name"));
    }
}

#[test]
fn plugin_list_marks_executable_status() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "new", "shown"])
        .assert()
        .success();
    ccs(&tmp)
        .args(["plugin", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("shown").and(predicate::str::contains("plugin:shown")));
}

#[cfg(unix)]
#[test]
fn plugin_run_executes_and_reports_stdout() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "new", "echoer"])
        .assert()
        .success();
    // Replace template body with a deterministic line we can grep.
    let path = ccs_plugin_path(&tmp).join("echoer");
    std::fs::write(&path, "#!/bin/sh\nprintf 'PLUGIN_OK_MARK'\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&path, perm).unwrap();

    ccs(&tmp)
        .args(["plugin", "run", "echoer"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("PLUGIN_OK_MARK")
                .and(predicate::str::contains("exit"))
                .and(predicate::str::contains("As shown in the status line")),
        );
}

#[test]
fn plugin_run_missing_plugin_errors() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "run", "nope-xyz-123"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no such plugin"));
}

#[test]
fn plugin_doctor_empty_dir_explains() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("plugin doctor"));
}

#[cfg(unix)]
#[test]
fn plugin_doctor_reports_orphan_plugin() {
    // A plugin file that exists on disk but isn't referenced by any
    // mode should be flagged as a warning, not a failure.
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "new", "lonely"])
        .assert()
        .success();
    ccs(&tmp)
        .args(["plugin", "doctor"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("lonely")
                .and(predicate::str::contains("not referenced by any mode")),
        );
}

#[cfg(unix)]
#[test]
fn plugin_doctor_credits_referenced_plugin() {
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "new", "wired"])
        .assert()
        .success();
    ccs(&tmp)
        .args(["mode", "append", "plugin:wired"])
        .assert()
        .success();
    ccs(&tmp)
        .args(["plugin", "doctor"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("wired")
                .and(predicate::str::contains("referenced by mode(s):")),
        );
}

#[cfg(unix)]
#[test]
fn plugin_doctor_flags_non_executable() {
    // Drop a file in the plugins dir without exec bit; doctor should
    // call it out.
    let tmp = TempDir::new().unwrap();
    let dir = ccs_plugin_path(&tmp);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("noexec"), "#!/bin/sh\nprintf x\n").unwrap();
    // Deliberately do NOT chmod +x.
    ccs(&tmp)
        .args(["plugin", "doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not executable"));
}

#[cfg(unix)]
#[test]
fn plugin_doctor_flags_slow_plugin() {
    // Plugin that sleeps past the 250ms render budget. Doctor should
    // grade it as fail and surface the timing message; the summary
    // line must show at least one fail.
    let tmp = TempDir::new().unwrap();
    let dir = ccs_plugin_path(&tmp);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sleeper");
    std::fs::write(&path, "#!/bin/sh\nsleep 0.4\nprintf 'late'\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&path, perm).unwrap();

    ccs(&tmp)
        .args(["plugin", "doctor"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("sleeper")
                .and(predicate::str::contains("exceeds 250 ms"))
                .and(predicate::str::contains("1 fail")),
        );
}

#[cfg(unix)]
#[test]
fn plugin_run_warm_avoids_cold_start_warning() {
    // Without --warm, a fresh sh script's first execution often crosses
    // 250ms on macOS due to Gatekeeper. With --warm, the discarded run
    // primes the cache so the reported elapsed should not show the
    // "would TIME OUT" suffix. We assert the absence rather than a
    // strict timing bound (CI machines vary wildly).
    let tmp = TempDir::new().unwrap();
    ccs(&tmp)
        .args(["plugin", "new", "warmable"])
        .assert()
        .success();

    // Touch the plugin once via --warm; the second run inside the same
    // ccs invocation should not flag a cold-start timeout.
    let out = ccs(&tmp)
        .args(["plugin", "run", "warmable", "--warm"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "ccs plugin run --warm exited non-zero: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("--warm"),
        "--warm header missing: {}",
        stdout
    );
    assert!(
        !stdout.contains("would TIME OUT"),
        "--warm should not flag a cold-start timeout, got: {}",
        stdout
    );
}

#[cfg(unix)]
#[test]
fn plugin_run_shows_sanitized_preview_when_stdout_has_newlines() {
    // Multi-line stdout must collapse to a single line in the
    // "As shown in the status line" section but stay multi-line in
    // the "stdout (raw)" block. This is the contract that lets users
    // verify they understand what Claude Code will see.
    let tmp = TempDir::new().unwrap();
    let dir = ccs_plugin_path(&tmp);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("multiline");
    std::fs::write(&path, "#!/bin/sh\nprintf 'line1\\nline2\\nline3'\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&path, perm).unwrap();

    let out = ccs(&tmp)
        .args(["plugin", "run", "multiline"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Both raw and sanitized blocks present.
    assert!(stdout.contains("stdout (raw)"));
    assert!(stdout.contains("As shown in the status line"));
    // Sanitized must contain the joined form on a single line.
    assert!(
        stdout.contains("line1 line2 line3"),
        "sanitized preview missing collapsed form: {}",
        stdout
    );
}

/// Helper: resolve where the test's `ccs` binary will write plugin files.
/// We can't import `plugin::plugins_dir` directly (private), but `ccs
/// plugin path` prints exactly that location for our isolated env.
fn ccs_plugin_path(tmp: &TempDir) -> std::path::PathBuf {
    let out = ccs(tmp).args(["plugin", "path"]).output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    std::path::PathBuf::from(stdout.trim())
}
