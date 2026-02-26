//! A meta-registry HTTP server for WebAssembly package discovery.
//!
//! This crate indexes OCI registries for package metadata and exposes a
//! search API. It takes a TOML config listing repositories, periodically
//! syncs manifest and config metadata via `wasm-package-manager`, and serves
//! search results over HTTP.

pub mod config;
pub mod indexer;
pub mod server;

pub use config::Config;
pub use indexer::Indexer;
pub use server::router;
