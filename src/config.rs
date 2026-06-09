use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::pricing::ModelPrice;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_mode")]
    pub current_mode: String,
    #[serde(default)]
    pub modes: BTreeMap<String, Mode>,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub segments: SegmentSettings,
    /// Per-model price overrides. Key = model id substring (e.g.
    /// `"opus"` or `"anthropic--claude-opus-latest"`); value =
    /// $/1M-tokens. Substrings match case-insensitively.
    #[serde(default)]
    pub pricing: HashMap<String, ModelPrice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Mode {
    pub lines: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Theme {
    #[serde(default = "default_ctx_low")]
    pub ctx_low: u8,
    #[serde(default = "default_ctx_med")]
    pub ctx_med: u8,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            ctx_low: default_ctx_low(),
            ctx_med: default_ctx_med(),
        }
    }
}

fn default_ctx_low() -> u8 {
    20
}
fn default_ctx_med() -> u8 {
    50
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SegmentSettings {
    #[serde(default)]
    pub last_turn: LastTurnCfg,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LastTurnCfg {
    #[serde(default = "default_true")]
    pub show_cache_creation: bool,
}

impl Default for LastTurnCfg {
    fn default() -> Self {
        Self {
            show_cache_creation: true,
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_mode() -> String {
    "compact".into()
}

impl Default for Config {
    fn default() -> Self {
        let mut modes = BTreeMap::new();
        // `balanced` is the default mode for new installs: three lines
        // covering the things most people want to glance at every turn.
        // The skills/mcp line vanishes when neither has been called
        // this session — empty segments collapse and there's nothing
        // left to render. Line 2 is dense (~80 chars on a healthy
        // turn) but stays readable on any modern terminal.
        modes.insert(
            "balanced".into(),
            Mode {
                lines: vec![
                    "{dir} {git} {model} {ctx}".into(),
                    "{session_age} {last_turn} {cost_last} {cost_session} {hit_rate} {burn}".into(),
                    "{skills} {mcp}".into(),
                ],
            },
        );
        modes.insert(
            "compact".into(),
            Mode {
                lines: vec!["{dir} {git} {model} {ctx}".into()],
            },
        );
        modes.insert(
            "minimal".into(),
            Mode {
                lines: vec!["{dir} {ctx}".into()],
            },
        );
        modes.insert(
            "detailed".into(),
            Mode {
                lines: vec![
                    "{dir} {git} {model}".into(),
                    "{ctx} {last_turn} {hit_rate} {burn}".into(),
                    "{skills} {mcp}".into(),
                ],
            },
        );
        modes.insert(
            "cost".into(),
            Mode {
                lines: vec![
                    "{dir} {git} {model} {ctx}".into(),
                    "{cost_last} {cost_session} {cost_today} {cost_week}".into(),
                ],
            },
        );
        modes.insert(
            "tokens".into(),
            Mode {
                lines: vec![
                    "{dir} {git} {ctx}".into(),
                    "{last_turn} {hit_rate} {burn}".into(),
                ],
            },
        );
        modes.insert(
            "tools".into(),
            Mode {
                lines: vec![
                    "{dir} {git} {model} {ctx}".into(),
                    "{skills}".into(),
                    "{mcp}".into(),
                ],
            },
        );
        modes.insert(
            "debug".into(),
            Mode {
                lines: vec![
                    "{dir} {git}".into(),
                    "{model} {ctx}".into(),
                    "last: {last_turn}".into(),
                    "cache: {cache_ttl}  {hit_rate}".into(),
                    "burn: {burn}".into(),
                    "cost: {cost_last} | {cost_session} | {cost_today} | {cost_week}".into(),
                    "skills: {skills}".into(),
                    "mcp: {mcp}".into(),
                ],
            },
        );
        Self {
            current_mode: "balanced".into(),
            modes,
            theme: Theme::default(),
            segments: SegmentSettings::default(),
            pricing: HashMap::new(),
        }
    }
}

pub fn config_path() -> Result<PathBuf> {
    let dirs =
        ProjectDirs::from("dev", "hanke", "cc-status").context("cannot determine config dir")?;
    Ok(dirs.config_dir().join("config.toml"))
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let cfg: Config = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(cfg)
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(cfg)?;
    std::fs::write(&path, text)?;
    Ok(())
}

pub fn init_default(force: bool) -> Result<()> {
    let path = config_path()?;
    if path.exists() && !force {
        eprintln!(
            "config already exists at {} (use --force to overwrite)",
            path.display()
        );
        return Ok(());
    }
    save(&Config::default())?;
    println!("wrote default config to {}", path.display());
    Ok(())
}

pub fn set_mode(name: &str) -> Result<()> {
    let mut cfg = load()?;
    if !cfg.modes.contains_key(name) {
        anyhow::bail!(
            "unknown mode '{}', available: {}",
            name,
            cfg.modes.keys().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    cfg.current_mode = name.into();
    save(&cfg)?;
    println!("mode -> {}", name);
    Ok(())
}

pub fn add_mode(name: &str, lines: &[String], force: bool) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("mode name cannot be empty");
    }
    if lines.is_empty() {
        anyhow::bail!("at least one --line is required");
    }
    // Validate referenced segment names; warn (not fail) on unknowns so users
    // can still add literal text or future segments.
    let mut unknowns: Vec<String> = Vec::new();
    for line in lines {
        for tok in extract_tokens(line) {
            if !crate::segments_meta::is_known(&tok) {
                unknowns.push(tok);
            }
        }
    }
    if !unknowns.is_empty() {
        eprintln!(
            "warning: unknown segment(s) referenced: {} (run `ccs segments` for the list)",
            unknowns.join(", ")
        );
    }

    let mut cfg = load()?;
    if cfg.modes.contains_key(name) && !force {
        anyhow::bail!("mode '{}' already exists; use --force to overwrite", name);
    }
    cfg.modes.insert(
        name.into(),
        Mode {
            lines: lines.to_vec(),
        },
    );
    save(&cfg)?;
    println!("mode '{}' saved with {} line(s)", name, lines.len());
    println!("switch to it with: ccs mode {}", name);
    Ok(())
}

pub fn remove_mode(name: &str) -> Result<()> {
    let mut cfg = load()?;
    if !cfg.modes.contains_key(name) {
        anyhow::bail!("mode '{}' does not exist", name);
    }
    if cfg.modes.len() == 1 {
        anyhow::bail!("cannot remove the only remaining mode");
    }
    if cfg.current_mode == name {
        // Pick any remaining mode to switch to.
        let next = cfg
            .modes
            .keys()
            .find(|k| k.as_str() != name)
            .cloned()
            .unwrap();
        eprintln!("active mode was '{}', switching to '{}'", name, next);
        cfg.current_mode = next;
    }
    cfg.modes.remove(name);
    save(&cfg)?;
    println!("removed mode '{}'", name);
    Ok(())
}

// ANSI used by list_modes' pretty preview. Kept local because the
// rest of this file is configuration plumbing.
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[1;36m";
const GREEN: &str = "\x1b[0;32m";

pub fn list_modes() -> Result<()> {
    let cfg = load()?;
    let preview_ctx = build_preview_ctx(&cfg);

    println!(
        "{B}cc-status modes{R}  {DIM}({})\n{DIM}{}{R}",
        config_path()?.display(),
        "─".repeat(60),
        B = BOLD,
        R = RESET,
        DIM = DIM,
    );

    for (name, mode) in &cfg.modes {
        let is_current = name == &cfg.current_mode;
        let header = if is_current {
            format!(
                "{G}▸ {C}{}{R}  {DIM}(active){R}",
                name,
                G = GREEN,
                C = CYAN,
                R = RESET,
                DIM = DIM,
            )
        } else {
            format!("  {C}{}{R}", name, C = CYAN, R = RESET)
        };
        println!();
        println!("{}", header);

        for line in &mode.lines {
            println!(
                "    {DIM}template{R}  {DIM}{}{R}",
                line,
                DIM = DIM,
                R = RESET
            );
            // Render the line with mock data so the user sees what each
            // template actually produces. render_line collapses
            // whitespace around empty segments — if a segment legitimately
            // has no preview value it just disappears, which mirrors how
            // the live status line behaves.
            let rendered = crate::render::render_line(line, &preview_ctx);
            if !rendered.trim().is_empty() {
                println!("    {DIM}example {R}  {}", rendered, DIM = DIM, R = RESET);
            }
        }
    }

    println!();
    println!("{}{}{}", DIM, "─".repeat(60), RESET);
    println!(
        "{B}Switch:{R}  ccs mode <name>             {B}Append:{R}  ccs mode append <segment>",
        B = BOLD,
        R = RESET
    );
    println!(
        "{B}Edit:{R}    ccs mode edit               {B}Add:{R}     ccs mode add <name> -l \"...\"",
        B = BOLD,
        R = RESET
    );
    println!(
        "{B}Explain:{R} ccs explain                 {B}Plugins:{R} ccs plugin new <name>",
        B = BOLD,
        R = RESET
    );
    println!();
    println!(
        "{}Examples above are rendered from synthetic data so each segment{}",
        DIM, RESET
    );
    println!(
        "{}has something to show. Empty segments collapse the surrounding space.{}",
        DIM, RESET
    );
    Ok(())
}

/// Build a `Ctx` populated with representative mock data so every
/// segment renders to something realistic-looking. The numbers are
/// chosen to look like a healthy mid-session: 3/4 of the window
/// remaining, mixed cache hits, a few skill / mcp calls. Keeping this
/// hand-tuned (instead of e.g. all 1's) means the preview matches the
/// shape users will actually see.
///
/// Returned values stay alive for the lifetime of the borrowed
/// references in the returned Ctx — caller holds the owning storage
/// in a tuple.
fn build_preview_ctx(cfg: &Config) -> crate::segments::Ctx<'_> {
    use crate::cache::SessionCache;
    use std::collections::HashMap;

    // We need owned storage for the things `Ctx` borrows, so leak it
    // into a `Box::leak` — `list_modes` runs once and exits, so the
    // small leak is fine and avoids threading a tuple of owned data
    // through call sites.
    //
    // Use the user's *real* cwd for `{dir}` and `{git}` so those
    // segments reflect the actual project they're sitting in. Token /
    // cost / skill segments stay synthetic because there's no truthful
    // mock for them at config-listing time.
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| "/Users/you/projects/cc-status".to_string());
    let stdin: &'static serde_json::Value = Box::leak(Box::new(serde_json::json!({
        "cwd": cwd,
        "model": {
            "id": "claude-opus-4-7",
            "display_name": "Claude Opus 4.7"
        },
        "context_window": { "remaining_percentage": 75.0 },
        "session_id": "preview",
        "transcript_path": ""
    })));

    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut skill_counts: HashMap<String, u32> = HashMap::new();
    skill_counts.insert("jira".into(), 3);
    skill_counts.insert("wiki".into(), 1);
    let mut mcp_counts: HashMap<String, u32> = HashMap::new();
    mcp_counts.insert("github".into(), 2);

    // Numbers picked so:
    //   * `last_turn` shows ↑12.3k ↓2.1k +865 🎯92% (cache_read / hit_base)
    //   * `ctx` backsolves to ~615k physical → 584k capacity (75% remaining)
    //   * `burn` over 5 minutes lands around 32k/min
    //   * `cache_ttl` lands around 4:42 remaining
    //   * `model` resolves from `last_model` (transcript precedence) so the
    //     example renders the same canonical id users will actually see
    let cache: &'static SessionCache = Box::leak(Box::new(SessionCache {
        last_turn_input: 12_300,
        last_turn_output: 2_100,
        last_turn_cache_read: 154_000,
        last_turn_cache_creation: 865,
        last_cache_read_ms: Some(now_ms - 18_000),
        skill_counts,
        mcp_counts,
        total_input: 41_000,
        total_output: 7_500,
        total_cache_read: 110_000,
        total_cache_creation: 3_400,
        first_turn_ms: Some(now_ms - 5 * 60_000),
        last_turn_ms: Some(now_ms - 30_000),
        last_model: Some("claude-opus-4-7".to_string()),
        ..SessionCache::default()
    }));

    // Mock rollup so cost_today / cost_week / cost can render. One day,
    // one model — built-in pricing for Opus 4.7 ($5 / $25) gives plausible
    // dollar figures.
    let rollup: &'static crate::rollup::Rollup = Box::leak(Box::new({
        let mut r = crate::rollup::Rollup::default();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut by_model: HashMap<String, crate::rollup::DayBucket> = HashMap::new();
        by_model.insert(
            "claude-opus-4-7".into(),
            crate::rollup::DayBucket {
                input: 1_200_000,
                output: 90_000,
                cache_read: 8_500_000,
                cache_creation: 250_000,
            },
        );
        r.by_day.insert(today, by_model);
        r
    }));

    let cfg_static: &'static Config = Box::leak(Box::new(cfg.clone()));

    crate::segments::Ctx {
        stdin,
        cache,
        cfg: cfg_static,
        rollup: Some(rollup),
    }
}

