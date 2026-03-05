//! Error types for the `wasm run` command.

use miette::Diagnostic;

/// Error type for `wasm run` command failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub(crate) enum RunError {
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

    /// The pulled OCI image has no manifest.
    #[diagnostic(
        code(wasm::run::no_manifest),
        help("ensure the OCI reference points to a valid Wasm package")
    )]
    NoManifest,

    /// The OCI manifest contains no `application/wasm` layer.
    #[diagnostic(
        code(wasm::run::no_wasm_layer),
        help("ensure the image contains an `application/wasm` layer")
    )]
    NoWasmLayer,

    /// A manifest component key is not present in the lockfile.
    #[diagnostic(
        code(wasm::run::not_in_lockfile),
        help("run `wasm install {name}` to populate the lockfile")
    )]
    NotInLockfile {
        /// The component key that was looked up.
        name: String,
    },

    /// The lockfile `registry` field is not in the expected `host/repository` format.
    #[diagnostic(
        code(wasm::run::invalid_registry_path),
        help("registry path '{path}' for '{name}' should have format 'host/repository'")
    )]
    InvalidRegistryPath {
        /// The registry path that was found.
        path: String,
        /// The component key.
        name: String,
    },

    /// The vendored file for a manifest component does not exist on disk.
    #[diagnostic(
        code(wasm::run::vendored_file_missing),
        help("'{path}' not found; run `wasm install {name}` to vendor the component")
    )]
    VendoredFileMissing {
        /// The expected file path.
        path: String,
        /// The component key.
        name: String,
    },
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
            RunError::NoManifest => {
                write!(f, "pulled image has no manifest")
            }
            RunError::NoWasmLayer => {
                write!(f, "manifest contains no application/wasm layer")
            }
            RunError::NotInLockfile { name } => {
                write!(
                    f,
                    "component '{name}' is in the manifest but not in the lockfile",
                )
            }
            RunError::InvalidRegistryPath { path, name } => {
                write!(f, "invalid registry path '{path}' in lockfile for '{name}'",)
            }
            RunError::VendoredFileMissing { path, name } => {
                write!(f, "vendored file '{path}' not found for component '{name}'",)
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
            Box::new(RunError::NoManifest),
            Box::new(RunError::NoWasmLayer),
            Box::new(RunError::NotInLockfile {
                name: "test".to_string(),
            }),
            Box::new(RunError::InvalidRegistryPath {
                path: "test".to_string(),
                name: "test".to_string(),
            }),
            Box::new(RunError::VendoredFileMissing {
                path: "test".to_string(),
                name: "test".to_string(),
            }),
        ];

        let expected_codes = [
            "wasm::run::core_module",
            "wasm::run::invalid_binary",
            "wasm::run::no_version_header",
            "wasm::run::no_manifest",
            "wasm::run::no_wasm_layer",
            "wasm::run::not_in_lockfile",
            "wasm::run::invalid_registry_path",
            "wasm::run::vendored_file_missing",
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
