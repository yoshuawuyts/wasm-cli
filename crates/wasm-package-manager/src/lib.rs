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
/// Core manager functionality for pulling, installing, and listing packages.
pub mod manager;
pub mod oci;
mod progress;
/// Storage layer for persisting package metadata and state.
pub mod storage;
pub mod types;

pub use config::{Config, RegistryConfig, RunConfig};
pub use credential_helper::{CredentialError, CredentialHelper};
pub use oci_client::Reference;
pub use progress::ProgressEvent;

/// Format a byte size as a human-readable string (B, KB, MB, GB).
///
/// # Examples
///
/// ```rust
/// use wasm_package_manager::format_size;
///
/// assert_eq!(format_size(0), "0 B");
/// assert_eq!(format_size(1024), "1.00 KB");
/// assert_eq!(format_size(1_048_576), "1.00 MB");
/// assert_eq!(format_size(1_073_741_824), "1.00 GB");
/// ```
// r[impl format.size.bytes]
// r[impl format.size.kilobytes]
// r[impl format.size.megabytes]
// r[impl format.size.gigabytes]
#[must_use]
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        let (whole, frac) = (bytes / GB, (bytes % GB) * 100 / GB);
        format!("{whole}.{frac:02} GB")
    } else if bytes >= MB {
        let (whole, frac) = (bytes / MB, (bytes % MB) * 100 / MB);
        format!("{whole}.{frac:02} MB")
    } else if bytes >= KB {
        let (whole, frac) = (bytes / KB, (bytes % KB) * 100 / KB);
        format!("{whole}.{frac:02} KB")
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify format.size.bytes]
    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    // r[verify format.size.kilobytes]
    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(1024 * 1023), "1023.00 KB");
    }

    // r[verify format.size.megabytes]
    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 + 512 * 1024), "1.50 MB");
        assert_eq!(format_size(1024 * 1024 * 100), "100.00 MB");
    }

    // r[verify format.size.gigabytes]
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
