//! CLI entry point for the wasm-meta-registry server.

use std::sync::Arc;

use clap::Parser;
use tracing::{error, info};
use wasm_package_manager::Manager;

use wasm_meta_registry::{Config, Indexer, router};

/// An HTTP server that indexes OCI registries for WebAssembly package
/// metadata and exposes a search API.
#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    /// Path to the TOML configuration file.
    config: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Read and parse configuration
    let config_str = tokio::fs::read_to_string(&cli.config).await?;
    let config = Config::from_toml(&config_str)?;

    info!(
        bind = %config.bind,
        packages = config.packages.len(),
        sync_interval = config.sync_interval,
        "Starting wasm-meta-registry"
    );

    // Open the Manager for the HTTP server
    let server_manager = Manager::open().await?;
    let state = Arc::new(std::sync::Mutex::new(server_manager));

    // Start background indexer on a dedicated thread (Manager is !Sync)
    let indexer_config = config.clone();
    let indexer_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime for indexer");
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async move {
            let manager = match Manager::open().await {
                Ok(m) => m,
                Err(e) => {
                    error!(error = %e, "Failed to open manager for indexer");
                    return;
                }
            };
            let indexer = Indexer::new(indexer_config, manager);
            indexer.run().await;
        });
    });

    // Monitor indexer thread health
    tokio::spawn(async move {
        loop {
            if indexer_handle.is_finished() {
                error!("Indexer thread has stopped unexpectedly");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });

    // Build and start HTTP server
    let app = router(state);
    let bind_addr = config.bind.clone();
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!("Listening on {}", bind_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
