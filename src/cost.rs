//! `ccs cost` — full cross-session cost dashboard.

use crate::{config, pricing, rollup};
use anyhow::Result;
use chrono::{Datelike, Local};
use std::collections::HashMap;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[90m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[0;33m";
const RED: &str = "\x1b[0;31m";
const CYAN: &str = "\x1b[1;36m";

#[derive(Default)]
pub struct Args {
    /// Number of days to show. Default 7.
    pub days: usize,
    /// Print per-file contribution breakdown.
    pub debug: bool,
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
    let today = Local::now();
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
    if args.debug {
        print_debug_per_file(&day_totals, &cfg);
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

fn print_debug_per_file(day_totals: &[(String, f64, HashMap<String, f64>)], cfg: &config::Config) {
    let from_day = day_totals.first().map(|(d, _, _)| d.as_str()).unwrap_or("");
    let to_day = day_totals.last().map(|(d, _, _)| d.as_str()).unwrap_or("");
    if from_day.is_empty() {
        return;
    }

    println!();
    println!(
        "{B}Per-file breakdown ({}–{}){R}",
        from_day,
        to_day,
        B = BOLD,
        R = RESET
    );
    println!("{}{}{}", DIM, "─".repeat(50), RESET);

    let contributions = rollup::debug_per_file(from_day, to_day);
    if contributions.is_empty() {
        println!(
            "{}(no transcript files contributed in this window){}",
            DIM, RESET
        );
        return;
    }

    let mut grand = (0u64, 0u64, 0u64, 0u64);
    let mut grand_cost = 0.0;
    let mut grand_entries = 0u64;
    let mut grand_unique = 0u64;
    let mut grand_skipped = 0u64;

    for c in &contributions {
        let pricing_lookup = pricing::lookup(&c.model, &cfg.pricing);
        let usd = pricing_lookup.map_or(0.0, |p| {
            pricing::cost(
                p,
                c.bucket.input,
                c.bucket.output,
                c.bucket.cache_read,
                c.bucket.cache_creation,
            )
        });
        grand.0 += c.bucket.input;
        grand.1 += c.bucket.output;
        grand.2 += c.bucket.cache_read;
        grand.3 += c.bucket.cache_creation;
        grand_cost += usd;
        grand_entries += c.entries_total;
        grand_unique += c.entries_unique;
        grand_skipped += c.entries_skipped_date;

        let dup_pct = if c.entries_total > 0 {
            (c.entries_total - c.entries_unique) as f64 / c.entries_total as f64 * 100.0
        } else {
            0.0
        };

        let short_path = c
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .chars()
            .take(36)
            .collect::<String>();
        let project = c
            .path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("?");

        println!(
            "  {C}{:<12}{R}  {DIM}entries {} → {} unique ({}% dup), date-skip {}{R}",
            pricing::fmt_usd(usd),
            c.entries_total,
            c.entries_unique,
            dup_pct.round() as u32,
            c.entries_skipped_date,
            C = if usd >= 5.0 { YELLOW } else { GREEN },
            R = RESET,
            DIM = DIM,
        );
        println!(
            "    {DIM}↑{}  ↓{}  cr{}  cw{}  · {}{R}",
            short_num(c.bucket.input),
            short_num(c.bucket.output),
            short_num(c.bucket.cache_read),
            short_num(c.bucket.cache_creation),
            c.model,
            DIM = DIM,
            R = RESET,
        );
        println!(
            "    {DIM}{}/{}{R}",
            project,
            short_path,
            DIM = DIM,
            R = RESET
        );
    }

    println!();
    println!(
        "{B}Files: {} · entries: {} → {} unique ({} dup'd, {} out-of-window){R}",
        contributions.len(),
        grand_entries + grand_skipped,
        grand_unique,
        grand_entries - grand_unique,
        grand_skipped,
        B = BOLD,
        R = RESET,
    );
    println!(
        "{B}Sum: ↑{} ↓{} cr{} cw{}  =  {}{R}",
        short_num(grand.0),
        short_num(grand.1),
        short_num(grand.2),
        short_num(grand.3),
        pricing::fmt_usd(grand_cost),
        B = BOLD,
        R = RESET,
    );
}

fn short_num(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
