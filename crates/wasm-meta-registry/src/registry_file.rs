//! Per-namespace registry file parsing.

use serde::Deserialize;

use crate::config::{PackageKind, PackageSource};

/// A per-namespace registry file.
///
/// Each file defines a single namespace with its OCI registry base path,
/// plus zero or more `[[component]]` and `[[interface]]` entries.
///
/// # Example
///
/// ```
/// use wasm_meta_registry::RegistryFile;
///
/// let toml = r#"
/// [namespace]
/// name = "wasi"
/// registry = "ghcr.io/webassembly"
///
/// [[interface]]
/// name = "io"
/// repository = "wasi/io"
/// "#;
///
/// let file = RegistryFile::from_toml(toml).unwrap();
/// assert_eq!(file.namespace.name, "wasi");
/// assert_eq!(file.interface.len(), 1);
/// ```
#[derive(Debug, Clone, Deserialize)]
#[must_use]
pub struct RegistryFile {
    /// The namespace definition.
    pub namespace: Namespace,
    /// Runnable Wasm components in this namespace.
    #[serde(default)]
    pub component: Vec<PackageEntry>,
    /// WIT interface type packages in this namespace.
    #[serde(default)]
    pub interface: Vec<PackageEntry>,
}

/// A WIT namespace mapped to an OCI registry base path.
///
/// # Example
///
/// ```
/// use wasm_meta_registry::registry_file::Namespace;
///
/// let ns = Namespace {
///     name: "wasi".to_string(),
///     registry: "ghcr.io/webassembly".to_string(),
/// };
///
/// assert_eq!(ns.name, "wasi");
/// ```
#[derive(Debug, Clone, Deserialize)]
#[must_use]
pub struct Namespace {
    /// WIT namespace name (must match the filename).
    pub name: String,
    /// OCI registry base path (e.g., "ghcr.io/webassembly").
    pub registry: String,
}

/// A package entry within a namespace.
///
/// # Example
///
/// ```
/// use wasm_meta_registry::registry_file::PackageEntry;
///
/// let entry = PackageEntry {
///     name: "clocks".to_string(),
///     repository: "wasi/clocks".to_string(),
/// };
///
/// assert_eq!(entry.name, "clocks");
/// ```
#[derive(Debug, Clone, Deserialize)]
#[must_use]
pub struct PackageEntry {
    /// Package name under the namespace (e.g., "io" for `wasi:io`).
    pub name: String,
    /// OCI repository path, relative to the namespace's registry.
    pub repository: String,
}

impl RegistryFile {
    /// Parse a per-namespace registry TOML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the TOML is invalid or missing required fields.
    ///
    /// # Example
    ///
    /// ```
    /// use wasm_meta_registry::RegistryFile;
    ///
    /// let toml = r#"
    /// [namespace]
    /// name = "wasi"
    /// registry = "ghcr.io/webassembly"
    ///
    /// [[component]]
    /// name = "my-app"
    /// repository = "wasi/my-app"
    /// "#;
    ///
    /// let file = RegistryFile::from_toml(toml).unwrap();
    /// assert_eq!(file.namespace.name, "wasi");
    /// assert_eq!(file.component.len(), 1);
    /// ```
    pub fn from_toml(toml_str: &str) -> anyhow::Result<Self> {
        let file: RegistryFile = toml::from_str(toml_str)?;
        Ok(file)
    }

    /// Convert this registry file into a list of [`PackageSource`] entries.
    ///
    /// # Example
    ///
    /// ```
    /// use wasm_meta_registry::RegistryFile;
    /// use wasm_meta_registry::config::PackageKind;
    ///
    /// let toml = r#"
    /// [namespace]
    /// name = "wasi"
    /// registry = "ghcr.io/webassembly"
    ///
    /// [[interface]]
    /// name = "io"
    /// repository = "wasi/io"
    /// "#;
    ///
    /// let file = RegistryFile::from_toml(toml).unwrap();
    /// let sources = file.into_package_sources();
    ///
    /// assert_eq!(sources.len(), 1);
    /// assert_eq!(sources[0].name, "io");
    /// assert_eq!(sources[0].kind, PackageKind::Interface);
    /// ```
    #[must_use]
    pub fn into_package_sources(self) -> Vec<PackageSource> {
        let registry = self.namespace.registry;
        let mut sources = Vec::new();
        for entry in self.component {
            sources.push(PackageSource {
                registry: registry.clone(),
                repository: entry.repository,
                name: entry.name,
                kind: PackageKind::Component,
            });
        }
        for entry in self.interface {
            sources.push(PackageSource {
                registry: registry.clone(),
                repository: entry.repository,
                name: entry.name,
                kind: PackageKind::Interface,
            });
        }
        sources
    }
}
