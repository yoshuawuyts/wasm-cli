//! CLI entry point for the wasm-meta-registry server.

use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use tracing::{error, info};
use wasm_package_manager::manager::Manager;

use wasm_meta_registry::server::StateData;
use wasm_meta_registry::{Config, Indexer, router};

/// An HTTP server that indexes OCI registries for WebAssembly package
/// metadata and exposes a search API.
#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    /// Path to the registry directory containing per-namespace TOML files.
    registry_dir: std::path::PathBuf,

    /// Data directory for the registry's own cache (separate from the CLI cache).
    /// Defaults to `<OS data dir>/wasm-registry`.
    #[arg(long)]
    data_dir: Option<std::path::PathBuf>,

    /// Sync interval in seconds.
    #[arg(long, default_value_t = 3600)]
    sync_interval: u64,

    /// HTTP server bind address.
    #[arg(long, default_value = "0.0.0.0:8080")]
    bind: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Read and parse configuration from registry directory
    let config = Config::from_registry_dir(&cli.registry_dir, cli.sync_interval, cli.bind)?;

    // Determine the registry data directory (separate from the CLI cache)
    let data_dir = match cli.data_dir {
        Some(dir) => dir,
        None => dirs::data_local_dir()
            .context("No local data dir known for the current OS")?
            .join("wasm-registry"),
    };

    info!(
        bind = %config.bind,
        packages = config.packages.len(),
        engines = config.engines.len(),
        sync_interval = config.sync_interval,
        data_dir = %data_dir.display(),
        "Starting wasm-meta-registry"
    );

    // Open the Manager for the HTTP server with its own data directory
    let server_manager = Manager::open_at(&data_dir).await?;
    let state = Arc::new(StateData {
        manager: std::sync::Mutex::new(server_manager),
        engines: config.engines.clone(),
    });

    // Start background indexer on a dedicated thread (Manager is !Sync)
    let indexer_config = config.clone();
    let indexer_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime for indexer");
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async move {
            let manager = match Manager::open_at(&data_dir).await {
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
