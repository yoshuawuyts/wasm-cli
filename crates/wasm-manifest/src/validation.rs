//! Validation functions for manifest and lockfile consistency.

use crate::{Lockfile, Manifest};
use std::collections::HashSet;

/// Error type for validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum ValidationError {
    /// A package in the lockfile is not present in the manifest.
    MissingDependency {
        /// The name of the missing package.
        name: String,
    },
    /// A package dependency references a package that doesn't exist in the lockfile.
    InvalidDependency {
        /// The package that has the invalid dependency.
        package: String,
        /// The name of the dependency that doesn't exist.
        dependency: String,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::MissingDependency { name } => {
                write!(
                    f,
                    "Package '{name}' is in the lockfile but not in the manifest",
                )
            }
            ValidationError::InvalidDependency {
                package,
                dependency,
            } => {
                write!(
                    f,
                    "Package '{package}' depends on '{dependency}' which doesn't exist in the lockfile",
                )
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validates that a lockfile is consistent with its manifest.
///
/// This function checks that:
/// - All packages in the lockfile have corresponding entries in the manifest
/// - All package dependencies reference packages that exist in the lockfile
///
/// # Example
///
/// ```rust
/// use wasm_manifest::{Manifest, Lockfile, validate};
///
/// let manifest_toml = r#"
/// [interfaces]
/// "wasi:logging" = "ghcr.io/webassembly/wasi-logging:1.0.0"
/// "#;
///
/// let lockfile_toml = r#"
/// lockfile_version = 3
///
/// [[interfaces]]
/// name = "wasi:logging"
/// version = "1.0.0"
/// registry = "ghcr.io/webassembly/wasi-logging"
/// digest = "sha256:abc123"
/// "#;
///
/// let manifest: Manifest = toml::from_str(manifest_toml).unwrap();
/// let lockfile: Lockfile = toml::from_str(lockfile_toml).unwrap();
///
/// assert!(validate(&manifest, &lockfile).is_ok());
/// ```
///
/// # Errors
///
/// Returns a vector of `ValidationError` if validation fails. An empty vector
/// indicates successful validation.
pub fn validate(manifest: &Manifest, lockfile: &Lockfile) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    // Build a set of all dependency names from the manifest
    let manifest_deps: HashSet<&str> = manifest
        .all_dependencies()
        .map(|(name, _, _)| name.as_str())
        .collect();

    // Build a set of all package names from the lockfile for quick lookup
    let lockfile_packages: HashSet<&str> = lockfile
        .all_packages()
        .map(|(p, _)| p.name.as_str())
        .collect();

    // Check that all packages in the lockfile exist in the manifest
    for (package, _pkg_type) in lockfile.all_packages() {
        if !manifest_deps.contains(package.name.as_str()) {
            errors.push(ValidationError::MissingDependency {
                name: package.name.clone(),
            });
        }

        // Check that all dependencies of this package exist in the lockfile
        for dep in &package.dependencies {
            if !lockfile_packages.contains(dep.name.as_str()) {
                errors.push(ValidationError::InvalidDependency {
                    package: package.name.clone(),
                    dependency: dep.name.clone(),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dependency, Package, PackageDependency};
    use std::collections::HashMap;

    // r[verify validation.success]
    #[test]
    fn test_validate_success() {
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "wasi:logging".to_string(),
            Dependency::Compact("ghcr.io/webassembly/wasi-logging:1.0.0".to_string()),
        );
        interfaces.insert(
            "wasi:key-value".to_string(),
            Dependency::Compact("ghcr.io/webassembly/wasi-key-value:2.0.0".to_string()),
        );

        let manifest = Manifest {
            interfaces,
            ..Default::default()
        };

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

        assert!(validate(&manifest, &lockfile).is_ok());
    }

    // r[verify validation.missing-dependency]
    #[test]
    fn test_validate_missing_dependency() {
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "wasi:logging".to_string(),
            Dependency::Compact("ghcr.io/webassembly/wasi-logging:1.0.0".to_string()),
        );
        // Missing wasi:key-value in manifest

        let manifest = Manifest {
            interfaces,
            ..Default::default()
        };

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
                    dependencies: vec![],
                },
            ],
        };

        let result = validate(&manifest, &lockfile);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0],
            ValidationError::MissingDependency {
                name: "wasi:key-value".to_string()
            }
        );
    }

    // r[verify validation.invalid-dependency]
    #[test]
    fn test_validate_invalid_dependency() {
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "wasi:logging".to_string(),
            Dependency::Compact("ghcr.io/webassembly/wasi-logging:1.0.0".to_string()),
        );
        interfaces.insert(
            "wasi:key-value".to_string(),
            Dependency::Compact("ghcr.io/webassembly/wasi-key-value:2.0.0".to_string()),
        );

        let manifest = Manifest {
            interfaces,
            ..Default::default()
        };

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
                    dependencies: vec![
                        PackageDependency {
                            name: "wasi:logging".to_string(),
                            version: "1.0.0".to_string(),
                        },
                        PackageDependency {
                            name: "wasi:http".to_string(), // This package doesn't exist
                            version: "1.0.0".to_string(),
                        },
                    ],
                },
            ],
        };

        let result = validate(&manifest, &lockfile);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0],
            ValidationError::InvalidDependency {
                package: "wasi:key-value".to_string(),
                dependency: "wasi:http".to_string()
            }
        );
    }

    // r[verify validation.empty]
    #[test]
    fn test_validate_empty() {
        let manifest = Manifest::default();

        let lockfile = Lockfile {
            lockfile_version: 3,
            components: vec![],
            interfaces: vec![],
        };

        assert!(validate(&manifest, &lockfile).is_ok());
    }

    // r[verify validation.error-display]
    #[test]
    fn test_validation_error_display() {
        let err1 = ValidationError::MissingDependency {
            name: "wasi:logging".to_string(),
        };
        assert_eq!(
            err1.to_string(),
            "Package 'wasi:logging' is in the lockfile but not in the manifest"
        );

        let err2 = ValidationError::InvalidDependency {
            package: "wasi:key-value".to_string(),
            dependency: "wasi:http".to_string(),
        };
        assert_eq!(
            err2.to_string(),
            "Package 'wasi:key-value' depends on 'wasi:http' which doesn't exist in the lockfile"
        );
    }

    // r[verify validation.mixed-types]
    #[test]
    fn test_validate_components_and_interfaces() {
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
                name: "wasi:logging".to_string(),
                version: "1.0.0".to_string(),
                registry: "ghcr.io/webassembly/wasi-logging".to_string(),
                digest: "sha256:abc123".to_string(),
                dependencies: vec![],
            }],
        };

        assert!(validate(&manifest, &lockfile).is_ok());
    }
}
