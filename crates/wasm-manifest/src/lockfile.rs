//! Types for the WASM lockfile (`wasm.lock`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::PackageType;

/// The current revision of the lockfile.
///
/// # Example
///
/// ```rust
/// use wasm_manifest::LOCKFILE_VERSION;
///
/// assert_eq!(LOCKFILE_VERSION, 3);
/// ```
pub const LOCKFILE_VERSION: u32 = 3;

/// The root lockfile structure for a WASM package.
///
/// The lockfile (`wasm.lock.toml`) is auto-generated and tracks resolved dependencies
/// with their exact versions and content digests, separated into components and interfaces.
///
/// # Example
///
/// ```rust
/// use wasm_manifest::Lockfile;
///
/// let toml = r#"
/// lockfile_version = 3
///
/// [[interfaces]]
/// name = "wasi:logging"
/// version = "1.0.0"
/// registry = "ghcr.io/webassembly/wasi-logging"
/// digest = "sha256:abc123"
/// "#;
///
/// let lockfile: Lockfile = toml::from_str(toml).unwrap();
/// assert_eq!(lockfile.lockfile_version, 3);
/// assert_eq!(lockfile.interfaces.len(), 1);
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
    ///
    /// # Example
    ///
    /// ```rust
    /// use wasm_manifest::{Lockfile, PackageType};
    ///
    /// let toml = r#"
    /// lockfile_version = 3
    ///
    /// [[components]]
    /// name = "root:component"
    /// version = "0.1.0"
    /// registry = "ghcr.io/example/component"
    /// digest = "sha256:comp123"
    ///
    /// [[interfaces]]
    /// name = "wasi:clocks"
    /// version = "0.2.5"
    /// registry = "ghcr.io/webassembly/wasi/clocks"
    /// digest = "sha256:iface456"
    /// "#;
    ///
    /// let lockfile: Lockfile = toml::from_str(toml).unwrap();
    /// let all: Vec<_> = lockfile.all_packages().collect();
    /// assert_eq!(all.len(), 2);
    /// assert!(all.iter().any(|(_, pt)| *pt == PackageType::Component));
    /// assert!(all.iter().any(|(_, pt)| *pt == PackageType::Interface));
    /// ```
    pub fn all_packages(&self) -> impl Iterator<Item = (&Package, PackageType)> {
        self.components
            .iter()
            .map(|p| (p, PackageType::Component))
            .chain(self.interfaces.iter().map(|p| (p, PackageType::Interface)))
    }

    /// Backfill `registry` and `digest` on every [`PackageDependency`] by
    /// looking up the matching top-level [`Package`] entry (matched by name).
    ///
    /// Call this after all packages have been inserted into the lockfile so
    /// that every dependency reference carries the resolved registry path and
    /// content digest.
    pub fn resolve_dependency_details(&mut self) {
        // Build a lookup from package name → (registry, digest).
        let lookup: HashMap<String, (String, String)> = self
            .components
            .iter()
            .chain(self.interfaces.iter())
            .map(|p| (p.name.clone(), (p.registry.clone(), p.digest.clone())))
            .collect();

        for pkg in self.components.iter_mut().chain(self.interfaces.iter_mut()) {
            for dep in &mut pkg.dependencies {
                if let Some((registry, digest)) = lookup.get(&dep.name) {
                    dep.registry.clone_from(registry);
                    dep.digest.clone_from(digest);
                }
            }
        }
    }
}

/// A resolved package entry in the lockfile.
///
/// Each package represents a dependency that has been resolved to a specific
/// version with a content digest for integrity verification.
///
/// # Example
///
/// ```rust
/// use wasm_manifest::Lockfile;
///
/// let toml = r#"
/// lockfile_version = 3
///
/// [[interfaces]]
/// name = "wasi:key-value"
/// version = "2.0.0"
/// registry = "ghcr.io/webassembly/wasi-key-value"
/// digest = "sha256:def456"
///
/// [[interfaces.dependencies]]
/// name = "wasi:logging"
/// version = "1.0.0"
/// registry = "ghcr.io/webassembly/wasi-logging"
/// digest = "sha256:abc123"
/// "#;
///
/// let lockfile: Lockfile = toml::from_str(toml).unwrap();
/// let pkg = &lockfile.interfaces[0];
/// assert_eq!(pkg.name, "wasi:key-value");
/// assert_eq!(pkg.dependencies.len(), 1);
/// assert_eq!(pkg.dependencies[0].name, "wasi:logging");
/// ```
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
/// This represents a dependency that a package has on another package,
/// fully resolved with registry path and content digest.
///
/// # Example
///
/// ```rust
/// use wasm_manifest::PackageDependency;
///
/// let dep = PackageDependency {
///     name: "wasi:logging".to_string(),
///     version: "1.0.0".to_string(),
///     registry: "ghcr.io/webassembly/wasi-logging".to_string(),
///     digest: "sha256:abc123".to_string(),
/// };
/// assert_eq!(dep.name, "wasi:logging");
/// assert_eq!(dep.registry, "ghcr.io/webassembly/wasi-logging");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct PackageDependency {
    /// The name of the dependency package.
    pub name: String,

    /// The version of the dependency package.
    pub version: String,

    /// The full registry path (e.g., "ghcr.io/webassembly/wasi-logging").
    pub registry: String,

    /// The content digest for integrity verification (e.g., "sha256:abc123...").
    pub digest: String,
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
            registry = "ghcr.io/webassembly/wasi-logging"
            digest = "sha256:a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456"
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
        assert_eq!(
            key_value.dependencies[0].registry,
            "ghcr.io/webassembly/wasi-logging"
        );
        assert!(key_value.dependencies[0].digest.starts_with("sha256:"));
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
                        registry: "ghcr.io/webassembly/wasi-logging".to_string(),
                        digest: "sha256:abc123".to_string(),
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

    // r[verify lockfile.required-fields]
    #[test]
    fn test_dependency_requires_registry_and_digest() {
        // A dependency entry missing `registry` must fail to parse.
        let toml_missing_registry = r#"
            lockfile_version = 3

            [[interfaces]]
            name = "wasi:key-value"
            version = "2.0.0"
            registry = "ghcr.io/webassembly/wasi-key-value"
            digest = "sha256:def456"

            [[interfaces.dependencies]]
            name = "wasi:logging"
            version = "1.0.0"
            digest = "sha256:abc123"
        "#;
        assert!(
            toml::from_str::<Lockfile>(toml_missing_registry).is_err(),
            "parsing should fail when dependency is missing 'registry'"
        );

        // A dependency entry missing `digest` must fail to parse.
        let toml_missing_digest = r#"
            lockfile_version = 3

            [[interfaces]]
            name = "wasi:key-value"
            version = "2.0.0"
            registry = "ghcr.io/webassembly/wasi-key-value"
            digest = "sha256:def456"

            [[interfaces.dependencies]]
            name = "wasi:logging"
            version = "1.0.0"
            registry = "ghcr.io/webassembly/wasi-logging"
        "#;
        assert!(
            toml::from_str::<Lockfile>(toml_missing_digest).is_err(),
            "parsing should fail when dependency is missing 'digest'"
        );
    }

    #[test]
    fn test_resolve_dependency_details() {
        let mut lockfile = Lockfile {
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
                        registry: String::new(),
                        digest: String::new(),
                    }],
                },
            ],
        };

        lockfile.resolve_dependency_details();

        let dep = &lockfile.interfaces[1].dependencies[0];
        assert_eq!(dep.registry, "ghcr.io/webassembly/wasi-logging");
        assert_eq!(dep.digest, "sha256:abc123");
    }
}
