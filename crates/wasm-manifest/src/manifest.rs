//! Types for the WASM manifest file (`wasm.toml`).

use crate::permissions::RunPermissions;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The type of a WASM package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[must_use]
pub enum PackageType {
    /// A compiled WebAssembly component.
    Component,
    /// A WIT interface definition.
    Interface,
}

/// The root manifest structure for a WASM package.
///
/// The manifest file (`deps/wasm.toml`) defines dependencies for a WASM package,
/// separated into components and interfaces.
///
/// # Example
///
/// ```toml
/// [components]
/// "root:component" = "ghcr.io/bytecodealliance/sample-wasi-http-rust/sample-wasi-http-rust:0.1.6"
///
/// [interfaces]
/// "wasi:clocks" = "ghcr.io/webassembly/wasi/clocks:0.2.5"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[must_use]
pub struct Manifest {
    /// The components section of the manifest.
    #[serde(default)]
    pub components: HashMap<String, Dependency>,
    /// The interfaces section of the manifest.
    #[serde(default)]
    pub interfaces: HashMap<String, Dependency>,
}

impl Manifest {
    /// Iterate over all dependencies with their package type.
    pub fn all_dependencies(&self) -> impl Iterator<Item = (&String, &Dependency, PackageType)> {
        self.components
            .iter()
            .map(|(k, v)| (k, v, PackageType::Component))
            .chain(
                self.interfaces
                    .iter()
                    .map(|(k, v)| (k, v, PackageType::Interface)),
            )
    }
}

/// A dependency specification in the manifest.
///
/// Dependencies can be specified in two formats:
///
/// 1. Compact format (string):
///    ```toml
///    [dependencies]
///    "wasi:logging" = "ghcr.io/webassembly/wasi-logging:1.0.0"
///    ```
///
/// 2. Explicit format (table):
///    ```toml
///    [dependencies."wasi:logging"]
///    registry = "ghcr.io"
///    namespace = "webassembly"
///    package = "wasi-logging"
///    version = "1.0.0"
///    ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[must_use]
pub enum Dependency {
    /// Compact format: a single string with full registry path and version.
    ///
    /// Format: `registry/namespace/package:version`
    ///
    /// # Example
    /// ```text
    /// "ghcr.io/webassembly/wasi-logging:1.0.0"
    /// ```
    Compact(String),

    /// Explicit format: a table with individual fields.
    Explicit {
        /// The registry host (e.g., "ghcr.io").
        registry: String,
        /// The namespace or organization (e.g., "webassembly").
        namespace: String,
        /// The package name (e.g., "wasi-logging").
        package: String,
        /// The package version (e.g., "1.0.0").
        version: String,
        /// Optional sandbox permissions for running this component.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        permissions: Option<RunPermissions>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_compact_format() {
        let toml = r#"
            [interfaces]
            "wasi:logging" = "ghcr.io/webassembly/wasi-logging:1.0.0"
            "wasi:key-value" = "ghcr.io/webassembly/wasi-key-value:2.0.0"
        "#;

        let manifest: Manifest = toml::from_str(toml).expect("Failed to parse manifest");

        assert_eq!(manifest.interfaces.len(), 2);
        assert!(manifest.interfaces.contains_key("wasi:logging"));
        assert!(manifest.interfaces.contains_key("wasi:key-value"));

        match &manifest.interfaces["wasi:logging"] {
            Dependency::Compact(s) => {
                assert_eq!(s, "ghcr.io/webassembly/wasi-logging:1.0.0");
            }
            _ => panic!("Expected compact format"),
        }
    }

    #[test]
    fn test_parse_explicit_format() {
        let toml = r#"
            [interfaces."wasi:logging"]
            registry = "ghcr.io"
            namespace = "webassembly"
            package = "wasi-logging"
            version = "1.0.0"

            [interfaces."wasi:key-value"]
            registry = "ghcr.io"
            namespace = "webassembly"
            package = "wasi-key-value"
            version = "2.0.0"
        "#;

        let manifest: Manifest = toml::from_str(toml).expect("Failed to parse manifest");

        assert_eq!(manifest.interfaces.len(), 2);

        match &manifest.interfaces["wasi:logging"] {
            Dependency::Explicit {
                registry,
                namespace,
                package,
                version,
                ..
            } => {
                assert_eq!(registry, "ghcr.io");
                assert_eq!(namespace, "webassembly");
                assert_eq!(package, "wasi-logging");
                assert_eq!(version, "1.0.0");
            }
            _ => panic!("Expected explicit format"),
        }
    }

