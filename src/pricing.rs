//! Per-model pricing tables and cost calculation.
//!
//! Anthropic charges per million tokens, with two extra rules that
//! apply uniformly across all current Claude models:
//!   - cache *read* tokens are billed at 0.1× the input price.
//!   - cache *write* tokens (`cache_creation`) are billed at 1.25× the
//!     input price.
//!
//! The 1M-context tier (model id with the `[1m]` suffix) doubles both
//! input and output prices once a request exceeds the 200k-token
//! standard context window. We apply that multiplier when the model
//! name contains `[1m]`, on the assumption that any user who flipped
//! that switch is intentionally working in the high tier.
//!
//! These numbers reflect the public Anthropic pricing page as the
//! author last consulted it. They can drift, so users are expected to
//! override the table in `~/.config/cc-status/config.toml` for any
//! model where the default is wrong (e.g. SAP-internal proxy pricing,
//! enterprise plans, future model launches).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-million-token prices for one model variant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelPrice {
    /// Input price in USD per 1M tokens.
    pub input: f64,
    /// Output price in USD per 1M tokens.
    pub output: f64,
}

const CACHE_READ_MULT: f64 = 0.1;
const CACHE_WRITE_MULT: f64 = 1.25;
const ONE_M_TIER_MULT: f64 = 2.0;

/// Built-in defaults. Users override via `[pricing]` in config.toml.
fn builtin_table() -> HashMap<&'static str, ModelPrice> {
    let mut t = HashMap::new();
    // Opus 4.x family — standard tier.
    t.insert(
        "opus",
        ModelPrice {
            input: 15.0,
            output: 75.0,
        },
    );
    // Sonnet 4.x family.
    t.insert(
        "sonnet",
        ModelPrice {
            input: 3.0,
            output: 15.0,
        },
    );
    // Haiku 4.x family.
    t.insert(
        "haiku",
        ModelPrice {
            input: 0.80,
            output: 4.0,
        },
    );
    t
}

/// Look up a price for `model_name` (Anthropic-style id like
/// `claude-opus-4-7`, `anthropic--claude-opus-latest[1m]`,
/// `Claude Opus 4.7 (1m)`, …). Returns `None` if no family keyword
/// matches.
pub fn lookup(model_name: &str, overrides: &HashMap<String, ModelPrice>) -> Option<ModelPrice> {
    let lower = model_name.to_ascii_lowercase();

    // 1. exact override match wins
    if let Some(p) = overrides.get(model_name) {
        return Some(apply_tier(*p, &lower));
    }
    for (k, v) in overrides {
        if lower.contains(&k.to_ascii_lowercase()) {
            return Some(apply_tier(*v, &lower));
        }
    }

    // 2. built-in family match
    let base = if lower.contains("opus") {
        builtin_table().get("opus").copied()
    } else if lower.contains("sonnet") {
        builtin_table().get("sonnet").copied()
    } else if lower.contains("haiku") {
        builtin_table().get("haiku").copied()
    } else {
        None
    };
    base.map(|p| apply_tier(p, &lower))
}

/// `[1m]` in the model name doubles both input and output prices.
fn apply_tier(p: ModelPrice, lower_model: &str) -> ModelPrice {
    if lower_model.contains("[1m]") || lower_model.contains("(1m)") {
        ModelPrice {
            input: p.input * ONE_M_TIER_MULT,
            output: p.output * ONE_M_TIER_MULT,
        }
    } else {
        p
    }
}

/// Compute USD cost for one set of token counts under a given price.
///
/// `cache_read` and `cache_creation` are multiplied by the input price
/// modifiers (0.1× and 1.25× respectively).
pub fn cost(
    price: ModelPrice,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_creation: u64,
) -> f64 {
    let per_token_in = price.input / 1_000_000.0;
    let per_token_out = price.output / 1_000_000.0;

    (input as f64) * per_token_in
        + (cache_read as f64) * per_token_in * CACHE_READ_MULT
        + (cache_creation as f64) * per_token_in * CACHE_WRITE_MULT
        + (output as f64) * per_token_out
}

