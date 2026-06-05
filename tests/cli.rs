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
        // macOS uses Library/Application Support — point HOME to the tmp dir.
        .env("HOME", tmp.path())
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
    assert!(written.contains("\"command\""), "should write a command field");
    assert!(written.contains("ccs"), "command should reference the ccs binary");
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
