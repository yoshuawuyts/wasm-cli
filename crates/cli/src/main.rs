//! Wasm CLI command
//!

mod config;
mod init;
mod inspect;
mod install;
mod local;
mod package;
mod self_;
mod tui;
mod util;

use std::io::IsTerminal;

use clap::{ColorChoice, CommandFactory, Parser};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// When to use colored output.
    ///
    /// Can also be controlled via environment variables:
    /// - NO_COLOR=1 (disables color)
    /// - CLICOLOR=0 (disables color)
    /// - CLICOLOR_FORCE=1 (forces color)
    #[arg(
        long,
        value_name = "WHEN",
        default_value = "auto",
        global = true,
        help_heading = "Global Options"
    )]
    color: ColorChoice,

    /// Run in offline mode.
    ///
    /// Disables all network operations. Commands that require network access
    /// will fail with an error. Local-only commands will continue to work.
    #[arg(long, global = true, help_heading = "Global Options")]
    offline: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

impl Cli {
    async fn run(self) -> Result<(), anyhow::Error> {
        match self.command {
            Some(Command::Run) => todo!(),
            Some(Command::Inspect(opts)) => opts.run()?,
            Some(Command::Convert) => todo!(),
            Some(Command::Local(opts)) => opts.run()?,
            Some(Command::Package(opts)) => opts.run(self.offline).await?,
            Some(Command::Compose) => todo!(),
            Some(Command::Init(opts)) => opts.run().await?,
            Some(Command::Install(opts)) => opts.run(self.offline).await?,
            Some(Command::Self_(opts)) => opts.run().await?,
            None if std::io::stdin().is_terminal() => tui::run(self.offline).await?,
            None => {
                // Apply the parsed color choice when printing help
                Cli::command().color(self.color).print_help()?;
            }
        }
        Ok(())
    }
}

#[derive(clap::Parser)]
enum Command {
    /// Execute a Wasm Component
    #[command(subcommand)]
    Run,
    /// Create a new wasm component in an existing directory
    Init(init::Opts),
    /// Install a dependency from an OCI registry
    Install(install::Opts),
    /// Inspect a Wasm Component
    Inspect(inspect::Opts),
    /// Convert a Wasm Component to another format
    #[command(subcommand)]
    Convert,
    /// Detect and manage local WASM files
    #[command(subcommand)]
    Local(local::Opts),
    /// Package, push, and pull Wasm Components
    #[command(subcommand)]
    Package(package::Opts),
    /// Compose Wasm Components with other components
    #[command(subcommand)]
    Compose,
    /// Configure the `wasm(1)` tool, generate completions, & manage state
    #[clap(name = "self")]
    #[command(subcommand)]
    Self_(self_::Opts),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Cli::parse().run().await?;
    Ok(())
}
