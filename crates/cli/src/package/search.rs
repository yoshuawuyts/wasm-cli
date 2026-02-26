//! `wasm package search` subcommand.

use anyhow::Result;
use comfy_table::{ContentArrangement, Table};
use wasm_package_manager::{Manager, SyncResult};

use crate::config::CliConfig;

/// Search for packages across configured registries.
#[derive(clap::Args)]
pub(crate) struct SearchOpts {
    /// Search query (matches package name and description).
    query: String,

    /// Maximum number of results to show.
    #[arg(long, default_value = "20")]
    limit: u32,
}

impl SearchOpts {
    pub(crate) async fn run(self, offline: bool) -> Result<()> {
        let manager = if offline {
            Manager::open_offline().await?
        } else {
            Manager::open().await?
        };

        // Attempt to sync from meta-registry if configured and not offline.
        if !offline {
            let config = CliConfig::load()?;
            if let Some(ref url) = config.registry.url {
                let interval = config.registry.sync_interval();
                match manager.sync_from_meta_registry(url, interval, false).await {
                    Ok(SyncResult::Degraded { error }) => {
                        eprintln!("warning: registry sync failed: {error}");
                    }
                    Err(e) => {
                        eprintln!("warning: {e}");
                    }
                    _ => {}
                }
            }
        }

        let packages = manager.search_packages(&self.query, 0, self.limit)?;

        if packages.is_empty() {
            println!("No packages found matching '{}'", self.query);
            return Ok(());
        }

        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["PACKAGE", "DESCRIPTION", "TAGS"]);

        for pkg in &packages {
            let reference = pkg.reference();
            let description = pkg.description.as_deref().unwrap_or("-");
            let tags = if pkg.tags.is_empty() {
                "-".to_string()
            } else {
                pkg.tags.join(", ")
            };
            table.add_row(vec![&reference, description, &tags]);
        }

        println!("{table}");
        Ok(())
    }
}
