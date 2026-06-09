use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

mod cache;
mod config;
mod cost;
mod daemon;
mod explain;
mod list_segments;
mod plugin;
mod pricing;
mod render;
mod rollup;
mod segments;
mod segments_meta;
mod setup;
mod status;
mod transcript;
mod upgrade;
mod web;

#[derive(Parser)]
#[command(name = "ccs", version, about = "Claude Code status line")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Read Claude Code status JSON from stdin and print the status line.
    Render,
    /// Print a self-explanatory dashboard for the current session
    /// (reads the same JSON-on-stdin that `render` does).
    Status,
    /// Print a legend for every status-line segment.
    Explain,
    /// List every segment available for use in mode templates.
    Segments,
    /// Print a multi-day cost dashboard (uses the cross-session rollup).
    Cost {
        /// Number of days to display (default 7, max 90).
        #[arg(long, default_value_t = 7)]
        days: usize,
        /// Print per-file token / cost / dedupe breakdown.
        #[arg(long)]
        debug: bool,
    },
    /// One-shot configuration: write the `statusLine` block into
    /// `~/.claude/settings.json` (with backup). Detects npx vs binary
    /// install and writes the appropriate command.
    Setup {
        /// Skip confirmation prompts.
        #[arg(long)]
        yes: bool,
        /// Report current state without making changes.
        #[arg(long)]
        check: bool,
        /// Remove the statusLine block instead of installing it.
        #[arg(long)]
        uninstall: bool,
        /// Force-install the conversational helper skill (no prompt).
        #[arg(long)]
        with_skill: bool,
        /// Skip the conversational helper skill (no prompt).
        #[arg(long, conflicts_with = "with_skill")]
        no_skill: bool,
    },
    /// Upgrade ccs in place using the original install method.
    Upgrade {
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
        /// Detect install method but don't run anything.
        #[arg(long)]
        check: bool,
    },
    /// Optional Unix-socket daemon for sub-millisecond status-line refresh.
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Generate shell completion script for bash / zsh / fish / etc.
    Completions {
        /// Target shell.
        shell: Shell,
    },
    /// Manage display modes (switch / add / remove / list).
    Mode {
        #[command(subcommand)]
        action: Option<ModeAction>,
        /// Mode name to switch to (when no subcommand is given).
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
    /// Manage `{plugin:NAME}` segments — scaffold, list, debug.
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
    /// Write a default config file. Use --force to overwrite an existing one.
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Print resolved config path.
    ConfigPath,
    /// Open a drag-and-drop mode editor in the browser. Spawns a
    /// short-lived 127.0.0.1 server that exits 5 s after Save (or
    /// after 30 minutes of idle).
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon (forks into the background by default).
    Start {
        /// Run in the foreground (don't fork). Useful for launchd / systemd.
        #[arg(long)]
        foreground: bool,
    },
    /// Stop a running daemon (SIGTERM).
    Stop,
    /// Restart the daemon.
    Restart,
    /// Show daemon status.
    Status,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Open a drag-and-drop mode editor in the browser. Starts a
    /// short-lived 127.0.0.1 server that exits 5 s after Save (or
    /// after 30 minutes of idle).
    Edit {
        /// Don't try to launch the browser; just print the URL.
        #[arg(long)]
        no_open: bool,
        /// Bind to a specific port instead of letting the OS pick.
        #[arg(long)]
        port: Option<u16>,
    },
}

#[derive(Subcommand)]
enum PluginAction {
    /// List installed plugins (executables under `<config>/plugins/`).
    List,
    /// Print the plugins directory path.
    Path,
    /// Scaffold a new plugin from a template (sh or python).
    New {
        /// Plugin name (letters, digits, '-', '_', '.'; cannot start with '.').
        name: String,
        /// Language template: sh (default) or python.
        #[arg(long, default_value = "sh")]
        lang: String,
        /// Overwrite if a file with this name already exists.
        #[arg(long)]
        force: bool,
    },
    /// Run a plugin in debug mode (shows stdout, stderr, exit, elapsed,
    /// and the sanitized value the status line would display).
    Run {
        /// Plugin name (must already exist under `<config>/plugins/`).
        name: String,
        /// Run twice and report the second run's timing. macOS Gatekeeper
        /// adds 200ms+ to the first execution of a new file; --warm shows
        /// the steady-state cost the status line will actually pay.
        #[arg(long)]
        warm: bool,
    },
    /// Health-check every installed plugin: executable bit, shebang,
    /// warm runtime vs the 250ms budget, exit code, and whether any
    /// mode references it.
    Doctor,
}

