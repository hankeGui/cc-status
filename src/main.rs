use anyhow::Result;
use clap::{Parser, Subcommand};

mod cache;
mod config;
mod explain;
mod render;
mod segments;
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
    /// Switch the current display mode.
    Mode { name: String },
    /// Write a default config file. Use --force to overwrite an existing one.
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Print resolved config path.
    ConfigPath,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Cmd::Render) {
        Cmd::Render => render::run(),
        Cmd::Status => status::run(),
        Cmd::Explain => explain::run(),
        Cmd::Mode { name } => config::set_mode(&name),
        Cmd::Init { force } => config::init_default(force),
        Cmd::ConfigPath => {
            println!("{}", config::config_path()?.display());
            Ok(())
        }
    }
}
