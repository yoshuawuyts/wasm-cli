//! A package manager for WebAssembly components.
//!
//! This crate provides functionality to pull, store, and manage WebAssembly
//! component packages from OCI registries.
//!
//! # Example
//!
//! ```no_run
//! use wasm_package_manager::Config;
//! use wasm_package_manager::manager::Manager;
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Open the package manager (uses default config and cache location)
//!     let manager = Manager::open().await?;
//!
//!     // Pull a WebAssembly package from an OCI registry
//!     let reference = "ghcr.io/webassembly/wasi/clocks:0.2.0".parse()?;
//!     let result = manager.pull(reference).await?;
//!     println!("Pull result: {:?}", result.insert_result);
//!
//!     // Install a package by vendoring its layers into a local directory
//!     let reference = "ghcr.io/webassembly/wasi/clocks:0.2.0".parse()?;
//!     let install = manager.install(reference, Path::new("vendor")).await?;
//!     for path in &install.vendored_files {
//!         println!("Installed: {}", path.display());
//!     }
//!
//!     // List all cached images
//!     let images = manager.list_all()?;
//!     for image in &images {
//!         println!("{} ({} bytes)", image.reference(), image.size_on_disk);
//!     }
//!
//!     Ok(())
//! }
//! ```

pub mod components;
mod config;
mod credential_helper;
pub mod interfaces;
pub mod manager;
mod network;
pub mod oci;
mod progress;
pub mod storage;

pub use config::{Config, RegistryConfig};
pub use credential_helper::CredentialHelper;
pub use oci_client::Reference;
pub use progress::ProgressEvent;

/// Format a byte size as a human-readable string (B, KB, MB, GB).
#[must_use]
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(1024 * 1023), "1023.00 KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 + 512 * 1024), "1.50 MB");
        assert_eq!(format_size(1024 * 1024 * 100), "100.00 MB");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_size(1024 * 1024 * 1024 * 2), "2.00 GB");
        assert_eq!(
            format_size(1024 * 1024 * 1024 + 512 * 1024 * 1024),
            "1.50 GB"
        );
    }
}
