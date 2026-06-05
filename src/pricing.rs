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
///
/// Prices are per million tokens (USD), matching the public Anthropic
/// pricing page at the time of writing. Verified against ccusage's
/// `pricing.rs:367-545` and cross-checked against cc-switch's
/// `services/session_usage.rs` outputs.
///
/// Important version-specific points:
///   - Opus 4.7 / 4.8 dropped from $15/$75 (the 4 / 4.5 / 4.6 rate)
///     to $5/$25, matching Sonnet's old tier.
///   - Haiku 4.5 went *up* to $1/$5; Haiku 3.5 stays at $0.80/$4.
///
/// We match on model id substrings — most specific first wins.
fn lookup_builtin(lower: &str) -> Option<ModelPrice> {
    // Order matters: try the most specific keys first so e.g.
    // "claude-opus-4-7" doesn't match the generic "opus" rule.

    // ---- Opus ----
    if lower.contains("opus-4-7") || lower.contains("opus-4-8") || lower.contains("opus-latest") {
        // Opus 4.7+ — cheaper tier
        return Some(ModelPrice {
            input: 5.0,
            output: 25.0,
        });
    }
    if lower.contains("opus") {
        // Opus 4 / 4.5 / 4.6 / 3
        return Some(ModelPrice {
            input: 15.0,
            output: 75.0,
        });
    }

    // ---- Sonnet ----
    if lower.contains("sonnet") {
        return Some(ModelPrice {
            input: 3.0,
            output: 15.0,
        });
    }

    // ---- Haiku ----
    if lower.contains("haiku-4") || lower.contains("haiku-latest") {
        return Some(ModelPrice {
            input: 1.0,
            output: 5.0,
        });
    }
    if lower.contains("haiku") {
        return Some(ModelPrice {
            input: 0.80,
            output: 4.0,
        });
    }

    None
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

    // 2. built-in family match (version-specific)
    lookup_builtin(&lower).map(|p| apply_tier(p, &lower))
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
/// Following ccusage and cc-switch: bills four buckets independently
/// at their per-million rates. `cache_read` and `cache_creation` use
/// the standard Anthropic multipliers off the input price (0.1× and
/// 1.25× respectively).
///
/// We trust the transcript's `input_tokens` as fresh input
/// (Anthropic semantics). Older Claude Code transcripts (pre-prompt-
/// caching) often have cache_read = 0 and a large input — those are
/// genuinely fresh-input billed turns, not a schema bug.
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
    fn matches_opus_4_6_at_legacy_rate() {
        let p = lookup("claude-opus-4-6", &empty()).unwrap();
        assert_eq!(p.input, 15.0);
        assert_eq!(p.output, 75.0);
    }

    #[test]
    fn matches_opus_4_7_at_new_cheaper_rate() {
        let p = lookup("claude-opus-4-7", &empty()).unwrap();
        assert_eq!(p.input, 5.0);
        assert_eq!(p.output, 25.0);
    }

    #[test]
    fn matches_opus_latest_alias_at_4_7_rate() {
        let p = lookup("anthropic--claude-opus-latest", &empty()).unwrap();
        assert_eq!(p.input, 5.0);
        assert_eq!(p.output, 25.0);
    }

    #[test]
    fn matches_sonnet_via_id() {
        let p = lookup("anthropic--claude-sonnet-latest", &empty()).unwrap();
        assert_eq!(p.input, 3.0);
    }

    #[test]
    fn one_m_tier_doubles() {
        // Opus 4.7 with [1m] → $5×2 / $25×2
        let p = lookup("anthropic--claude-opus-latest[1m]", &empty()).unwrap();
        assert_eq!(p.input, 10.0);
        assert_eq!(p.output, 50.0);
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
    fn cost_realistic_opus_47_turn() {
        // Real numbers from cc-switch on a single day:
        // input=19k, output=599k, cache_read=245M, cache_creation=5.65M
        // → $172.16 against Opus 4.7's $5/$25 rate.
        let p = lookup("claude-opus-4-7", &empty()).unwrap();
        let c = cost(p, 19_000, 599_000, 245_000_000, 5_646_000);
        // 19k×5/M + 599k×25/M + 245M×5/M×0.1 + 5.65M×5/M×1.25 = 172.86
        assert!((c - 172.86).abs() < 1.0, "got {}", c);
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
        // 100k cache hit + 1k input + 1k output on Opus 4.6
        // = 100k × $15/M × 0.1 + 1k × $15/M + 1k × $75/M
        // = 0.150 + 0.015 + 0.075 = 0.240
        let p = lookup("claude-opus-4-6", &empty()).unwrap();
        let c = cost(p, 1_000, 1_000, 100_000, 0);
        assert!((c - 0.240).abs() < 1e-6, "got {}", c);
    }

    #[test]
    fn cost_legacy_no_cache_bills_at_input_rate() {
        // Pre-cache CC transcripts: cache_read = 0, input large.
        // That's genuinely fresh input — bill at full input rate.
        // Opus 4.6: 1M × $15/M = $15.00
        let p = lookup("claude-opus-4-6", &empty()).unwrap();
        let c = cost(p, 1_000_000, 0, 0, 0);
        assert!((c - 15.0).abs() < 1e-6, "got {}", c);
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
