//! `ccs explain` — print a legend describing every statusline segment.

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_MAGENTA: &str = "\x1b[1;35m";
const RED: &str = "\x1b[0;31m";
const YELLOW: &str = "\x1b[0;33m";
const GREEN: &str = "\x1b[0;32m";
const DIM: &str = "\x1b[2m";

pub fn run() -> anyhow::Result<()> {
    let out = format!(
        "\
{B}cc-status · status-line legend{R}

{B}Line 1 · environment{R}
  {C}~/path{R}                Current directory (last 3 components, ~ for HOME)
  {DIM}wt:NAME{R}               Git worktree name (only when inside one)
  {M}branch{R}                Git branch
  {DIM}⇡N{R} / {DIM}⇣N{R}              Commits ahead of / behind upstream
  {RED}[+!?]{R}                 + staged   ! modified   ? untracked
  {DIM}model{R}                 Model name reported by Claude Code

{B}Line 2 · this turn + cache{R}
  ctx {GREEN}84%{R} {GREEN}█████{R} {DIM}133k/950k{R}  Remaining % + bar + used/capacity
                        (green ≥50, yellow 20–50, red <20)
                        capacity = backsolved physical window × CLAUDE_AUTOCOMPACT_PCT_OVERRIDE/100
                        Default 95. A 1M Opus thus shows ~950k usable.
  {DIM}↑12.3k{R}                Last turn's input tokens (incl. cache hit)
  {DIM}↓341{R}                  Last turn's output tokens
  {DIM}+865{R}                  Tokens written to cache this turn (cache_creation, 1.25× price)
  {DIM}🎯99%{R}                 Cache hit rate of the last turn
  {DIM}cache 4:42{R}            Prompt-cache 5-min TTL countdown ({RED}red <1 min{R} / cache expired)
  {DIM}hit 96%{R}               Session-wide cumulative cache hit rate
  {DIM}🔥 32.4k/min{R}          Session-average token rate

{B}Line 3 · tools used in this session{R}
  {DIM}skills: jira×3 wiki×1{R}  Skill tool calls (top 4, sorted by count)
  {DIM}mcp: github×2{R}         MCP-server calls (aggregated by server name)

{B}Color meanings{R}
  {C}bold cyan{R}    path
  {M}bold magenta{R} git branch
  {RED}red{R}          danger (dirty / low ctx / cache about to expire)
  {YELLOW}yellow{R}       warning (ctx 20–50%)
  {GREEN}green{R}        healthy (ctx ≥50%)
  {DIM}dim{R}          secondary data

{B}Pricing cheat-sheet{R}
  cache_read       0.1× normal input price (higher hit rate = cheaper)
  cache_creation   1.25× normal input price (pricey now, but cheap on the next turn)
  output           ~5× normal input price

{B}Related commands{R}
  ccs status            Detailed dashboard for the current session
  ccs mode <name>       Switch display mode: compact / detailed / debug
  ccs config-path       Print the config file path
",
        B = BOLD,
        R = RESET,
        C = BOLD_CYAN,
        M = BOLD_MAGENTA,
        DIM = DIM,
        RED = RED,
        YELLOW = YELLOW,
        GREEN = GREEN,
    );
    print!("{}", out);
    Ok(())
}
