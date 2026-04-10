//! A meta-registry HTTP server for WebAssembly package discovery.
//!
//! This crate indexes OCI registries for package metadata and exposes a
//! search API. It reads a directory of per-namespace TOML registry files,
//! periodically syncs manifest and config metadata via `wasm-package-manager`,
//! and serves search results over HTTP.
//!
//! # Registry format
//!
//! The registry is a directory of TOML files, one per WIT namespace:
//!
//! ```text
//! registry/
//!   wasi.toml
//!   ba.toml
//!   engines.toml
//! ```
//!
//! Each file defines a `[namespace]` table and `[[component]]`/`[[interface]]`
//! entries:
//!
//! ```toml
//! [namespace]
//! name = "wasi"
//! registry = "ghcr.io/webassembly"
//!
//! [[interface]]
//! name = "io"
//! repository = "wasi/io"
//! ```
//!
//! `engines.toml` can optionally declare host runtime support data:
//!
//! ```toml
//! [[engine]]
//! name = "wasmtime"
//!
//! [[engine.interfaces]]
//! interface = "wasi:http"
//! versions = ["0.2.0"]
//! ```
//!
//! # Example
//!
//! ```no_run
//! use wasm_meta_registry::{Config, Indexer, router};
//! use wasm_meta_registry::server::StateData;
//! use wasm_package_manager::manager::Manager;
//! use std::sync::{Arc, Mutex};
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Load configuration from a registry directory
//!     let config = Config::from_registry_dir(
//!         Path::new("registry/"),
//!         3600,
//!         "0.0.0.0:8080".to_string(),
//!     )?;
//!
//!     // Create the HTTP router backed by a package manager with its own data directory
//!     let manager = Manager::open_at("/tmp/wasm-registry").await?;
//!     let state = Arc::new(StateData {
//!         manager: Mutex::new(manager),
//!         engines: vec![],
//!     });
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
pub mod registry_file;
pub mod server;

pub use config::Config;
pub use indexer::Indexer;
pub use registry_file::RegistryFile;
pub use server::router;