/// Extract `{name}` tokens from a line template.
fn extract_tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('}') {
            out.push(after[..end].to_string());
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    out
}

/// Wrap raw segment names in `{...}` and validate them against the
/// known-segments registry, warning on unknowns. Returns the formatted
/// pieces ready to be joined by spaces.
fn format_segment_args(args: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(args.len());
    let mut unknowns: Vec<String> = Vec::new();
    for raw in args {
        // Allow either `{name}` or bare `name`; allow literal text too
        // (anything that already contains a `{` is passed through).
        let formatted = if raw.contains('{') {
            raw.clone()
        } else if let Some(plugin_name) = raw.strip_prefix("plugin:") {
            // plugin:NAME → wrap into {plugin:NAME}; the segment dispatcher
            // routes it at render time. We don't validate against a static
            // list (the plugin file has to exist on disk).
            if plugin_name.is_empty() {
                eprintln!("warning: empty plugin name in 'plugin:'");
            }
            format!("{{{}}}", raw)
        } else if !raw.is_empty()
            && raw
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            // bare name → wrap, validate
            if !crate::segments_meta::is_known(raw) {
                unknowns.push(raw.clone());
            }
            format!("{{{}}}", raw)
        } else {
            // literal text (spaces, punctuation, …): pass through
            raw.clone()
        };
        out.push(formatted);
    }
    if !unknowns.is_empty() {
        eprintln!(
            "warning: unknown segment(s): {} (run `ccs segments` for the list)",
            unknowns.join(", ")
        );
    }
    out
}

