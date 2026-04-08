//! HTTP client for fetching package metadata from a `wasm-meta-registry`
//! instance.
//!
//! This crate provides:
//!
//! - [`KnownPackage`] — the shared wire type returned by the meta-registry
//!   `/v1/packages` endpoint (re-exported from `wasm-meta-registry-types`).
//! - [`RegistryClient`] and [`ApiError`] — an HTTP client that speaks the
//!   meta-registry protocol, supporting search, pagination, and package
//!   lookups. On native targets with the **`client`** feature, also provides
//!   [`FetchResult`] for ETag-based conditional fetches with
//!   exponential-backoff retries.
//!
//! # Example
//!
//! ```no_run
//! use wasm_meta_registry_client::RegistryClient;
//!
//! # async fn example() -> Result<(), wasm_meta_registry_client::ApiError> {
//! let client = RegistryClient::new("http://localhost:8081");
//! let packages = client.fetch_recent_packages(10).await?;
//! for pkg in &packages {
//!     println!("{}", pkg.reference());
//! }
//! # Ok(())
//! # }
//! ```

#[cfg(any(all(target_os = "wasi", target_env = "p2"), feature = "client"))]
mod client;

#[cfg(any(all(target_os = "wasi", target_env = "p2"), feature = "client"))]
pub use client::{ApiError, RegistryClient};

#[cfg(feature = "client")]
pub use client::FetchResult;

// Re-export all wire types from the types crate so existing consumers
// (`wasm_meta_registry_client::KnownPackage`, etc.) keep working.
pub use wasm_meta_registry_types::*;

// Tests for wire types live in `wasm-meta-registry-types`. The tests below
// verify that the re-exports work correctly from this crate.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexported_types_are_accessible() {
        let dep = PackageDependencyRef {
            package: "wasi:io".into(),
            version: Some("0.2.0".into()),
        };
        assert_eq!(dep.package, "wasi:io");

        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            kind: None,
            description: None,
            tags: vec!["v1.0".into()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
            wit_namespace: None,
            wit_name: None,
            dependencies: vec![dep],
        };
        assert_eq!(pkg.reference(), "ghcr.io/user/repo");
        assert_eq!(pkg.reference_with_tag(), "ghcr.io/user/repo:v1.0");
    }
}
