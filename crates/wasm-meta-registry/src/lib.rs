//! A meta-registry HTTP server for WebAssembly package discovery.
//!
//! This crate indexes OCI registries for package metadata and exposes a
//! search API. It takes a TOML config listing repositories, periodically
//! syncs manifest and config metadata via `wasm-package-manager`, and serves
//! search results over HTTP.
//!
//! # Example
//!
//! ```no_run
//! use wasm_meta_registry::{Config, Indexer, router};
//! use wasm_package_manager::Manager;
//! use std::sync::{Arc, Mutex};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Parse a TOML configuration
//!     let config = Config::from_toml(r#"
//!         sync_interval = 3600
//!         bind = "0.0.0.0:8080"
//!
//!         [[packages]]
//!         registry = "ghcr.io"
//!         repository = "webassembly/wasi/clocks"
//!     "#)?;
//!
//!     // Create the HTTP router backed by a package manager
//!     let manager = Manager::open().await?;
//!     let state = Arc::new(Mutex::new(manager));
//!     let app = router(state);
//!
//!     // Start the server
//!     let listener = tokio::net::TcpListener::bind(&config.bind).await?;
//!     axum::serve(listener, app).await?;
//!
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod indexer;
pub mod server;

pub use config::Config;
pub use indexer::Indexer;
pub use server::router;