    #[test]
    fn test_serialize_compact_format() {
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "wasi:logging".to_string(),
            Dependency::Compact("ghcr.io/webassembly/wasi-logging:1.0.0".to_string()),
        );

        let manifest = Manifest {
            interfaces,
            ..Default::default()
        };
        let toml = toml::to_string(&manifest).expect("Failed to serialize manifest");

        assert!(toml.contains("wasi:logging"));
        assert!(toml.contains("ghcr.io/webassembly/wasi-logging:1.0.0"));
    }

    #[test]
    fn test_serialize_explicit_format() {
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "wasi:logging".to_string(),
            Dependency::Explicit {
                registry: "ghcr.io".to_string(),
                namespace: "webassembly".to_string(),
                package: "wasi-logging".to_string(),
                version: "1.0.0".to_string(),
                permissions: None,
            },
        );

        let manifest = Manifest {
            interfaces,
            ..Default::default()
        };
        let toml = toml::to_string(&manifest).expect("Failed to serialize manifest");

        assert!(toml.contains("wasi:logging"));
        assert!(toml.contains("registry"));
        assert!(toml.contains("ghcr.io"));
    }

    #[test]
    fn test_empty_manifest() {
        let toml = r#""#;
        let manifest: Manifest = toml::from_str(toml).expect("Failed to parse empty manifest");
        assert_eq!(manifest.components.len(), 0);
        assert_eq!(manifest.interfaces.len(), 0);
    }

    #[test]
    fn test_parse_components_and_interfaces() {
        let toml = r#"
            [components]
            "root:component" = "ghcr.io/example/component:0.1.0"

            [interfaces]
            "wasi:clocks" = "ghcr.io/webassembly/wasi/clocks:0.2.5"
        "#;

        let manifest: Manifest = toml::from_str(toml).expect("Failed to parse manifest");

        assert_eq!(manifest.components.len(), 1);
        assert_eq!(manifest.interfaces.len(), 1);
        assert!(manifest.components.contains_key("root:component"));
        assert!(manifest.interfaces.contains_key("wasi:clocks"));
    }

    #[test]
    fn test_all_dependencies() {
        let mut components = HashMap::new();
        components.insert(
            "root:component".to_string(),
            Dependency::Compact("ghcr.io/example/component:0.1.0".to_string()),
        );
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "wasi:logging".to_string(),
            Dependency::Compact("ghcr.io/webassembly/wasi-logging:1.0.0".to_string()),
        );

        let manifest = Manifest {
            components,
            interfaces,
        };

        let all: Vec<_> = manifest.all_dependencies().collect();
        assert_eq!(all.len(), 2);

        let has_component = all.iter().any(|(_, _, pt)| *pt == PackageType::Component);
        let has_interface = all.iter().any(|(_, _, pt)| *pt == PackageType::Interface);
        assert!(has_component);
        assert!(has_interface);
    }

    #[test]
    fn test_parse_explicit_with_permissions() {
        let toml = r#"
            [components."root:component"]
            registry = "ghcr.io"
            namespace = "yoshuawuyts"
            package = "fetch"
            version = "latest"
            permissions.inherit-env = true
            permissions.allow-dirs = ["/data", "./output"]
        "#;

        let manifest: Manifest = toml::from_str(toml).expect("Failed to parse manifest");

        match &manifest.components["root:component"] {
            Dependency::Explicit {
                registry,
                permissions,
                ..
            } => {
                assert_eq!(registry, "ghcr.io");
                let perms = permissions.as_ref().expect("Expected permissions");
                assert_eq!(perms.inherit_env, Some(true));
                assert_eq!(
                    perms.allow_dirs,
                    Some(vec![
                        std::path::PathBuf::from("/data"),
                        std::path::PathBuf::from("./output"),
                    ])
                );
            }
            _ => panic!("Expected explicit format"),
        }
    }

    #[test]
    fn test_explicit_without_permissions_still_works() {
        let toml = r#"
            [components."root:component"]
            registry = "ghcr.io"
            namespace = "yoshuawuyts"
            package = "fetch"
            version = "latest"
        "#;

        let manifest: Manifest = toml::from_str(toml).expect("Failed to parse manifest");

        match &manifest.components["root:component"] {
            Dependency::Explicit { permissions, .. } => {
                assert!(permissions.is_none());
            }
            _ => panic!("Expected explicit format"),
        }
    }
}
