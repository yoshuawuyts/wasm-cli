//! Types for the WASM lockfile (`wasm.lock`).

use serde::{Deserialize, Serialize};

use crate::PackageType;

/// The current revision of the lockfile.
pub const LOCKFILE_VERSION: u32 = 3;

/// The root lockfile structure for a WASM package.
///
/// The lockfile (`deps/wasm.lock`) is auto-generated and tracks resolved dependencies
/// with their exact versions and content digests, separated into components and interfaces.
///
/// # Example
///
/// ```toml
/// lockfile_version = 3
///
/// [[components]]
/// name = "root:component"
/// version = "0.1.6"
/// registry = "ghcr.io/bytecodealliance/sample-wasi-http-rust/sample-wasi-http-rust"
/// digest = "sha256:abc123..."
///
/// [[interfaces]]
/// name = "wasi:clocks"
/// version = "0.2.5"
/// registry = "ghcr.io/webassembly/wasi/clocks"
/// digest = "sha256:def456..."
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct Lockfile {
    /// The lockfile format version.
    pub lockfile_version: u32,

    /// The list of resolved component packages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<Package>,

    /// The list of resolved interface packages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interfaces: Vec<Package>,
}

impl Default for Lockfile {
    fn default() -> Self {
        Self {
            lockfile_version: LOCKFILE_VERSION,
            components: Vec::default(),
            interfaces: Vec::default(),
        }
    }
}

impl Lockfile {
    /// Iterate over all packages with their package type.
    pub fn all_packages(&self) -> impl Iterator<Item = (&Package, PackageType)> {
        self.components
            .iter()
            .map(|p| (p, PackageType::Component))
            .chain(self.interfaces.iter().map(|p| (p, PackageType::Interface)))
    }
}

/// A resolved package entry in the lockfile.
///
/// Each package represents a dependency that has been resolved to a specific
/// version with a content digest for integrity verification.
///
/// # Example with dependencies
///
/// ```toml
/// [[interfaces]]
/// name = "wasi:key-value"
/// version = "2.0.0"
/// registry = "ghcr.io/webassembly/wasi-key-value"
/// digest = "sha256:def456..."
///
/// [[interfaces.dependencies]]
/// name = "wasi:logging"
/// version = "1.0.0"
/// ```
///
/// Note: `[[interfaces.dependencies]]` defines dependencies for the last `[[interfaces]]` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct Package {
    /// The package name (e.g., "wasi:logging").
    pub name: String,

    /// The package version (e.g., "1.0.0").
    pub version: String,

    /// The full registry path (e.g., "ghcr.io/webassembly/wasi-logging").
    pub registry: String,

    /// The content digest for integrity verification (e.g., "sha256:abc123...").
    pub digest: String,

    /// Optional dependencies of this package.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<PackageDependency>,
}

