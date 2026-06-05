//! `ccs cost` — full cross-session cost dashboard.

use crate::{config, pricing, rollup};
use anyhow::Result;
use chrono::{Datelike, Utc};
use std::collections::HashMap;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[0;33m";
const RED: &str = "\x1b[0;31m";
const CYAN: &str = "\x1b[1;36m";

#[derive(Default)]
pub struct Args {
    /// Number of days to show. Default 7.
    pub days: usize,
}

pub fn run(args: Args) -> Result<()> {
    let cfg = config::load()?;
    let mut r = rollup::load();
    rollup::refresh(&mut r);
    let _ = rollup::save(&r);

    let days = args.days.max(1).min(90);

    println!(
        "{B}cc-status cost dashboard · last {} day(s){R}",
        days,
        B = BOLD,
        R = RESET
    );
    println!("{}{}{}", DIM, "─".repeat(50), RESET);

    // --- per-day breakdown -------------------------------------------------
    let mut day_totals: Vec<(String, f64, HashMap<String, f64>)> = Vec::new();
    let today = Utc::now();
    for offset in (0..days as i64).rev() {
        let d = today - chrono::Duration::days(offset);
        let key = format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day());
        let mut day_total = 0.0;
        let mut by_model: HashMap<String, f64> = HashMap::new();
        if let Some(buckets) = r.by_day.get(&key) {
            for (model, b) in buckets {
                let Some(price) = pricing::lookup(model, &cfg.pricing) else {
                    continue;
                };
                let usd = pricing::cost(price, b.input, b.output, b.cache_read, b.cache_creation);
                *by_model.entry(model.clone()).or_default() += usd;
                day_total += usd;
            }
        }
        day_totals.push((key, day_total, by_model));
    }

    let max_day = day_totals
        .iter()
        .map(|(_, t, _)| *t)
        .fold(0.0_f64, f64::max);
    let bar_w = 24usize;

    println!("{B}By day{R}", B = BOLD, R = RESET);
    let mut grand_total = 0.0;
    let mut grand_by_model: HashMap<String, f64> = HashMap::new();
    for (date, total, by_model) in &day_totals {
        grand_total += total;
        for (m, v) in by_model {
            *grand_by_model.entry(m.clone()).or_default() += v;
        }
        let bar_chars = if max_day > 0.0 {
            ((total / max_day) * bar_w as f64).round() as usize
        } else {
            0
        };
        let color = if *total >= max_day * 0.8 {
            RED
        } else if *total >= max_day * 0.4 {
            YELLOW
        } else {
            GREEN
        };
        let bar: String = "█".repeat(bar_chars);
        let pad: String = " ".repeat(bar_w.saturating_sub(bar_chars));
        let weekday = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map(|d| d.format("%a").to_string())
            .unwrap_or_default();
        println!(
            "  {DIM}{}{R} {} {C}{}{R}{P}  {}",
            date,
            weekday,
            bar,
            pricing::fmt_usd(*total),
            DIM = DIM,
            R = RESET,
            C = color,
            P = pad,
        );
    }
    println!();

    // --- by model ---------------------------------------------------------
    if !grand_by_model.is_empty() {
        println!("{B}By model (last {} day(s)){R}", days, B = BOLD, R = RESET);
        let mut models: Vec<(String, f64)> = grand_by_model.into_iter().collect();
        models.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let model_max = models.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max);
        for (model, usd) in &models {
            let pct = if model_max > 0.0 {
                usd / model_max
            } else {
                0.0
            };
            let bar_chars = ((pct) * 18.0).round() as usize;
            let bar: String = "█".repeat(bar_chars);
            let pad: String = " ".repeat(18 - bar_chars);
            println!(
                "  {C}{:<26}{R} {}{}  {}",
                model,
                bar,
                pad,
                pricing::fmt_usd(*usd),
                C = CYAN,
                R = RESET
            );
        }
        println!();
    }

    // --- grand total ------------------------------------------------------
    println!(
        "{B}Total (last {} day(s)): {}{R}",
        days,
        pricing::fmt_usd(grand_total),
        B = BOLD,
        R = RESET
    );
    if grand_total > 0.0 {
        let avg_per_day = grand_total / days as f64;
        println!("{}~{} / day{}", DIM, pricing::fmt_usd(avg_per_day), RESET);
    }
    println!();
    println!(
        "{}Run `ccs cost --days N` to widen the window. Use `[pricing.<model>]`{}",
        DIM, RESET
    );
    println!(
        "{}in {} to override built-in rates.{}",
        DIM,
        config::config_path()?.display(),
        RESET
    );

    Ok(())
}
