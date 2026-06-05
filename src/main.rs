use anyhow::Result;
use clap::{Parser, Subcommand};

mod cache;
mod config;
mod explain;
mod list_segments;
mod pricing;
mod render;
mod rollup;
mod segments;
mod segments_meta;
mod setup;
mod status;
mod transcript;

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
    },
    /// Manage display modes (switch / add / remove / list).
    Mode {
        #[command(subcommand)]
        action: Option<ModeAction>,
        /// Mode name to switch to (when no subcommand is given).
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
    /// Write a default config file. Use --force to overwrite an existing one.
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Print resolved config path.
    ConfigPath,
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
        Cmd::Setup {
            yes,
            check,
            uninstall,
        } => setup::run(setup::Args {
            yes,
            check,
            uninstall,
        }),
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
        Cmd::Init { force } => config::init_default(force),
        Cmd::ConfigPath => {
            println!("{}", config::config_path()?.display());
            Ok(())
        }
    }
}
