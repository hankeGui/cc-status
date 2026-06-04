use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

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
        Self { ctx_low: default_ctx_low(), ctx_med: default_ctx_med() }
    }
}

fn default_ctx_low() -> u8 { 20 }
fn default_ctx_med() -> u8 { 50 }

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
        Self { show_cache_creation: true }
    }
}

fn default_true() -> bool { true }
fn default_mode() -> String { "compact".into() }

impl Default for Config {
    fn default() -> Self {
        let mut modes = BTreeMap::new();
        modes.insert("compact".into(), Mode {
            lines: vec!["{dir} {git} {model} {ctx}".into()],
        });
        modes.insert("detailed".into(), Mode {
            lines: vec![
                "{dir} {git} {model}".into(),
                "{ctx} {last_turn} {cache_ttl} {hit_rate} {burn}".into(),
                "{skills} {mcp}".into(),
            ],
        });
        modes.insert("debug".into(), Mode {
            lines: vec![
                "{dir} {git}".into(),
                "{model} {ctx}".into(),
                "last: {last_turn}".into(),
                "cache: {cache_ttl}  {hit_rate}".into(),
                "burn: {burn}".into(),
                "skills: {skills}".into(),
                "mcp: {mcp}".into(),
            ],
        });
        Self {
            current_mode: "compact".into(),
            modes,
            theme: Theme::default(),
            segments: SegmentSettings::default(),
        }
    }
}

pub fn config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("dev", "hanke", "cc-status")
        .context("cannot determine config dir")?;
    Ok(dirs.config_dir().join("config.toml"))
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    let cfg: Config = toml::from_str(&text)
        .with_context(|| format!("parse {}", path.display()))?;
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
        eprintln!("config already exists at {} (use --force to overwrite)", path.display());
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
