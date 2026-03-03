//! HTTP client for fetching package metadata from a `wasm-meta-registry`
//! instance.
//!
//! This crate provides:
//!
//! - [`KnownPackage`] — the shared wire type returned by the meta-registry
//!   `/v1/packages` endpoint.
//! - [`RegistryClient`] and [`FetchResult`] — an HTTP client that speaks the
//!   meta-registry protocol, with ETag-based conditional fetches and
//!   exponential-backoff retries (requires the **`client`** feature, enabled
//!   by default).
//!
//! # Example
//!
//! ```no_run
//! use wasm_meta_registry_client::{KnownPackage, RegistryClient, FetchResult};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = RegistryClient::new("http://localhost:3000");
//!     match client.fetch_packages(None, 100).await? {
//!         FetchResult::NotModified => println!("up to date"),
//!         FetchResult::Updated { packages, .. } => {
//!             for pkg in &packages {
//!                 println!("{}", pkg.reference());
//!             }
//!         }
//!     }
//!     Ok(())
//! }
//! ```

#[cfg(feature = "client")]
mod client;

#[cfg(feature = "client")]
pub use client::{FetchResult, RegistryClient};

/// A public view of a known package from a meta-registry.
///
/// This type matches the JSON schema returned by the `/v1/packages` endpoint
/// and is the primary wire type shared between the meta-registry server and
/// its clients.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnownPackage {
    /// Registry hostname (e.g. `"ghcr.io"`).
    pub registry: String,
    /// Repository path (e.g. `"user/repo"`).
    pub repository: String,
    /// Optional package description.
    pub description: Option<String>,
    /// Release tags.
    pub tags: Vec<String>,
    /// Signature tags (kept for API compatibility, always empty).
    #[serde(default)]
    pub signature_tags: Vec<String>,
    /// Attestation tags (kept for API compatibility, always empty).
    #[serde(default)]
    pub attestation_tags: Vec<String>,
    /// Timestamp of last seen.
    pub last_seen_at: String,
    /// Timestamp of creation.
    pub created_at: String,
}

impl KnownPackage {
    /// Returns the full reference string for this package (e.g., `"ghcr.io/user/repo"`).
    #[must_use]
    pub fn reference(&self) -> String {
        format!("{}/{}", self.registry, self.repository)
    }

    /// Returns the full reference string with the most recent tag.
    #[must_use]
    pub fn reference_with_tag(&self) -> String {
        if let Some(tag) = self.tags.first() {
            format!("{}:{}", self.reference(), tag)
        } else {
            format!("{}:latest", self.reference())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_package_reference() {
        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            description: None,
            tags: vec![],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
        };
        assert_eq!(pkg.reference(), "ghcr.io/user/repo");
    }

    #[test]
    fn known_package_reference_with_tag() {
        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            description: None,
            tags: vec!["v1.0".into(), "latest".into()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
        };
        assert_eq!(pkg.reference_with_tag(), "ghcr.io/user/repo:v1.0");
    }

    #[test]
    fn known_package_reference_with_tag_default() {
        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            description: None,
            tags: vec![],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
        };
        assert_eq!(pkg.reference_with_tag(), "ghcr.io/user/repo:latest");
    }
}
