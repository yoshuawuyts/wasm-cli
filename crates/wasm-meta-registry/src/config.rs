//! Configuration for the meta-registry server.
//!
//! The registry is configured as a directory of TOML files, one per WIT
//! namespace. Each file contains a `[namespace]` table mapping the namespace
//! to an OCI registry base path, plus `[[component]]` and `[[interface]]`
//! entries for individual packages.
//!
//! Server settings (`sync_interval`, `bind`) are provided via CLI arguments.

use std::path::Path;

use serde::Deserialize;

/// Top-level configuration for the meta-registry server.
///
/// Built by loading a registry directory of per-namespace TOML files
/// and combining with server settings from CLI arguments.
#[derive(Debug, Clone)]
#[must_use]
pub struct Config {
    /// Sync interval in seconds.
    pub sync_interval: u64,

    /// HTTP server bind address.
    pub bind: String,

    /// List of OCI packages to index, expanded from registry files.
    pub packages: Vec<PackageSource>,
}

/// A single OCI package source to index.
#[derive(Debug, Clone)]
#[must_use]
pub struct PackageSource {
    /// OCI registry base path (e.g., "ghcr.io/webassembly").
    pub registry: String,
    /// OCI repository path, relative to the registry (e.g., "wasi/clocks").
    pub repository: String,
    /// The package name under its namespace (e.g., "clocks").
    pub name: String,
    /// Whether the package is a component or interface type.
    pub kind: PackageKind,
}

/// The kind of package: either a runnable component or a WIT interface type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageKind {
    /// A runnable Wasm component.
    Component,
    /// A WIT interface type package.
    Interface,
}

/// A per-namespace registry file.
///
/// Each file defines a single namespace with its OCI registry base path,
/// plus zero or more `[[component]]` and `[[interface]]` entries.
///
/// # Example
///
/// ```toml
/// [namespace]
/// name = "wasi"
/// registry = "ghcr.io/webassembly"
///
/// [[interface]]
/// name = "io"
/// repository = "wasi/io"
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
#[derive(Debug, Clone, Deserialize)]
#[must_use]
pub struct Namespace {
    /// WIT namespace name (must match the filename).
    pub name: String,
    /// OCI registry base path (e.g., "ghcr.io/webassembly").
    pub registry: String,
}

/// A package entry within a namespace.
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
    pub fn from_toml(toml_str: &str) -> anyhow::Result<Self> {
        let file: RegistryFile = toml::from_str(toml_str)?;
        Ok(file)
    }

    /// Convert this registry file into a list of [`PackageSource`] entries.
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

impl Config {
    /// Load configuration from a registry directory.
    ///
    /// Reads all `*.toml` files in the given directory, parses each as a
    /// [`RegistryFile`], and combines them with the provided server settings.
    ///
    /// Each file's name (without extension) must match the `namespace.name`
    /// field inside it.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be read, any TOML file is
    /// invalid, or a filename does not match its namespace name.
    pub fn from_registry_dir(dir: &Path, sync_interval: u64, bind: String) -> anyhow::Result<Self> {
        let mut packages = Vec::new();

        let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);

