//! Error types for the `wasm install` CLI command.

use miette::Diagnostic;

/// CLI-specific error type for `wasm install` command failures.
///
/// Core install errors (e.g. invalid references, unknown packages) live in
/// [`wasm_package_manager::manager::install::InstallError`].
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub(crate) enum InstallError {
    /// No `wasm.toml` manifest was found in the project.
    #[diagnostic(
        code(wasm::install::no_manifest),
        help(
            "call `wasm init` to create a `wasm.toml` manifest locally\n\
             call `wasm registry fetch <component>` to fetch the package \
             without affecting the local manifest"
        )
    )]
    NoManifest,

    /// Dependency resolution failed: no compatible set of versions exists.
    #[diagnostic(
        code(wasm::install::dependency_conflict),
        help(
            "Run `wasm registry fetch` to update the registry index.\n\
             If the conflict persists, check for incompatible dependency\n\
             version constraints in the packages you are installing."
        )
    )]
    DependencyConflict(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::NoManifest => {
                write!(f, "no local `wasm.toml` manifest found")
            }
            InstallError::DependencyConflict(reason) => {
                write!(f, "dependency conflict: {reason}")
            }
        }
    }
}

impl std::error::Error for InstallError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_variants_have_error_codes() {
        use miette::Diagnostic;

        let no_manifest = InstallError::NoManifest;
        assert_eq!(
            no_manifest
                .code()
                .expect("NoManifest must have a diagnostic code")
                .to_string(),
            "wasm::install::no_manifest",
        );
        assert!(
            no_manifest.help().is_some(),
            "NoManifest must have a help message"
        );

        let dep_conflict = InstallError::DependencyConflict("no solution".to_string());
        assert_eq!(
            dep_conflict
                .code()
                .expect("DependencyConflict must have a diagnostic code")
                .to_string(),
            "wasm::install::dependency_conflict",
        );
        assert!(
            dep_conflict.help().is_some(),
            "DependencyConflict must have a help message"
        );
    }
}
