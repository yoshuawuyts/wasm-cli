//! Error types for the `wasm install` command.

use miette::Diagnostic;

/// Error type for `wasm install` command failures.
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

    /// The input could not be resolved as an OCI reference or manifest key.
    #[diagnostic(
        code(wasm::install::invalid_input),
        help(
            "'{input}' is not a recognized manifest key (e.g., wasi:logging) \
             or OCI reference (e.g., ghcr.io/owner/repo:tag)"
        )
    )]
    InvalidInput {
        /// The input string that could not be resolved.
        input: String,
    },

    /// A dependency string from the manifest could not be parsed as an OCI reference.
    #[diagnostic(
        code(wasm::install::invalid_reference),
        help("check the dependency value in wasm.toml: {reason}")
    )]
    InvalidReference {
        /// The reason the reference is invalid.
        reason: String,
    },
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::NoManifest => {
                write!(f, "no local `wasm.toml` manifest found")
            }
            InstallError::InvalidInput { input } => {
                write!(f, "'{input}' is not a valid OCI reference or manifest key",)
            }
            InstallError::InvalidReference { reason } => {
                write!(f, "invalid OCI reference in manifest: {reason}")
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

        let invalid_input = InstallError::InvalidInput {
            input: "not-a-ref".to_string(),
        };
        assert_eq!(
            invalid_input
                .code()
                .expect("InvalidInput must have a diagnostic code")
                .to_string(),
            "wasm::install::invalid_input",
        );
        assert!(
            invalid_input.help().is_some(),
            "InvalidInput must have a help message"
        );

        let invalid_ref = InstallError::InvalidReference {
            reason: "bad format".to_string(),
        };
        assert_eq!(
            invalid_ref
                .code()
                .expect("InvalidReference must have a diagnostic code")
                .to_string(),
            "wasm::install::invalid_reference",
        );
        assert!(
            invalid_ref.help().is_some(),
            "InvalidReference must have a help message"
        );
    }
}