/// Append segments to a mode. By default appends them as a new line at
/// the end of the mode. With `to_line: Some(idx)` it appends them to
/// the end of an existing line (1-based index).
pub fn append_segments(
    mode_name: Option<&str>,
    segments: &[String],
    to_line: Option<usize>,
) -> Result<()> {
    if segments.is_empty() {
        anyhow::bail!("at least one segment is required");
    }
    let mut cfg = load()?;
    let target = mode_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| cfg.current_mode.clone());

    let pieces = format_segment_args(segments).join(" ");
    let mode = cfg
        .modes
        .get_mut(&target)
        .ok_or_else(|| anyhow::anyhow!("mode '{}' does not exist", target))?;

    match to_line {
        Some(line_no) => {
            if line_no == 0 || line_no > mode.lines.len() {
                anyhow::bail!(
                    "line {} out of range: mode '{}' has {} line(s)",
                    line_no,
                    target,
                    mode.lines.len()
                );
            }
            let idx = line_no - 1;
            let existing = std::mem::take(&mut mode.lines[idx]);
            mode.lines[idx] = if existing.is_empty() {
                pieces
            } else {
                format!("{} {}", existing, pieces)
            };
            println!(
                "appended to mode '{}' line {}: {}",
                target, line_no, mode.lines[idx]
            );
        }
        None => {
            mode.lines.push(pieces.clone());
            println!(
                "appended new line to mode '{}' (now {} line(s)): {}",
                target,
                mode.lines.len(),
                pieces
            );
        }
    }
    save(&cfg)?;
    Ok(())
}

