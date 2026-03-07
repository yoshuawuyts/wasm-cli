//! Validation functions for manifest and lockfile consistency.

use crate::{Lockfile, Manifest};
use miette::Diagnostic;
use std::collections::HashSet;

/// Error type for validation failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure and maps back to a spec requirement.
///
/// # Example
///
/// ```rust
/// use miette::Diagnostic;
/// use wasm_manifest::ValidationError;
///
/// let err = ValidationError::MissingDependency {
///     name: "wasi:logging".to_string(),
/// };
/// assert_eq!(
///     err.to_string(),
///     "Package 'wasi:logging' is in the lockfile but not in the manifest"
/// );
/// assert_eq!(
///     err.code().expect("should have a code").to_string(),
///     "wasm::validation::missing_dependency",
/// );
///
/// let err = ValidationError::InvalidDependency {
///     package: "wasi:key-value".to_string(),
///     dependency: "wasi:http".to_string(),
/// };
/// assert!(err.to_string().contains("wasi:http"));
/// assert_eq!(
///     err.code().expect("should have a code").to_string(),
///     "wasm::validation::invalid_dependency",
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub enum ValidationError {
    /// A package in the lockfile is not present in the manifest.
    ///
    /// See spec: `r[validation.missing-dependency]`
    #[diagnostic(
        code(wasm::validation::missing_dependency),
        help("remove '{name}' from the lockfile or add it to the manifest")
    )]
    MissingDependency {
        /// The name of the missing package.
        name: String,
    },
    /// A package dependency references a package that doesn't exist in the lockfile.
    ///
    /// See spec: `r[validation.invalid-dependency]`
    #[diagnostic(
        code(wasm::validation::invalid_dependency),
        help("add '{dependency}' to the lockfile or remove the reference from '{package}'")
    )]
    InvalidDependency {
        /// The package that has the invalid dependency.
        package: String,
        /// The name of the dependency that doesn't exist.
        dependency: String,
    },
    /// A version constraint in the manifest is not a valid semver requirement.
    ///
    /// See spec: `r[validation.invalid-version-constraint]`
    #[diagnostic(
        code(wasm::validation::invalid_version_constraint),
        help(
            "'{name}' has version '{version}': {reason}. Use a valid semver constraint such as '1.0.0', '>=1.0, <2.0', '~1.2', '=1.2.3', or '*'"
        )
    )]
    InvalidVersionConstraint {
        /// The dependency name with the invalid constraint.
        name: String,
        /// The invalid version string.
        version: String,
        /// The parse error message.
        reason: String,
    },
    /// Two dependencies across sections have incompatible version constraints
    /// for the same package name.
    ///
    /// See spec: `r[validation.version-conflict]`
    #[diagnostic(
        code(wasm::validation::version_conflict),
        help(
            "'{name}' has '{version_a}' in one section and '{version_b}' in another; align versions or use compatible ranges"
        )
    )]
    VersionConflict {
        /// The package name with conflicting constraints.
        name: String,
        /// The first version constraint.
        version_a: String,
        /// The second version constraint.
        version_b: String,
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
            ValidationError::InvalidVersionConstraint {
                name,
                version,
                reason,
            } => {
                write!(
                    f,
                    "Dependency '{name}' has invalid version constraint '{version}': {reason}",
                )
            }
            ValidationError::VersionConflict {
                name,
                version_a,
                version_b,
            } => {
                write!(
                    f,
                    "Dependency '{name}' has conflicting version constraints: '{version_a}' vs '{version_b}'",
                )
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validates that a lockfile is consistent with its manifest.
///
/// This function checks that:
/// - All version strings in the manifest are valid semver requirements
/// - There are no conflicting version constraints for the same package across sections
/// - All packages in the lockfile have corresponding entries in the manifest
/// - All package dependencies reference packages that exist in the lockfile
///
/// # Example
///
/// ```rust
/// use wasm_manifest::{Manifest, Lockfile, validate};
///
/// let manifest_toml = r#"
/// [dependencies.interfaces]
/// "wasi:logging" = "1.0.0"
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

    // r[impl validation.invalid-version-constraint]
    // Validate all version strings parse as valid semver requirements
    validate_version_constraints(manifest, &mut errors);

    // r[impl validation.version-conflict]
    // Check for conflicting version constraints across sections
    validate_version_conflicts(manifest, &mut errors);

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

/// Validate that all version strings in the manifest parse as valid semver
/// requirements.
fn validate_version_constraints(manifest: &Manifest, errors: &mut Vec<ValidationError>) {
    for (name, dep, _) in manifest.all_dependencies() {
        if let Err(e) = dep.parse_version_req() {
            errors.push(ValidationError::InvalidVersionConstraint {
                name: name.clone(),
                version: dep.version().to_string(),
                reason: e.to_string(),
            });
        }
    }
}

/// Detect conflicting version constraints for the same package name across
/// the components and interfaces sections.
///
/// Two constraints conflict when they parse to different semver requirements.
/// Strings that normalize to the same `VersionReq` (e.g. `"1"` and `"1.0.0"`)
/// are considered compatible.
fn validate_version_conflicts(manifest: &Manifest, errors: &mut Vec<ValidationError>) {
    for (name, comp_dep) in &manifest.dependencies.components {
        if let Some(iface_dep) = manifest.dependencies.interfaces.get(name) {
            let comp_ver = comp_dep.version();
            let iface_ver = iface_dep.version();

            // Try parsing both; if either fails, the invalid-constraint
            // check above will catch it — skip the conflict check.
            let (Ok(comp_req), Ok(iface_req)) =
                (comp_dep.parse_version_req(), iface_dep.parse_version_req())
            else {
                continue;
            };

            // Compare the parsed requirements — this handles cases like
            // "1.0.0" vs "^1.0.0" which are semantically identical.
            if comp_req != iface_req {
                errors.push(ValidationError::VersionConflict {
                    name: name.clone(),
                    version_a: comp_ver.to_string(),
                    version_b: iface_ver.to_string(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dependencies, Dependency, Package, PackageDependency};
    use std::collections::HashMap;

    // r[verify validation.success]
    #[test]
    fn test_validate_success() {
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "wasi:logging".to_string(),
            Dependency::Compact("1.0.0".to_string()),
        );
        interfaces.insert(
            "wasi:key-value".to_string(),
            Dependency::Compact("2.0.0".to_string()),
        );

        let manifest = Manifest {
            dependencies: Dependencies {
                interfaces,
                ..Default::default()
            },
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
                        registry: "ghcr.io/webassembly/wasi-logging".to_string(),
                        digest: "sha256:abc123".to_string(),
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
            Dependency::Compact("1.0.0".to_string()),
        );
        // Missing wasi:key-value in manifest

        let manifest = Manifest {
            dependencies: Dependencies {
                interfaces,
                ..Default::default()
            },
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
            Dependency::Compact("1.0.0".to_string()),
        );
        interfaces.insert(
            "wasi:key-value".to_string(),
            Dependency::Compact("2.0.0".to_string()),
        );

        let manifest = Manifest {
            dependencies: Dependencies {
                interfaces,
                ..Default::default()
            },
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
                            registry: "ghcr.io/webassembly/wasi-logging".to_string(),
                            digest: "sha256:abc123".to_string(),
                        },
                        PackageDependency {
                            name: "wasi:http".to_string(), // This package doesn't exist
                            version: "1.0.0".to_string(),
                            registry: "ghcr.io/webassembly/wasi-http".to_string(),
                            digest: "sha256:missing".to_string(),
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

        let err3 = ValidationError::InvalidVersionConstraint {
            name: "wasi:logging".to_string(),
            version: "not-valid".to_string(),
            reason: "unexpected character".to_string(),
        };
        assert!(err3.to_string().contains("not-valid"));

        let err4 = ValidationError::VersionConflict {
            name: "wasi:logging".to_string(),
            version_a: "1.0.0".to_string(),
            version_b: "2.0.0".to_string(),
        };
        assert!(err4.to_string().contains("conflicting"));
    }

    // r[verify validation.mixed-types]
    #[test]
    fn test_validate_components_and_interfaces() {
        let mut components = HashMap::new();
        components.insert(
            "root:component".to_string(),
            Dependency::Compact("0.1.0".to_string()),
        );
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "wasi:logging".to_string(),
            Dependency::Compact("1.0.0".to_string()),
        );

        let manifest = Manifest {
            dependencies: Dependencies {
                components,
                interfaces,
            },
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

    // r[verify validation.error-codes]
    #[test]
    fn test_all_variants_have_error_codes() {
        use miette::Diagnostic;

        let missing = ValidationError::MissingDependency {
            name: "test".to_string(),
        };
        assert_eq!(
            missing
                .code()
                .expect("MissingDependency must have a diagnostic code")
                .to_string(),
            "wasm::validation::missing_dependency",
        );
        assert!(
            missing.help().is_some(),
            "MissingDependency must have a help message"
        );

        let invalid = ValidationError::InvalidDependency {
            package: "test".to_string(),
            dependency: "dep".to_string(),
        };
        assert_eq!(
            invalid
                .code()
                .expect("InvalidDependency must have a diagnostic code")
                .to_string(),
            "wasm::validation::invalid_dependency",
        );
        assert!(
            invalid.help().is_some(),
            "InvalidDependency must have a help message"
        );

        let invalid_version = ValidationError::InvalidVersionConstraint {
            name: "test".to_string(),
            version: "bad".to_string(),
            reason: "parse error".to_string(),
        };
        assert_eq!(
            invalid_version
                .code()
                .expect("InvalidVersionConstraint must have a diagnostic code")
                .to_string(),
            "wasm::validation::invalid_version_constraint",
        );
        assert!(
            invalid_version.help().is_some(),
            "InvalidVersionConstraint must have a help message"
        );

        let conflict = ValidationError::VersionConflict {
            name: "test".to_string(),
            version_a: "1.0.0".to_string(),
            version_b: "2.0.0".to_string(),
        };
        assert_eq!(
            conflict
                .code()
                .expect("VersionConflict must have a diagnostic code")
                .to_string(),
            "wasm::validation::version_conflict",
        );
        assert!(
            conflict.help().is_some(),
            "VersionConflict must have a help message"
        );
    }

    // r[verify validation.invalid-version-constraint]
    #[test]
    fn test_validate_invalid_version_constraint() {
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "wasi:logging".to_string(),
            Dependency::Compact("not-a-version".to_string()),
        );

        let manifest = Manifest {
            dependencies: Dependencies {
                interfaces,
                ..Default::default()
            },
        };

        let lockfile = Lockfile {
            lockfile_version: 3,
            components: vec![],
            interfaces: vec![],
        };

        let result = validate(&manifest, &lockfile);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::InvalidVersionConstraint { name, .. } if name == "wasi:logging"
        )));
    }

    // r[verify validation.version-conflict]
    #[test]
    fn test_validate_version_conflict() {
        let mut components = HashMap::new();
        components.insert(
            "test:pkg".to_string(),
            Dependency::Compact("1.0.0".to_string()),
        );
        let mut interfaces = HashMap::new();
        interfaces.insert(
            "test:pkg".to_string(),
            Dependency::Compact("2.0.0".to_string()),
        );

        let manifest = Manifest {
            dependencies: Dependencies {
                components,
                interfaces,
            },
        };

        let lockfile = Lockfile {
            lockfile_version: 3,
            components: vec![],
            interfaces: vec![],
        };

        let result = validate(&manifest, &lockfile);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::VersionConflict { name, .. } if name == "test:pkg"
        )));
    }
}