/// Smart-formats a cost in USD for a status-line column. Returns "" on
/// negative / NaN.
pub fn fmt_usd(usd: f64) -> String {
    if !usd.is_finite() || usd < 0.0 {
        return String::new();
    }
    if usd >= 100.0 {
        format!("${:.0}", usd)
    } else if usd >= 1.0 {
        format!("${:.2}", usd)
    } else if usd >= 0.01 {
        format!("${:.3}", usd)
    } else if usd > 0.0 {
        format!("${:.4}", usd)
    } else {
        "$0".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> HashMap<String, ModelPrice> {
        HashMap::new()
    }

    #[test]
    fn matches_opus_family() {
        let p = lookup("Claude Opus 4.7", &empty()).unwrap();
        assert_eq!(p.input, 15.0);
        assert_eq!(p.output, 75.0);
    }

    #[test]
    fn matches_sonnet_via_id() {
        let p = lookup("anthropic--claude-sonnet-latest", &empty()).unwrap();
        assert_eq!(p.input, 3.0);
    }

    #[test]
    fn one_m_tier_doubles() {
        let p = lookup("anthropic--claude-opus-latest[1m]", &empty()).unwrap();
        assert_eq!(p.input, 30.0);
        assert_eq!(p.output, 150.0);
    }

    #[test]
    fn override_wins_over_builtin() {
        let mut o = HashMap::new();
        o.insert(
            "opus".into(),
            ModelPrice {
                input: 1.0,
                output: 2.0,
            },
        );
        let p = lookup("Claude Opus 4.7", &o).unwrap();
        assert_eq!(p.input, 1.0);
    }

    #[test]
    fn override_then_one_m_tier() {
        let mut o = HashMap::new();
        o.insert(
            "opus".into(),
            ModelPrice {
                input: 1.0,
                output: 2.0,
            },
        );
        let p = lookup("Claude Opus 4.7 (1m)", &o).unwrap();
        assert_eq!(
            p.input, 2.0,
            "1m multiplier still applies on top of override"
        );
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(lookup("Gemini Ultra", &empty()).is_none());
    }

    #[test]
    fn cost_zero_inputs_zero() {
        let p = ModelPrice {
            input: 15.0,
            output: 75.0,
        };
        assert_eq!(cost(p, 0, 0, 0, 0), 0.0);
    }

    #[test]
    fn cost_applies_cache_multipliers() {
        let p = ModelPrice {
            input: 15.0,
            output: 75.0,
        };
        // 1M cache_read at 15.0 × 0.1 = 1.5
        let c = cost(p, 0, 0, 1_000_000, 0);
        assert!((c - 1.5).abs() < 1e-9);
        // 1M cache_creation at 15.0 × 1.25 = 18.75
        let c = cost(p, 0, 0, 0, 1_000_000);
        assert!((c - 18.75).abs() < 1e-9);
    }

    #[test]
    fn cost_realistic_turn() {
        // 100k cache hit + 1k input + 1k output on Opus
        // = 100k × $15/M × 0.1 + 1k × $15/M + 1k × $75/M
        // = 0.150 + 0.015 + 0.075 = 0.240
        let p = ModelPrice {
            input: 15.0,
            output: 75.0,
        };
        let c = cost(p, 1_000, 1_000, 100_000, 0);
        assert!((c - 0.240).abs() < 1e-6, "got {}", c);
    }

    #[test]
    fn fmt_usd_buckets() {
        assert_eq!(fmt_usd(0.0), "$0");
        assert_eq!(fmt_usd(0.0042), "$0.0042");
        assert_eq!(fmt_usd(0.42), "$0.420");
        assert_eq!(fmt_usd(4.2), "$4.20");
        assert_eq!(fmt_usd(420.0), "$420");
    }

    #[test]
    fn fmt_usd_rejects_nan() {
        assert_eq!(fmt_usd(f64::NAN), "");
        assert_eq!(fmt_usd(-1.0), "");
    }
}
