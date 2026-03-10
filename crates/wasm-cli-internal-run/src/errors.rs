//! Error types for component validation and execution.

use miette::Diagnostic;

/// Error type for component validation and execution failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub enum RunError {
    /// The binary is a core WebAssembly module, not a component.
    #[diagnostic(
        code(wasm::run::core_module),
        help("use a tool like `wasm-tools component new` to wrap the module as a component")
    )]
    CoreModule,

    /// The binary could not be parsed as valid WebAssembly.
    #[diagnostic(
        code(wasm::run::invalid_binary),
        help("{reason}; ensure the file is a valid WebAssembly binary")
    )]
    InvalidBinary {
        /// The parser error message.
        reason: String,
    },

    /// The binary has no version header.
    #[diagnostic(
        code(wasm::run::no_version_header),
        help("ensure the file is a valid WebAssembly binary")
    )]
    NoVersionHeader,
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::CoreModule => {
                write!(
                    f,
                    "only Wasm Components can be executed; this appears to be a core module",
                )
            }
            RunError::InvalidBinary { reason } => {
                write!(f, "invalid Wasm binary: {reason}")
            }
            RunError::NoVersionHeader => {
                write!(f, "invalid Wasm binary: no version header found")
            }
        }
    }
}

impl std::error::Error for RunError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_variants_have_error_codes() {
        use miette::Diagnostic;

        let variants: Vec<Box<dyn Diagnostic>> = vec![
            Box::new(RunError::CoreModule),
            Box::new(RunError::InvalidBinary {
                reason: "test".to_string(),
            }),
            Box::new(RunError::NoVersionHeader),
        ];

        let expected_codes = [
            "wasm::run::core_module",
            "wasm::run::invalid_binary",
            "wasm::run::no_version_header",
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
