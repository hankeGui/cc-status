//! `ccs explain` — print a legend describing every statusline segment.

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_MAGENTA: &str = "\x1b[1;35m";
const RED: &str = "\x1b[0;31m";
const YELLOW: &str = "\x1b[0;33m";
const GREEN: &str = "\x1b[0;32m";
const DIM: &str = "\x1b[90m";

pub fn run() -> anyhow::Result<()> {
    let out = format!(
        "\
{B}cc-status · status-line legend{R}

{B}Where you are{R}
  {C}~/hanke-dev/cc-status{R}    {{dir}}    last 3 path segments, HOME shown as `~`
  {DIM}wt:foo{R}                  {{git}}    git worktree name (only inside a worktree)
  {M}main{R}                    {{git}}    git branch
  {DIM}⇡2{R} / {DIM}⇣1{R}                {{git}}    commits ahead of / behind upstream
  {RED}[+!?]{R}                   {{git}}    {RED}+{R} staged   {RED}!{R} modified   {RED}?{R} untracked
  {DIM}claude-opus-4-7 [1m]{R}     {{model}}  model id Claude Code is invoking. `[1m]`
                                  flags the 1M-context tier. Source order:
                                  transcript → stdin → settings.json.

{B}Capacity (`ctx`){R}
  {GREEN}ctx 54% ▰▰▰▰▰{DIM}▱▱▱▱▱{R} {DIM}504.1k/1.0M{R}
                            {{ctx}}     percent remaining + battery bar
                                       ({GREEN}▰{R}=remaining, {DIM}▱{R}=used) +
                                       tokens used / capacity
                            color: {GREEN}green{R} ≥50%  {YELLOW}yellow{R} 20–50%  {RED}red{R} <20%
                            capacity is the *physical* window backsolved
                            from CC's `remaining_percentage`, multiplied by
                            CLAUDE_AUTOCOMPACT_PCT_OVERRIDE/100 (default 95).
                            That's where auto-compact actually fires.

  {DIM}ctx-used 504.1k/1.0M{R}    {{ctx_tokens}}  numbers only, no bar/percent.

{B}This turn{R}
  {DIM}↑504.1k ↓618 cache+581 🎯100%{R}    {{last_turn}}
                            ↑   input *sent to model* (incl. cache hits +
                                cache writes). This is the actual on-wire size.
                            ↓   output the model produced.
                            cache+N  tokens *written* to the prompt cache
                                this turn (cache_creation, 1.25× price now,
                                will be 0.1× when re-hit next turn).
                            🎯  cache-hit rate of *this single turn*
                                (not the session). Higher = cheaper.

{B}Cache health{R}
  {DIM}cache 4:42{R}              {{cache_ttl}}    countdown to the prompt-cache 5-min
                                       TTL expiry. {RED}Red <1 min / expired{R} = next
                                       turn pays full input price, not cache_read.
  {DIM}hit 96%{R}                 {{hit_rate}}     session-wide cumulative cache hit
                                       rate (every turn aggregated). Different
                                       from {{last_turn}}'s 🎯 (last turn only).

{B}Session metrics{R}
  {DIM}🔥 32.4k tok/min{R}         {{burn}}        average token throughput across
                                       the whole session.
  {DIM}1h23m{R}                   {{session_age}} wall-clock since the first turn:
                                       42s / 12m / 1h23m / 2d3h.

{B}Tools used{R}
  {DIM}skills: jira×3 wiki×1{R}    {{skills}}     top-4 Skill calls × count.
  {DIM}mcp: github×2{R}            {{mcp}}        MCP-server calls grouped by server.

{B}Cost (USD){R}
  Built-in prices follow Anthropic's published per-million-token rates.
  Override any model in `[pricing]` of config.toml.

  {YELLOW}last $0.012{R}             {{cost_last}}     last turn (current model)
  {YELLOW}sess $1.42{R}              {{cost_session}}  cumulative for this session
  {YELLOW}today $4.18{R}             {{cost_today}}    every session today (local-time
                                          day buckets), all models
  {YELLOW}7d $24.50{R}               {{cost_week}}     rolling last 7 days, all models
  {YELLOW}last $0.012{R} {DIM}|{R} {YELLOW}today $4.18{R}   {{cost}}          combo of cost_last + cost_today

{B}Plugin segments{R}
  {DIM}{{plugin:NAME}}{R}           runs `<config>/plugins/NAME` as an executable;
                            its stdout becomes the segment value. CC's stdin
                            JSON is piped in. Hard 250 ms timeout, output
                            clipped to 80 chars, control chars stripped
                            (ANSI SGR allowed). Empty / failed / missing →
                            renders as nothing.
                            See `ccs plugin new` to scaffold one.

{B}Color cheat-sheet{R}
  {C}bold cyan{R}     path
  {M}bold magenta{R}  git branch
  {RED}red{R}           danger (dirty / low ctx / cache about to expire)
  {YELLOW}yellow{R}        money / warning (ctx 20–50%)
  {GREEN}green{R}         healthy (ctx ≥50%)
  {DIM}dim/grey{R}      secondary data

{B}Pricing cheat-sheet{R}
  cache_read       0.1× normal input price  (higher hit rate = cheaper)
  cache_creation   1.25× normal input price (pricey now, cheap next turn if hit)
  output           ~5× normal input price

{B}Related commands{R}
  ccs status            full dashboard for this session, every number labeled
  ccs segments          listing of every segment with examples
  ccs mode list         all configured display modes with rendered previews
  ccs mode <name>       switch active mode
  ccs config edit       drag-and-drop visual mode editor in your browser
  ccs plugin doctor     health-check installed plugins
  ccs config-path       print the config file path
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
