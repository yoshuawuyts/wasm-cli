//! Background indexer that syncs package metadata from OCI registries.
//!
//! The indexer periodically iterates over configured package sources, fetches
//! tags and metadata, and stores them in the local database via `Manager`.
//!
//! The indexer uses its own `Manager` instance, separate from the HTTP server's
//! instance. SQLite in WAL mode allows concurrent readers and a single writer,
//! making this safe.

use std::time::Duration;

use tracing::{error, info, warn};
use wasm_package_manager::Reference;
use wasm_package_manager::manager::Manager;

use crate::config::Config;

/// Background indexer that syncs package metadata from OCI registries.
#[derive(Debug)]
pub struct Indexer {
    config: Config,
    manager: Manager,
}

impl Indexer {
    /// Create a new indexer with the given configuration and its own manager.
    #[must_use]
    pub fn new(config: Config, manager: Manager) -> Self {
        Self { config, manager }
    }

    /// Run a single sync cycle, indexing all configured packages.
    ///
    /// This fetches metadata for each configured package source without
    /// downloading any wasm layers.
    pub async fn sync(&mut self) {
        info!(
            "Starting sync cycle for {} packages",
            self.config.packages.len()
        );

        for source in &self.config.packages {
            let reference_str = format!("{}/{}", source.registry, source.repository);
            let reference = match reference_str.parse::<Reference>() {
                Ok(r) => r,
                Err(e) => {
                    warn!(
                        registry = %source.registry,
                        repository = %source.repository,
                        error = %e,
                        "Failed to parse package reference, skipping"
                    );
                    continue;
                }
            };

            match self.manager.index_package(&reference).await {
                Ok(pkg) => {
                    info!(
                        registry = %pkg.registry,
                        repository = %pkg.repository,
                        tags = pkg.tags.len(),
                        "Indexed package"
                    );
                }
                Err(e) => {
                    error!(
                        registry = %source.registry,
                        repository = %source.repository,
                        error = %e,
                        "Failed to index package"
                    );
                }
            }
        }

        info!("Sync cycle complete");
    }

    /// Run the indexer in a loop, syncing at the configured interval.
    ///
    /// This method runs indefinitely and should be spawned as a background task.
    pub async fn run(mut self) {
        let interval = Duration::from_secs(self.config.sync_interval);

        // Run an initial sync immediately
        self.sync().await;

        loop {
            tokio::time::sleep(interval).await;
            self.sync().await;
        }
    }
}
