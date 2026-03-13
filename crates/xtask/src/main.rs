//! xtask - Build automation and task orchestration for the wasm project
//!
//! This binary provides a unified interface for running common development tasks
//! like testing, linting, and formatting checks.

mod readme;
mod sql;
mod test;

use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Build automation and task orchestration")]
enum Xtask {
    /// Run tests, clippy, and formatting checks
    Test,
    /// Run the `wasm` binary (equivalent to `cargo run --package wasm`)
    Run {
        /// Arguments to pass to the wasm binary
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run the `wasm-meta-registry` server
    RunRegistry {
        /// Arguments to pass to the wasm-meta-registry binary
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run a clean demo: reset state, init, and install ba:sample-wasi-http-rust
    Demo,
    /// Database schema and migration management
    Sql {
        #[command(subcommand)]
        command: SqlCommand,
    },
    /// Manage the README commands section
    Readme {
        #[command(subcommand)]
        command: ReadmeCommand,
    },
}

/// Subcommands for `cargo xtask readme`.
#[derive(Subcommand)]
enum ReadmeCommand {
    /// Regenerate the README commands section from `wasm --help`
    Update,
    /// Check that the README commands section is in sync with `wasm --help`
    Check,
}

/// Subcommands for `cargo xtask sql`.
#[derive(Subcommand)]
enum SqlCommand {
    /// Generate a new migration by diffing schema.sql against existing migrations
    Migrate {
        /// Name for the new migration (e.g. "add_oci_tables")
        #[arg(long)]
        name: String,
    },
    /// Check that schema.sql is in sync with existing migrations (CI gate)
    Check,
    /// Install sqlite3def for the current platform
    Install,
}

fn main() -> Result<()> {
    let xtask = Xtask::parse();

    match xtask {
        Xtask::Test => test::run_tests()?,
        Xtask::Run { args } => {
            let mut cargo_args = vec!["run", "--package", "wasm"];
            if !args.is_empty() {
                cargo_args.push("--");
                cargo_args.extend(args.iter().map(String::as_str));
            }
            run_command("cargo", &cargo_args)?;
        }
        Xtask::RunRegistry { args } => {
            let root = workspace_root()?;
            let registry_dir = root.join("registry");
            let registry_dir_str = registry_dir
                .to_str()
                .expect("workspace root path is valid UTF-8");
            let mut cargo_args = vec!["run", "--package", "wasm-meta-registry", "--"];
            if args.is_empty() {
                cargo_args.push(registry_dir_str);
            } else {
                cargo_args.extend(args.iter().map(String::as_str));
            }
            run_command("cargo", &cargo_args)?;
        }
        Xtask::Demo => run_demo()?,
        Xtask::Sql { command } => match command {
            SqlCommand::Migrate { name } => sql::migrate(&name)?,
            SqlCommand::Check => sql::check()?,
            SqlCommand::Install => sql::install()?,
        },
        Xtask::Readme { command } => {
            let root = workspace_root()?;
            match command {
                ReadmeCommand::Update => readme::update(&root)?,
                ReadmeCommand::Check => readme::check(&root)?,
            }
        }
    }

    Ok(())
}

/// Run a clean demo install.
///
/// 1. Checks the local meta-registry is reachable at `localhost:8080`.
/// 2. Cleans the global cache (`wasm self clean`).
/// 3. Removes local `vendor/`, `wasm.toml`, and `wasm.lock.toml`.
/// 4. Runs `wasm init`.
/// 5. Installs `ba:sample-wasi-http-rust`.
fn run_demo() -> Result<()> {
    // 1. Check the meta-registry is up.
    let health = std::net::TcpStream::connect_timeout(
        &"127.0.0.1:8080".parse().expect("valid addr"),
        std::time::Duration::from_secs(2),
    );
    if health.is_err() {
        anyhow::bail!(
            "meta-registry is not reachable at localhost:8080\n\
             Start it first with: cargo xtask run-registry"
        );
    }

    // 2. Clean global cache.
    run_command(
        "cargo",
        &["run", "--package", "wasm", "--", "self", "clean"],
    )?;

    // 3. Remove local project files.
    let root = workspace_root()?;
    let _ = std::fs::remove_dir_all(root.join("vendor"));
    let _ = std::fs::remove_file(root.join("wasm.toml"));
    let _ = std::fs::remove_file(root.join("wasm.lock.toml"));

    // 4. Init a fresh project.
    run_command("cargo", &["run", "--package", "wasm", "--", "init"])?;

    // 5. Install the sample component.
    run_command(
        "cargo",
        &[
            "run",
            "--package",
            "wasm",
            "--",
            "install",
            "ba:sample-wasi-http-rust",
        ],
    )?;

    Ok(())
}

/// Run an external command and bail if it exits with a non-zero status.
pub(crate) fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd).args(args).status()?;

    if !status.success() {
        anyhow::bail!("{} failed with exit code: {:?}", cmd, status.code());
    }

    Ok(())
}

/// Return the workspace root directory (the directory containing the root `Cargo.toml`).
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn workspace_root() -> Result<PathBuf> {
    // xtask is invoked via `cargo xtask` which sets CARGO_MANIFEST_DIR for the
    // xtask crate. Walk up to the workspace root (two levels: crates/xtask).
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_or_else(
            |_| {
                // Fallback: assume CWD is the workspace root.
                std::env::current_dir()
                    .expect("failed to determine current directory; ensure xtask is run from the workspace root")
            },
            PathBuf::from,
        );

    // If we're inside crates/xtask, go up two levels.
    if manifest_dir.ends_with("crates/xtask") {
        Ok(manifest_dir
            .parent()
            .and_then(|p| p.parent())
            .expect(
                "invalid workspace structure: expected crates/xtask to have two parent directories",
            )
            .to_path_buf())
    } else {
        Ok(manifest_dir)
    }
}
