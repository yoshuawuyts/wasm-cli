//! Error types for install operations.

use miette::Diagnostic;

/// Error type for install operation failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub enum InstallError {
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

    /// A WIT-style package name could not be resolved via the known-package index.
    #[diagnostic(
        code(wasm::install::unknown_package),
        help(
            "'{input}' looks like a WIT package name but was not found in the \n\
             registry index. Try running `wasm registry fetch` first to update \n\
             the index, or use a full OCI reference instead \n\
             (e.g. ghcr.io/webassembly/wasi/http:latest)"
        )
    )]
    UnknownPackage {
        /// The input string that could not be resolved.
        input: String,
    },

    /// A manifest dependency could not be resolved.
    #[diagnostic(
        code(wasm::install::resolve_failure),
        help("failed to resolve dependency: {reason}")
    )]
    ResolveFailure {
        /// The underlying reason for the failure.
        reason: String,
    },
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::InvalidInput { input } => {
                write!(f, "'{input}' is not a valid OCI reference or manifest key",)
            }
            InstallError::InvalidReference { reason } => {
                write!(f, "invalid OCI reference in manifest: {reason}")
            }
            InstallError::UnknownPackage { input } => {
                write!(f, "package '{input}' not found in the registry index")
            }
            InstallError::ResolveFailure { reason } => {
                write!(f, "failed to resolve dependency: {reason}")
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

        let variants: Vec<Box<dyn Diagnostic>> = vec![
            Box::new(InstallError::InvalidInput {
                input: "not-a-ref".to_string(),
            }),
            Box::new(InstallError::InvalidReference {
                reason: "bad format".to_string(),
            }),
            Box::new(InstallError::UnknownPackage {
                input: "wasi:http".to_string(),
            }),
            Box::new(InstallError::ResolveFailure {
                reason: "not found".to_string(),
            }),
        ];

        let expected_codes = [
            "wasm::install::invalid_input",
            "wasm::install::invalid_reference",
            "wasm::install::unknown_package",
            "wasm::install::resolve_failure",
        ];

        for (variant, expected_code) in variants.iter().zip(expected_codes.iter()) {
            assert_eq!(
                variant
                    .code()
                    .unwrap_or_else(|| panic!("{expected_code} must have a diagnostic code"))
                    .to_string(),
                *expected_code,
            );
            assert!(
                variant.help().is_some(),
                "{expected_code} must have a help message"
            );
        }
    }
}