/// Edit a mode interactively in $EDITOR. The user gets a temp file
/// with one line per template; on save we replace the mode's lines.
/// Empty lines and lines starting with `#` are dropped.
pub fn edit_mode(mode_name: Option<&str>) -> Result<()> {
    let mut cfg = load()?;
    let target = mode_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| cfg.current_mode.clone());
    let mode = cfg
        .modes
        .get(&target)
        .ok_or_else(|| anyhow::anyhow!("mode '{}' does not exist", target))?;

    let header = format!(
        "# Editing cc-status mode '{}'.\n# One line per status-line row. Blank/`#` lines are ignored.\n# Run `ccs segments` for the list of {{name}} placeholders.\n# Save and quit to apply, or quit without saving to abort.\n",
        target
    );
    let body: String = mode.lines.iter().map(|l| format!("{}\n", l)).collect();

    let dir = std::env::temp_dir();
    let file_path = dir.join(format!("ccs-mode-{}.tmpl", sanitize_filename(&target)));
    std::fs::write(&file_path, format!("{}{}", header, body))?;

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".into());

    let status = std::process::Command::new(&editor)
        .arg(&file_path)
        .status()
        .with_context(|| format!("failed to launch editor: {}", editor))?;
    if !status.success() {
        anyhow::bail!(
            "editor exited with non-zero status; mode '{}' not changed",
            target
        );
    }

    let edited = std::fs::read_to_string(&file_path)?;
    let new_lines: Vec<String> = edited
        .lines()
        .filter_map(|l| {
            let trimmed = l.trim_end();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();

    let _ = std::fs::remove_file(&file_path);

    if new_lines.is_empty() {
        anyhow::bail!("mode would be empty after editing — aborting");
    }

    // Validate referenced segments and warn on unknowns.
    let mut unknowns: Vec<String> = Vec::new();
    for line in &new_lines {
        for tok in extract_tokens(line) {
            if !crate::segments_meta::is_known(&tok) {
                unknowns.push(tok);
            }
        }
    }
    if !unknowns.is_empty() {
        eprintln!(
            "warning: unknown segment(s): {} (run `ccs segments` for the list)",
            unknowns.join(", ")
        );
    }

    cfg.modes
        .get_mut(&target)
        .expect("mode existed before editing")
        .lines = new_lines.clone();
    save(&cfg)?;
    println!("mode '{}' updated ({} line(s))", target, new_lines.len());
    Ok(())
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tokens_basic() {
        assert_eq!(
            extract_tokens("{dir} {git} {ctx}"),
            vec!["dir", "git", "ctx"]
        );
    }

    #[test]
    fn extract_tokens_with_text() {
        assert_eq!(extract_tokens("hi {a} mid {b} end"), vec!["a", "b"]);
    }

    #[test]
    fn extract_tokens_unbalanced_brace_stops() {
        // unmatched { should stop further extraction
        assert_eq!(extract_tokens("{ok} {bad"), vec!["ok"]);
    }

    #[test]
    fn default_config_has_eight_modes() {
        let cfg = Config::default();
        // Default lineup: balanced (active) + 7 specialty presets.
        // Update this whenever the default lineup changes — it's the
        // canary for `presets_are_present_after_init` in tests/cli.rs.
        for name in &[
            "balanced", "compact", "minimal", "detailed", "cost", "tokens", "tools", "debug",
        ] {
            assert!(
                cfg.modes.contains_key(*name),
                "missing default mode: {}",
                name
            );
        }
        assert_eq!(cfg.current_mode, "balanced");
    }

    #[test]
    fn theme_default_is_nonzero() {
        let t = Theme::default();
        assert_eq!(t.ctx_low, 20, "Theme::default() must not silently zero out");
        assert_eq!(t.ctx_med, 50);
    }

    #[test]
    fn config_round_trip_through_toml() {
        let cfg = Config::default();
        let s = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(back.current_mode, cfg.current_mode);
        assert_eq!(back.modes.len(), cfg.modes.len());
        assert_eq!(back.theme.ctx_low, cfg.theme.ctx_low);
    }
}