#[derive(Subcommand)]
enum ModeAction {
    /// List all configured modes (current marked with *).
    List,
    /// Add or update a custom mode.
    ///
    /// Each --line/-l flag adds one line. Use `{segment}` placeholders
    /// (run `ccs segments` for the list of available segments).
    Add {
        /// Name of the new mode.
        name: String,
        /// One line of the mode (repeatable).
        #[arg(short = 'l', long = "line", required = true)]
        lines: Vec<String>,
        /// Overwrite if a mode with this name already exists.
        #[arg(long)]
        force: bool,
    },
    /// Append segments to a mode without rewriting it.
    ///
    /// By default appends a new line to the *current* mode. Pass
    /// `--mode <name>` to target another mode and `--line <N>` (1-based)
    /// to append at the end of an existing line instead of a new one.
    Append {
        /// Segments to append. Either bare names (`ctx`) or already
        /// wrapped (`{ctx}`); literal text such as `cache:` is also OK.
        #[arg(required = true)]
        segments: Vec<String>,
        /// Target mode (default: current).
        #[arg(long)]
        mode: Option<String>,
        /// Append to an existing line (1-based). Default: new line.
        #[arg(long)]
        line: Option<usize>,
    },
    /// Open a mode in $EDITOR for free-form editing.
    Edit {
        /// Mode to edit (default: current).
        name: Option<String>,
    },
    /// Remove a mode.
    Rm {
        /// Name of the mode to remove.
        name: String,
    },
    /// Switch to a named mode (alias for `ccs mode <name>`).
    Set {
        /// Mode to switch to.
        name: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Cmd::Render) {
        Cmd::Render => render::run(),
        Cmd::Status => status::run(),
        Cmd::Explain => explain::run(),
        Cmd::Segments => list_segments::run(),
        Cmd::Cost { days, debug } => cost::run(cost::Args { days, debug }),
        Cmd::Setup {
            yes,
            check,
            uninstall,
            with_skill,
            no_skill,
        } => {
            let skill = if with_skill {
                Some(true)
            } else if no_skill {
                Some(false)
            } else {
                None
            };
            setup::run(setup::Args {
                yes,
                check,
                uninstall,
                skill,
            })
        }
        Cmd::Upgrade { yes, check } => upgrade::run(upgrade::Args { yes, check }),
        Cmd::Daemon { action } => match action {
            DaemonAction::Start { foreground } => daemon::start(daemon::StartArgs { foreground }),
            DaemonAction::Stop => daemon::stop(),
            DaemonAction::Restart => {
                let _ = daemon::stop();
                std::thread::sleep(std::time::Duration::from_millis(200));
                daemon::start(daemon::StartArgs::default())
            }
            DaemonAction::Status => daemon::status(),
        },
        Cmd::Completions { shell } => {
            let mut cmd = Cli::command();
            let bin_name = cmd.get_name().to_string();
            generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
            Ok(())
        }
        Cmd::Mode { action, name } => match (action, name) {
            (Some(ModeAction::List), _) => config::list_modes(),
            (Some(ModeAction::Add { name, lines, force }), _) => {
                config::add_mode(&name, &lines, force)
            }
            (
                Some(ModeAction::Append {
                    segments,
                    mode,
                    line,
                }),
                _,
            ) => config::append_segments(mode.as_deref(), &segments, line),
            (Some(ModeAction::Edit { name }), _) => config::edit_mode(name.as_deref()),
            (Some(ModeAction::Rm { name }), _) => config::remove_mode(&name),
            (Some(ModeAction::Set { name }), _) => config::set_mode(&name),
            (None, Some(name)) => config::set_mode(&name),
            (None, None) => config::list_modes(),
        },
        Cmd::Plugin { action } => match action {
            PluginAction::List => plugin::run(plugin::Action::List),
            PluginAction::Path => plugin::run(plugin::Action::Path),
            PluginAction::New { name, lang, force } => {
                let lang = plugin::Lang::from_flag(&lang)?;
                plugin::run(plugin::Action::New { name, lang, force })
            }
            PluginAction::Run { name, warm } => plugin::run(plugin::Action::Run { name, warm }),
            PluginAction::Doctor => plugin::run(plugin::Action::Doctor),
        },
        Cmd::Init { force } => config::init_default(force),
        Cmd::ConfigPath => {
            println!("{}", config::config_path()?.display());
            Ok(())
        }
        Cmd::Config { action } => match action {
            ConfigAction::Edit { no_open, port } => web::run(web::Args { no_open, port }),
        },
    }
}