/// A dependency reference within a package.
///
/// This represents a dependency that a package has on another package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct PackageDependency {
    /// The name of the dependency package.
    pub name: String,

    /// The version of the dependency package.
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify lockfile.parse]
    #[test]
    fn test_parse_lockfile() {
        let toml = r#"
            lockfile_version = 3

            [[interfaces]]
            name = "wasi:logging"
            version = "1.0.0"
            registry = "ghcr.io/webassembly/wasi-logging"
            digest = "sha256:a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456"

            [[interfaces]]
            name = "wasi:key-value"
            version = "2.0.0"
            registry = "ghcr.io/webassembly/wasi-key-value"
            digest = "sha256:b2c3d4e5f67890123456789012345678901abcdef2345678901abcdef2345678"

            [[interfaces.dependencies]]
            name = "wasi:logging"
            version = "1.0.0"
        "#;

        let lockfile: Lockfile = toml::from_str(toml).expect("Failed to parse lockfile");

        assert_eq!(lockfile.lockfile_version, 3);
        assert_eq!(lockfile.interfaces.len(), 2);

        let logging = &lockfile.interfaces[0];
        assert_eq!(logging.name, "wasi:logging");
        assert_eq!(logging.version, "1.0.0");
        assert_eq!(logging.registry, "ghcr.io/webassembly/wasi-logging");
        assert!(logging.digest.starts_with("sha256:"));

        let key_value = &lockfile.interfaces[1];
        assert_eq!(key_value.name, "wasi:key-value");
        assert_eq!(key_value.version, "2.0.0");
        assert_eq!(key_value.dependencies.len(), 1);
        assert_eq!(key_value.dependencies[0].name, "wasi:logging");
        assert_eq!(key_value.dependencies[0].version, "1.0.0");
    }

    // r[verify lockfile.serialize]
    #[test]
    fn test_serialize_lockfile() {
        let lockfile = Lockfile {
            lockfile_version: 3,
            components: vec![],
            interfaces: vec![
                Package {
                    name: "wasi:logging".to_string(),
                    version: "1.0.0".to_string(),
                    registry: "ghcr.io/webassembly/wasi-logging".to_string(),
                    digest: "sha256:abc123".to_string(),
                    dependencies: vec![],
                },
                Package {
                    name: "wasi:key-value".to_string(),
                    version: "2.0.0".to_string(),
                    registry: "ghcr.io/webassembly/wasi-key-value".to_string(),
                    digest: "sha256:def456".to_string(),
                    dependencies: vec![PackageDependency {
                        name: "wasi:logging".to_string(),
                        version: "1.0.0".to_string(),
                    }],
                },
            ],
        };

        let toml = toml::to_string(&lockfile).expect("Failed to serialize lockfile");

        assert!(toml.contains("version = 3"));
        assert!(toml.contains("wasi:logging"));
        assert!(toml.contains("wasi:key-value"));
        assert!(toml.contains("sha256:abc123"));
    }

    // r[verify lockfile.no-dependencies.parse]
    #[test]
    fn test_package_without_dependencies() {
        let toml = r#"
            lockfile_version = 3

            [[interfaces]]
            name = "wasi:logging"
            version = "1.0.0"
            registry = "ghcr.io/webassembly/wasi-logging"
            digest = "sha256:abc123"
        "#;

        let lockfile: Lockfile = toml::from_str(toml).expect("Failed to parse lockfile");

        assert_eq!(lockfile.interfaces.len(), 1);
        assert_eq!(lockfile.interfaces[0].dependencies.len(), 0);
    }

    // r[verify lockfile.no-dependencies.serialize]
    #[test]
    fn test_serialize_package_without_dependencies() {
        let package = Package {
            name: "wasi:logging".to_string(),
            version: "1.0.0".to_string(),
            registry: "ghcr.io/webassembly/wasi-logging".to_string(),
            digest: "sha256:abc123".to_string(),
            dependencies: vec![],
        };

        let toml = toml::to_string(&package).expect("Failed to serialize package");

        // Empty dependencies should be skipped
        assert!(!toml.contains("dependencies"));
    }

    // r[verify lockfile.mixed-types.parse]
    #[test]
    fn test_components_and_interfaces() {
        let toml = r#"
            lockfile_version = 3

            [[components]]
            name = "root:component"
            version = "0.1.0"
            registry = "ghcr.io/example/component"
            digest = "sha256:comp123"

            [[interfaces]]
            name = "wasi:clocks"
            version = "0.2.5"
            registry = "ghcr.io/webassembly/wasi/clocks"
            digest = "sha256:iface456"
        "#;

        let lockfile: Lockfile = toml::from_str(toml).expect("Failed to parse lockfile");

        assert_eq!(lockfile.components.len(), 1);
        assert_eq!(lockfile.interfaces.len(), 1);
        assert_eq!(lockfile.components[0].name, "root:component");
        assert_eq!(lockfile.interfaces[0].name, "wasi:clocks");
    }

    // r[verify lockfile.mixed-types.all-packages]
    #[test]
    fn test_all_packages() {
        let lockfile = Lockfile {
            lockfile_version: 3,
            components: vec![Package {
                name: "root:component".to_string(),
                version: "0.1.0".to_string(),
                registry: "ghcr.io/example/component".to_string(),
                digest: "sha256:comp123".to_string(),
                dependencies: vec![],
            }],
            interfaces: vec![Package {
                name: "wasi:clocks".to_string(),
                version: "0.2.5".to_string(),
                registry: "ghcr.io/webassembly/wasi/clocks".to_string(),
                digest: "sha256:iface456".to_string(),
                dependencies: vec![],
            }],
        };

        let all: Vec<_> = lockfile.all_packages().collect();
        assert_eq!(all.len(), 2);

        let has_component = all.iter().any(|(_, pt)| *pt == PackageType::Component);
        let has_interface = all.iter().any(|(_, pt)| *pt == PackageType::Interface);
        assert!(has_component);
        assert!(has_interface);
    }
}
