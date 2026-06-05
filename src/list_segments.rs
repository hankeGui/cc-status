//! `ccs segments` — list every segment available for use in mode templates.

use crate::segments_meta::SEGMENTS;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[1;36m";

pub fn run() -> anyhow::Result<()> {
    println!("{B}cc-status · available segments{R}", B = BOLD, R = RESET);
    println!();
    println!(
        "Reference any segment from a mode's lines as {C}{{name}}{R}. Example:",
        C = CYAN,
        R = RESET
    );
    println!(
        "  {DIM}lines = [\"{{dir}} {{git}} {{ctx}}\"]{R}",
        DIM = DIM,
        R = RESET
    );
    println!();

    // Compute column width for alignment.
    let name_w = SEGMENTS.iter().map(|s| s.name.len()).max().unwrap_or(10) + 4;
    let ex_w = SEGMENTS
        .iter()
        .map(|s| display_width(s.example))
        .max()
        .unwrap_or(20)
        + 2;

    println!(
        "{B}{:<nw$}{:<ew$}description{R}",
        "segment",
        "example",
        nw = name_w,
        ew = ex_w,
        B = BOLD,
        R = RESET,
    );
    println!(
        "{DIM}{}{R}",
        "─".repeat(name_w + ex_w + 30),
        DIM = DIM,
        R = RESET
    );
    for s in SEGMENTS {
        let name_field = format!("{{{}}}", s.name);
        let pad_name = name_w.saturating_sub(name_field.len());
        let pad_ex = ex_w.saturating_sub(display_width(s.example));
        println!(
            "{C}{}{R}{}{}{}{}",
            name_field,
            " ".repeat(pad_name),
            s.example,
            " ".repeat(pad_ex),
            s.description,
            C = CYAN,
            R = RESET,
        );
    }
    println!();
    println!("{B}Next steps{R}", B = BOLD, R = RESET);
    println!("  ccs mode list                          List all configured modes");
    println!("  ccs mode add <name> -l \"<line>\" ...    Create a new mode");
    println!("  ccs mode <name>                        Switch to that mode");
    Ok(())
}

/// Approximate visible width: count CJK / wide chars as 2.
fn display_width(s: &str) -> usize {
    let mut w = 0;
    for c in s.chars() {
        // Skip ANSI escape sequences (we don't have any in examples, but be safe).
        let cw = if c == '\u{1b}' {
            0
        } else if (c as u32) > 0x2E80
            || matches!(
                c,
                '🎯' | '🔥'
                    | '↑'
                    | '↓'
                    | '⇡'
                    | '⇣'
                    | '█'
                    | '▉'
                    | '▊'
                    | '▋'
                    | '▌'
                    | '▍'
                    | '▎'
                    | '▏'
            )
        {
            2
        } else {
            1
        };
        w += cw;
    }
    w
}
