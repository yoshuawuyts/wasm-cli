//! Error types for component composition.

use miette::Diagnostic;

/// Error type for component composition failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub enum ComposeError {
    /// No `.wac` files were found in the `seams/` directory.
    #[diagnostic(
        code(wasm::compose::no_wac_files),
        help("add `.wac` files to the `seams/` directory")
    )]
    NoWacFiles,

    /// The composition name contains path separators or traversal sequences.
    #[diagnostic(
        code(wasm::compose::invalid_name),
        help("'{name}' contains path separators; use a plain name like 'foo'")
    )]
    InvalidName {
        /// The invalid composition name.
        name: String,
    },

    /// The requested `.wac` file was not found in `seams/`.
    #[diagnostic(
        code(wasm::compose::wac_not_found),
        help("'seams/{name}.wac' not found; {hint}")
    )]
    WacNotFound {
        /// The name that was looked up.
        name: String,
        /// A contextual hint (e.g. listing available files).
        hint: String,
    },

    /// A `.wac` file could not be parsed.
    #[diagnostic(
        code(wasm::compose::parse_failed),
        help("check the WAC syntax in '{file}': {reason}")
    )]
    ParseFailed {
        /// The path to the WAC file.
        file: String,
        /// The underlying parse error.
        reason: String,
    },

    /// Could not determine the set of packages referenced by a `.wac` file.
    #[diagnostic(
        code(wasm::compose::package_discovery_failed),
        help("check the import declarations in '{file}': {reason}")
    )]
    PackageDiscoveryFailed {
        /// The path to the WAC file.
        file: String,
        /// The underlying error message.
        reason: String,
    },

    /// Could not resolve the packages required by a `.wac` file.
    #[diagnostic(
        code(wasm::compose::package_resolution_failed),
        help("ensure all dependencies for '{file}' are installed via `wasm install`: {reason}")
    )]
    PackageResolutionFailed {
        /// The path to the WAC file.
        file: String,
        /// The underlying resolution error.
        reason: String,
    },

    /// The WAC document resolution step failed.
    #[diagnostic(
        code(wasm::compose::resolution_failed),
        help("check the component wiring in '{file}': {reason}")
    )]
    ResolutionFailed {
        /// The path to the WAC file.
        file: String,
        /// The underlying resolution error.
        reason: String,
    },

    /// Encoding the composed component failed.
    #[diagnostic(
        code(wasm::compose::encode_failed),
        help("encoding of '{file}' failed: {reason}")
    )]
    EncodeFailed {
        /// The path to the WAC file.
        file: String,
        /// The underlying encode error.
        reason: String,
    },
}

impl std::fmt::Display for ComposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComposeError::NoWacFiles => {
                write!(f, "no .wac files found; add files to `seams/`")
            }
            ComposeError::InvalidName { name } => {
                write!(
                    f,
                    "invalid composition name '{name}': must be a plain name, not a path",
                )
            }
            ComposeError::WacNotFound { name, .. } => {
                write!(f, "WAC file 'seams/{name}.wac' not found")
            }
            ComposeError::ParseFailed { file, .. } => {
                write!(f, "parse error in '{file}'")
            }
            ComposeError::PackageDiscoveryFailed { file, .. } => {
                write!(f, "could not determine packages in '{file}'")
            }
            ComposeError::PackageResolutionFailed { file, .. } => {
                write!(f, "could not resolve packages for '{file}'")
            }
            ComposeError::ResolutionFailed { file, .. } => {
                write!(f, "resolution error in '{file}'")
            }
            ComposeError::EncodeFailed { file, .. } => {
                write!(f, "encode error for '{file}'")
            }
        }
    }
}

impl std::error::Error for ComposeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_variants_have_error_codes() {
        use miette::Diagnostic;

        let variants: Vec<Box<dyn Diagnostic>> = vec![
            Box::new(ComposeError::NoWacFiles),
            Box::new(ComposeError::InvalidName {
                name: "foo/bar".to_string(),
            }),
            Box::new(ComposeError::WacNotFound {
                name: "test".to_string(),
                hint: "no .wac files exist in `seams/`".to_string(),
            }),
            Box::new(ComposeError::ParseFailed {
                file: "seams/test.wac".to_string(),
                reason: "unexpected token".to_string(),
            }),
            Box::new(ComposeError::PackageDiscoveryFailed {
                file: "seams/test.wac".to_string(),
                reason: "unknown import".to_string(),
            }),
            Box::new(ComposeError::PackageResolutionFailed {
                file: "seams/test.wac".to_string(),
                reason: "missing dep".to_string(),
            }),
            Box::new(ComposeError::ResolutionFailed {
                file: "seams/test.wac".to_string(),
                reason: "type mismatch".to_string(),
            }),
            Box::new(ComposeError::EncodeFailed {
                file: "seams/test.wac".to_string(),
                reason: "invalid graph".to_string(),
            }),
        ];

        let expected_codes = [
            "wasm::compose::no_wac_files",
            "wasm::compose::invalid_name",
            "wasm::compose::wac_not_found",
            "wasm::compose::parse_failed",
            "wasm::compose::package_discovery_failed",
            "wasm::compose::package_resolution_failed",
            "wasm::compose::resolution_failed",
            "wasm::compose::encode_failed",
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
