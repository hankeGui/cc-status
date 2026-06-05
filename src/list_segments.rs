//! `ccs segments` — list every segment available for use in mode templates.

use crate::segments_meta::SEGMENTS;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[1;36m";

pub fn run() -> anyhow::Result<()> {
    println!("{B}cc-status · 可选段{R}", B = BOLD, R = RESET);
    println!();
    println!(
        "在 mode 的 lines 模板里用 {C}{{name}}{R} 引用任何段。例如：",
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
        "{B}{:<nw$}{:<ew$}说明{R}",
        "段",
        "示例",
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
    println!("{B}下一步{R}", B = BOLD, R = RESET);
    println!("  ccs mode list                          列出所有已有模式");
    println!("  ccs mode add <name> -l \"<line>\" ...    建一个新模式");
    println!("  ccs mode <name>                        切换到该模式");
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