        for entry in entries {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                let content = std::fs::read_to_string(&path)?;
                let registry_file = RegistryFile::from_toml(&content)?;

                let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
                    anyhow::anyhow!("registry filename is not valid UTF-8: {}", path.display())
                })?;
                if stem != registry_file.namespace.name {
                    anyhow::bail!(
                        "filename '{stem}.toml' does not match namespace name '{}'",
                        registry_file.namespace.name
                    );
                }

                packages.extend(registry_file.into_package_sources());
            }
        }

        Ok(Config {
            sync_interval,
            bind,
            packages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_registry_file_with_interfaces() {
        let toml = r#"
[namespace]
name = "wasi"
registry = "ghcr.io/webassembly"

[[interface]]
name = "io"
repository = "wasi/io"

[[interface]]
name = "clocks"
repository = "wasi/clocks"
"#;

        let file = RegistryFile::from_toml(toml).unwrap();
        assert_eq!(file.namespace.name, "wasi");
        assert_eq!(file.namespace.registry, "ghcr.io/webassembly");
        assert!(file.component.is_empty());
        assert_eq!(file.interface.len(), 2);
        assert_eq!(file.interface[0].name, "io");
        assert_eq!(file.interface[0].repository, "wasi/io");
        assert_eq!(file.interface[1].name, "clocks");
        assert_eq!(file.interface[1].repository, "wasi/clocks");
    }

    #[test]
    fn test_parse_registry_file_with_components() {
        let toml = r#"
[namespace]
name = "microsoft"
registry = "ghcr.io/microsoft"

[[component]]
name = "fetch-rs"
repository = "fetch-rs"

[[component]]
name = "eval-py"
repository = "eval-py"
"#;

        let file = RegistryFile::from_toml(toml).unwrap();
        assert_eq!(file.namespace.name, "microsoft");
        assert_eq!(file.namespace.registry, "ghcr.io/microsoft");
        assert_eq!(file.component.len(), 2);
        assert!(file.interface.is_empty());
        assert_eq!(file.component[0].name, "fetch-rs");
        assert_eq!(file.component[1].name, "eval-py");
    }

    #[test]
    fn test_parse_registry_file_mixed() {
        let toml = r#"
[namespace]
name = "example"
registry = "ghcr.io/example"

[[component]]
name = "my-app"
repository = "my-app"

[[interface]]
name = "my-api"
repository = "my-api"
"#;

        let file = RegistryFile::from_toml(toml).unwrap();
        assert_eq!(file.component.len(), 1);
        assert_eq!(file.interface.len(), 1);
    }

    #[test]
    fn test_parse_registry_file_namespace_only() {
        let toml = r#"
[namespace]
name = "empty"
registry = "ghcr.io/empty"
"#;

        let file = RegistryFile::from_toml(toml).unwrap();
        assert_eq!(file.namespace.name, "empty");
        assert!(file.component.is_empty());
        assert!(file.interface.is_empty());
    }

    #[test]
    fn test_parse_registry_file_invalid_toml() {
        let toml = "this is not valid toml [[[";
        assert!(RegistryFile::from_toml(toml).is_err());
    }

    #[test]
    fn test_parse_registry_file_missing_namespace() {
        let toml = r#"
[[component]]
name = "foo"
repository = "foo"
"#;
        assert!(RegistryFile::from_toml(toml).is_err());
    }

    #[test]
    fn test_parse_registry_file_missing_entry_fields() {
        let toml = r#"
[namespace]
name = "test"
registry = "ghcr.io/test"

[[component]]
name = "foo"
"#;
        // repository is required
        assert!(RegistryFile::from_toml(toml).is_err());
    }

    #[test]
    fn test_into_package_sources() {
        let toml = r#"
[namespace]
name = "wasi"
registry = "ghcr.io/webassembly"

[[component]]
name = "my-component"
repository = "wasi/my-component"

[[interface]]
name = "io"
repository = "wasi/io"
"#;

        let file = RegistryFile::from_toml(toml).unwrap();
        let sources = file.into_package_sources();
        assert_eq!(sources.len(), 2);

        let component = &sources[0];
        assert_eq!(component.registry, "ghcr.io/webassembly");
        assert_eq!(component.repository, "wasi/my-component");
        assert_eq!(component.name, "my-component");
        assert_eq!(component.kind, PackageKind::Component);

        let interface = &sources[1];
        assert_eq!(interface.registry, "ghcr.io/webassembly");
        assert_eq!(interface.repository, "wasi/io");
        assert_eq!(interface.name, "io");
        assert_eq!(interface.kind, PackageKind::Interface);
    }

    #[test]
    fn test_from_registry_dir() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(
            dir.path().join("wasi.toml"),
            r#"
[namespace]
name = "wasi"
registry = "ghcr.io/webassembly"

[[interface]]
name = "io"
repository = "wasi/io"
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("ba.toml"),
            r#"
[namespace]
name = "ba"
registry = "ghcr.io/bytecodealliance"

[[component]]
name = "sample-wasi-http-rust"
repository = "sample-wasi-http-rust/sample-wasi-http-rust"
"#,
        )
        .unwrap();

        let config =
            Config::from_registry_dir(dir.path(), 1800, "127.0.0.1:9090".to_string()).unwrap();
        assert_eq!(config.sync_interval, 1800);
        assert_eq!(config.bind, "127.0.0.1:9090");
        assert_eq!(config.packages.len(), 2);
    }

    #[test]
    fn test_from_registry_dir_filename_mismatch() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(
            dir.path().join("wrong.toml"),
            r#"
[namespace]
name = "wasi"
registry = "ghcr.io/webassembly"
"#,
        )
        .unwrap();

        let result = Config::from_registry_dir(dir.path(), 3600, "0.0.0.0:8080".to_string());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not match"));
    }

    #[test]
    fn test_from_registry_dir_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config =
            Config::from_registry_dir(dir.path(), 3600, "0.0.0.0:8080".to_string()).unwrap();
        assert!(config.packages.is_empty());
    }

    #[test]
    fn test_from_registry_dir_ignores_non_toml() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(dir.path().join("readme.txt"), "not a toml file").unwrap();

        fs::write(
            dir.path().join("wasi.toml"),
            r#"
[namespace]
name = "wasi"
registry = "ghcr.io/webassembly"

[[interface]]
name = "io"
repository = "wasi/io"
"#,
        )
        .unwrap();

        let config =
            Config::from_registry_dir(dir.path(), 3600, "0.0.0.0:8080".to_string()).unwrap();
        assert_eq!(config.packages.len(), 1);
    }
}
