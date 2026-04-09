//! Configuration for the meta-registry server.
//!
//! The registry is configured as a directory of TOML files, one per WIT
//! namespace. Each file contains a `[namespace]` table mapping the namespace
//! to an OCI registry base path, plus `[[component]]` and `[[interface]]`
//! entries for individual packages. An optional `engines.toml` file can define
//! host runtime interface support information.
//!
//! Server settings (`sync_interval`, `bind`) are provided via CLI arguments.

use std::path::Path;

use crate::registry_file::RegistryFile;
use serde::Deserialize;

/// Top-level configuration for the meta-registry server.
///
/// Built by loading a registry directory of per-namespace TOML files
/// and combining with server settings from CLI arguments.
///
/// # Example
///
/// ```
/// use wasm_meta_registry::config::{Config, PackageSource, PackageKind};
///
/// let config = Config {
///     sync_interval: 3600,
///     bind: "0.0.0.0:8080".to_string(),
///     packages: vec![PackageSource {
///         registry: "ghcr.io/webassembly".to_string(),
///         repository: "wasi/io".to_string(),
///         namespace: "wasi".to_string(),
///         name: "io".to_string(),
///         kind: PackageKind::Interface,
///     }],
///     engines: vec![],
/// };
///
/// assert_eq!(config.sync_interval, 3600);
/// assert_eq!(config.packages.len(), 1);
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct Config {
    /// Sync interval in seconds.
    pub sync_interval: u64,

    /// HTTP server bind address.
    pub bind: String,

    /// List of OCI packages to index, expanded from registry files.
    pub packages: Vec<PackageSource>,

    /// Host runtimes and the interfaces they support.
    pub engines: Vec<HostEngine>,
}

/// A single OCI package source to index.
///
/// # Example
///
/// ```
/// use wasm_meta_registry::config::{PackageSource, PackageKind};
///
/// let source = PackageSource {
///     registry: "ghcr.io/webassembly".to_string(),
///     repository: "wasi/clocks".to_string(),
///     namespace: "wasi".to_string(),
///     name: "clocks".to_string(),
///     kind: PackageKind::Interface,
/// };
///
/// assert_eq!(source.registry, "ghcr.io/webassembly");
/// assert_eq!(source.name, "clocks");
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct PackageSource {
    /// OCI registry base path (e.g., "ghcr.io/webassembly").
    pub registry: String,
    /// OCI repository path, relative to the registry (e.g., "wasi/clocks").
    pub repository: String,
    /// The WIT namespace this package belongs to (e.g., "wasi", "ba").
    pub namespace: String,
    /// The package name under its namespace (e.g., "clocks").
    pub name: String,
    /// Whether the package is a component or interface type.
    pub kind: PackageKind,
}

/// Re-export from the shared types crate.
pub use wasm_meta_registry_types::{HostEngine, PackageKind};

#[derive(Debug, Deserialize)]
struct EnginesFile {
    #[serde(default)]
    engine: Vec<HostEngine>,
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
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use wasm_meta_registry::Config;
    ///
    /// let config = Config::from_registry_dir(
    ///     Path::new("registry/"),
    ///     3600,
    ///     "0.0.0.0:8080".to_string(),
    /// )
    /// .expect("failed to load registry config");
    ///
    /// println!("Loaded {} packages", config.packages.len());
    /// ```
    pub fn from_registry_dir(dir: &Path, sync_interval: u64, bind: String) -> anyhow::Result<Self> {
        let mut packages = Vec::new();
        let mut engines = Vec::new();

        let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);

        for entry in entries {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                let content = std::fs::read_to_string(&path)?;
                let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
                    anyhow::anyhow!("registry filename is not valid UTF-8: {}", path.display())
                })?;

                if stem == "engines" {
                    let engines_file: EnginesFile = toml::from_str(&content)?;
                    engines.extend(engines_file.engine);
                    continue;
                }

                let registry_file = RegistryFile::from_toml(&content)?;
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
            engines,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry_file::RegistryFile;
    use std::fs;

    // r[verify registry.parse.interfaces]
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

    // r[verify registry.parse.components]
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

    // r[verify registry.parse.mixed]
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

    // r[verify registry.parse.namespace-only]
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

    // r[verify registry.parse.invalid-toml]
    #[test]
    fn test_parse_registry_file_invalid_toml() {
        let toml = "this is not valid toml [[[";
        assert!(RegistryFile::from_toml(toml).is_err());
    }

    // r[verify registry.parse.missing-namespace]
    #[test]
    fn test_parse_registry_file_missing_namespace() {
        let toml = r#"
[[component]]
name = "foo"
repository = "foo"
"#;
        assert!(RegistryFile::from_toml(toml).is_err());
    }

    // r[verify registry.parse.missing-fields]
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

    // r[verify registry.sources.convert]
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
        assert_eq!(component.namespace, "wasi");
        assert_eq!(component.name, "my-component");
        assert_eq!(component.kind, PackageKind::Component);

        let interface = &sources[1];
        assert_eq!(interface.registry, "ghcr.io/webassembly");
        assert_eq!(interface.repository, "wasi/io");
        assert_eq!(interface.namespace, "wasi");
        assert_eq!(interface.name, "io");
        assert_eq!(interface.kind, PackageKind::Interface);
    }

    // r[verify registry.dir.load]
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
        assert!(config.engines.is_empty());
    }

    // r[verify registry.dir.filename-match]
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

    // r[verify registry.dir.empty]
    #[test]
    fn test_from_registry_dir_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config =
            Config::from_registry_dir(dir.path(), 3600, "0.0.0.0:8080".to_string()).unwrap();
        assert!(config.packages.is_empty());
        assert!(config.engines.is_empty());
    }

    // r[verify registry.dir.ignore-non-toml]
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
        assert!(config.engines.is_empty());
    }

    // r[verify registry.engines.load]
    #[test]
    fn test_from_registry_dir_loads_engines_file() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(
            dir.path().join("engines.toml"),
            r#"
[[engine]]
name = "wasmtime"
homepage = "https://wasmtime.dev"

[[engine.interfaces]]
interface = "wasi:http"
versions = ["0.2.0", "0.3.0"]
"#,
        )
        .unwrap();

        let config =
            Config::from_registry_dir(dir.path(), 3600, "0.0.0.0:8080".to_string()).unwrap();
        assert!(config.packages.is_empty());
        assert_eq!(config.engines.len(), 1);
        assert_eq!(config.engines[0].name, "wasmtime");
        assert_eq!(config.engines[0].notes, None);
        assert_eq!(config.engines[0].interfaces.len(), 1);
        assert_eq!(config.engines[0].interfaces[0].interface, "wasi:http");
    }
}
