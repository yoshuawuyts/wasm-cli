use std::io;

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::Shell;
use wasm_package_manager::{Config, Manager, format_size};

/// The path of the dotenv file relative to the current working directory.
const DOTENV_PATH: &str = ".env";

/// Configure the `wasm(1)` tool, generate completions, & manage state
#[derive(clap::Parser)]
pub(crate) enum Opts {
    /// Print diagnostics about the local state
    State,
    /// Show configuration file location and current settings
    Config,
    /// Generate shell completions for the given shell
    Completions {
        /// The shell to generate completions for
        shell: Shell,
    },
    /// Generate a man page for the CLI
    ManPages,
}

impl Opts {
    pub(crate) async fn run(&self) -> Result<()> {
        match self {
            Opts::State => {
                let store = Manager::open().await?;
                let state_info = store.state_info();

                println!("[Migrations]");
                println!(
                    "Current: \t{}/{}",
                    state_info.migration_current(),
                    state_info.migration_total()
                );
                println!();
                println!("[Storage]");
                println!("Executable: \t{}", state_info.executable().display());
                println!("Data storage: \t{}", state_info.data_dir().display());
                println!(
                    "Content store: \t{} ({})",
                    state_info.store_dir().display(),
                    format_size(state_info.store_size())
                );
                println!(
                    "Image metadata: {} ({})",
                    state_info.metadata_file().display(),
                    format_size(state_info.metadata_size())
                );
                println!();
                println!("[Logging]");
                println!("Log directory: \t{}", state_info.log_dir().display());
                println!(
                    "Log file: \t{}",
                    state_info.log_dir().join("wasm.log").display()
                );
                Ok(())
            }
            Opts::Config => {
                // Get the global and local config paths
                let global_config_path = Config::config_path();
                let local_config_path = Config::local_config_path();

                println!("[Configuration]");
                println!("Global config:\t{}", global_config_path.display());
                if global_config_path.exists() {
                    println!("Status:\t\texists");
                } else {
                    println!("Status:\t\tnot created (will use defaults)");
                    println!();
                    println!("To create a default config file with examples, run:");
                    if let Some(parent) = global_config_path.parent() {
                        println!("  mkdir -p {}", parent.display());
                    }
                    println!("  touch {}", global_config_path.display());
                }

                println!();
                println!("Local config:\t{}", local_config_path.display());
                if local_config_path.exists() {
                    println!("Status:\t\texists");
                } else {
                    println!("Status:\t\tnot created (will use global config)");
                }

                // Load the merged config to show current settings
                let config = Config::load()?;
                println!();
                println!("[Registries]");

                // Show configured registries
                if config.registries.is_empty() {
                    println!("(none configured)");
                } else {
                    for (name, registry_config) in &config.registries {
                        let helper_status = if registry_config.credential_helper.is_some() {
                            "credential-helper configured"
                        } else {
                            "no credential-helper"
                        };
                        println!("  - {name}: {helper_status}");
                    }
                }

                // Show dotenv file detection status
                println!();
                println!("[Environment]");
                let dotenv_path = std::path::Path::new(DOTENV_PATH);
                println!("Dotenv file:\t{}", dotenv_path.display());
                if dotenv_path.exists() {
                    // Count variables defined in the file (system env vars take precedence;
                    // variables already set in the environment are not overridden).
                    let var_count = dotenvy::from_path_iter(dotenv_path)
                        .map(|iter| iter.count())
                        .unwrap_or(0);
                    println!("Status:\t\texists ({var_count} variable(s) defined in file)");
                } else {
                    println!("Status:\t\tnot found");
                }

                Ok(())
            }
            Opts::Completions { shell } => {
                let mut cmd = crate::Cli::command();
                clap_complete::generate(*shell, &mut cmd, "wasm", &mut io::stdout());
                Ok(())
            }
            Opts::ManPages => {
                let cmd = crate::Cli::command();
                let man = clap_mangen::Man::new(cmd);
                man.render(&mut io::stdout())?;
                Ok(())
            }
        }
    }
}
