//! `wasm package sync` subcommand.

use anyhow::Result;
use wasm_package_manager::{Manager, SyncResult};

use crate::config::CliConfig;

/// Force-sync the package index from the configured meta-registry.
#[derive(clap::Args)]
pub(crate) struct SyncOpts {}

impl SyncOpts {
    pub(crate) async fn run(self) -> Result<()> {
        let config = CliConfig::load()?;
        let interval = config.registry.sync_interval();
        let url = config.registry.url.ok_or_else(|| {
            anyhow::anyhow!(
                "No registry URL configured. Add 'registry.url' to ~/.config/wasm/config.toml"
            )
        })?;

        let manager = Manager::open().await?;

        match manager
            .sync_from_meta_registry(&url, interval, true)
            .await?
        {
            SyncResult::Updated { count } => {
                println!("Synced {count} packages from {url}");
            }
            SyncResult::NotModified => {
                println!("Already up to date");
            }
            SyncResult::Skipped => {
                println!("Already up to date");
            }
            SyncResult::Degraded { error } => {
                anyhow::bail!("Sync failed: {error}");
            }
        }

        Ok(())
    }
}
