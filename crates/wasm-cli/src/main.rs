//! Wasm CLI command
//!

mod init;
mod inspect;
mod install;
mod local;
mod registry;
mod self_;
mod tui;
mod util;

use std::io::IsTerminal;

use clap::{ColorChoice, CommandFactory, Parser};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub(crate) struct Cli {
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
            Some(Command::Registry(opts)) => opts.run(self.offline).await?,
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
    /// Manage images, components, and interfaces in OCI registries
    #[command(subcommand)]
    Registry(registry::Opts),
    /// Compose Wasm Components with other components
    #[command(subcommand)]
    Compose,
    /// Configure the `wasm(1)` tool, generate completions, & manage state
    #[clap(name = "self")]
    #[command(subcommand)]
    Self_(self_::Opts),
}

/// Compute the log directory for the application.
///
/// Uses the XDG state directory (`$XDG_STATE_HOME/wasm/logs`) on Linux,
/// and falls back to the local data directory on other systems.
pub(crate) fn log_dir() -> std::path::PathBuf {
    wasm_package_manager::StateInfo::default_log_dir()
}

/// Initialize the tracing subscriber with a file appender and a stderr layer
/// for warnings and above. Logs are stored in an XDG-compliant directory.
///
/// The returned `WorkerGuard` must be kept alive for the duration of the
/// program to ensure all buffered log records are flushed.
fn init_tracing() -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, Layer};

    let log_dir = log_dir();
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::never(&log_dir, "wasm.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")));

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(tracing_subscriber::filter::LevelFilter::WARN);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .init();

    Ok(guard)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _tracing_guard = init_tracing()?;
    Cli::parse().run().await?;
    Ok(())
}
