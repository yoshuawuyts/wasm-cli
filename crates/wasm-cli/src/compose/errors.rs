//! Error types for the `wasm compose` command.

use miette::Diagnostic;

/// Error type for `wasm compose` command failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub(crate) enum ComposeError {
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

        let no_wac = ComposeError::NoWacFiles;
        assert_eq!(
            no_wac
                .code()
                .expect("NoWacFiles must have a diagnostic code")
                .to_string(),
            "wasm::compose::no_wac_files",
        );
        assert!(
            no_wac.help().is_some(),
            "NoWacFiles must have a help message"
        );

        let invalid_name = ComposeError::InvalidName {
            name: "foo/bar".to_string(),
        };
        assert_eq!(
            invalid_name
                .code()
                .expect("InvalidName must have a diagnostic code")
                .to_string(),
            "wasm::compose::invalid_name",
        );
        assert!(
            invalid_name.help().is_some(),
            "InvalidName must have a help message"
        );

        let not_found = ComposeError::WacNotFound {
            name: "test".to_string(),
            hint: "no .wac files exist in `seams/`".to_string(),
        };
        assert_eq!(
            not_found
                .code()
                .expect("WacNotFound must have a diagnostic code")
                .to_string(),
            "wasm::compose::wac_not_found",
        );
        assert!(
            not_found.help().is_some(),
            "WacNotFound must have a help message"
        );
    }
}
