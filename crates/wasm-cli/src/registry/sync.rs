//! `wasm registry sync` subcommand.

use anyhow::Result;
use wasm_package_manager::manager::{Manager, SyncPolicy, SyncResult};

/// Default meta-registry URL.
const REGISTRY_URL: &str = "http://localhost:8080";

/// Default sync interval in seconds (1 hour).
const SYNC_INTERVAL: u64 = 3600;

/// Force-sync the package index from the configured meta-registry.
#[derive(clap::Args)]
pub(crate) struct SyncOpts {}

impl SyncOpts {
    pub(crate) async fn run(self) -> Result<()> {
        let manager = Manager::open().await?;

        match manager
            .sync_from_meta_registry(REGISTRY_URL, SYNC_INTERVAL, SyncPolicy::Force)
            .await?
        {
            SyncResult::Updated { count } => {
                println!("Synced {count} packages from {REGISTRY_URL}");
            }
            SyncResult::NotModified => {
                println!("Already up to date (verified with registry)");
            }
            SyncResult::Skipped => {
                println!("Already up to date (synced recently)");
            }
            SyncResult::Degraded { error } => {
                return Err(super::errors::SyncError::Degraded { reason: error }.into());
            }
        }

        Ok(())
    }
}
