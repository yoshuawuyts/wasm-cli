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
//!     let client = RegistryClient::new("http://localhost:8081");
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

#[cfg(any(all(target_os = "wasi", target_env = "p2"), feature = "client"))]
mod api_client;

#[cfg(any(all(target_os = "wasi", target_env = "p2"), feature = "client"))]
pub use api_client::{ApiClient, ApiError};

/// A declared dependency on another WIT package, as returned in the
/// `/v1/packages` response.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_client::PackageDependencyRef;
///
/// let dep = PackageDependencyRef {
///     package: "wasi:io".into(),
///     version: Some("0.2.0".into()),
/// };
/// assert_eq!(dep.package, "wasi:io");
/// assert_eq!(dep.version.as_deref(), Some("0.2.0"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageDependencyRef {
    /// Declared package name (e.g. `"wasi:io"`).
    pub package: String,
    /// Declared version, if any (e.g. `"0.2.0"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// A public view of a known package from a meta-registry.
///
/// This type matches the JSON schema returned by the `/v1/packages` endpoint
/// and is the primary wire type shared between the meta-registry server and
/// its clients.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_client::KnownPackage;
///
/// let pkg = KnownPackage {
///     registry: "ghcr.io".into(),
///     repository: "user/my-component".into(),
///     description: Some("A useful component".into()),
///     tags: vec!["v1.0.0".into(), "latest".into()],
///     signature_tags: vec![],
///     attestation_tags: vec![],
///     last_seen_at: "2025-01-01T00:00:00Z".into(),
///     created_at: "2024-06-15T12:00:00Z".into(),
///     wit_namespace: None,
///     wit_name: None,
///     dependencies: vec![],
/// };
///
/// assert_eq!(pkg.reference(), "ghcr.io/user/my-component");
/// ```
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
    /// Optional WIT namespace (e.g. `"ba"`, `"wasi"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wit_namespace: Option<String>,
    /// Optional WIT package name within the namespace (e.g. `"http"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wit_name: Option<String>,
    /// Declared WIT dependencies of this package's latest indexed version.
    ///
    /// The field MAY be omitted when no WIT metadata has been extracted for
    /// this package; omission MUST be treated as equivalent to an empty list.
    // r[impl client.known-package.dependencies]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<PackageDependencyRef>,
}

impl KnownPackage {
    /// Returns the full reference string for this package (e.g., `"ghcr.io/user/repo"`).
    ///
    /// # Example
    ///
    /// ```rust
    /// use wasm_meta_registry_client::KnownPackage;
    ///
    /// let pkg = KnownPackage {
    ///     registry: "ghcr.io".into(),
    ///     repository: "user/repo".into(),
    ///     description: None,
    ///     tags: vec![],
    ///     signature_tags: vec![],
    ///     attestation_tags: vec![],
    ///     last_seen_at: String::new(),
    ///     created_at: String::new(),
    ///     wit_namespace: None,
    ///     wit_name: None,
    ///     dependencies: vec![],
    /// };
    ///
    /// assert_eq!(pkg.reference(), "ghcr.io/user/repo");
    /// ```
    #[must_use]
    pub fn reference(&self) -> String {
        format!("{}/{}", self.registry, self.repository)
    }

    /// Returns the full reference string with the most recent tag.
    ///
    /// Uses the first tag in [`tags`](KnownPackage::tags), or `"latest"` when
    /// no tags are present.
    ///
    /// # Example
    ///
    /// ```rust
    /// use wasm_meta_registry_client::KnownPackage;
    ///
    /// let pkg = KnownPackage {
    ///     registry: "ghcr.io".into(),
    ///     repository: "user/repo".into(),
    ///     description: None,
    ///     tags: vec!["v1.0".into(), "latest".into()],
    ///     signature_tags: vec![],
    ///     attestation_tags: vec![],
    ///     last_seen_at: String::new(),
    ///     created_at: String::new(),
    ///     wit_namespace: None,
    ///     wit_name: None,
    ///     dependencies: vec![],
    /// };
    ///
    /// assert_eq!(pkg.reference_with_tag(), "ghcr.io/user/repo:v1.0");
    /// ```
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

    // r[verify client.known-package.reference]
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
            wit_namespace: None,
            wit_name: None,
            dependencies: vec![],
        };
        assert_eq!(pkg.reference(), "ghcr.io/user/repo");
    }

    // r[verify client.known-package.reference-with-tag]
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
            wit_namespace: None,
            wit_name: None,
            dependencies: vec![],
        };
        assert_eq!(pkg.reference_with_tag(), "ghcr.io/user/repo:v1.0");
    }

    // r[verify client.known-package.reference-default-tag]
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
            wit_namespace: None,
            wit_name: None,
            dependencies: vec![],
        };
        assert_eq!(pkg.reference_with_tag(), "ghcr.io/user/repo:latest");
    }

    // r[verify client.known-package.dependencies]
    #[test]
    fn known_package_dependencies_serialization() {
        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            description: None,
            tags: vec!["v1.0".into()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
            wit_namespace: Some("wasi".into()),
            wit_name: Some("http".into()),
            dependencies: vec![
                PackageDependencyRef {
                    package: "wasi:io".into(),
                    version: Some("0.2.0".into()),
                },
                PackageDependencyRef {
                    package: "wasi:clocks".into(),
                    version: None,
                },
            ],
        };

        let json = serde_json::to_string(&pkg).unwrap();
        let parsed: KnownPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.dependencies.len(), 2);
        assert_eq!(parsed.dependencies[0].package, "wasi:io");
        assert_eq!(parsed.dependencies[0].version.as_deref(), Some("0.2.0"));
        assert_eq!(parsed.dependencies[1].package, "wasi:clocks");
        assert!(parsed.dependencies[1].version.is_none());
    }

    // r[verify client.known-package.dependencies]
    #[test]
    fn known_package_empty_dependencies_skipped_in_json() {
        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            description: None,
            tags: vec![],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
            wit_namespace: None,
            wit_name: None,
            dependencies: vec![],
        };

        let json = serde_json::to_string(&pkg).unwrap();
        // Empty dependencies should not appear in JSON
        assert!(!json.contains("dependencies"));
    }
}
