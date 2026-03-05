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
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::NoManifest => {
                write!(f, "no local `wasm.toml` manifest found")
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
    }
}
