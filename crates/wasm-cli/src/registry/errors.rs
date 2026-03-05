//! Error types for `wasm registry` subcommands.

use miette::Diagnostic;

/// Error type for `wasm registry inspect` command failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub(crate) enum InspectError {
    /// The pulled OCI image has no manifest.
    #[diagnostic(
        code(wasm::registry::inspect_no_manifest),
        help("verify the reference '{reference}' points to a valid OCI image")
    )]
    NoManifest {
        /// The OCI reference that was inspected.
        reference: String,
    },

    /// The OCI manifest contains no `application/wasm` layer.
    #[diagnostic(
        code(wasm::registry::inspect_no_wasm_layer),
        help("'{reference}' does not contain an `application/wasm` layer")
    )]
    NoWasmLayer {
        /// The OCI reference that was inspected.
        reference: String,
    },
}

impl std::fmt::Display for InspectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InspectError::NoManifest { reference } => {
                write!(f, "no manifest found for '{reference}'")
            }
            InspectError::NoWasmLayer { reference } => {
                write!(f, "no wasm layers found for '{reference}'")
            }
        }
    }
}

impl std::error::Error for InspectError {}

/// Error type for `wasm registry sync` command failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub(crate) enum SyncError {
    /// The sync operation returned a degraded result.
    #[diagnostic(
        code(wasm::registry::sync_failed),
        help("check your network connection and try again: {reason}")
    )]
    Degraded {
        /// The error message from the sync.
        reason: String,
    },
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Degraded { reason } => {
                write!(f, "sync failed: {reason}")
            }
        }
    }
}

impl std::error::Error for SyncError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_variants_have_error_codes() {
        use miette::Diagnostic;

        let no_manifest = InspectError::NoManifest {
            reference: "ghcr.io/example/comp:1.0".to_string(),
        };
        assert_eq!(
            no_manifest
                .code()
                .expect("NoManifest must have a diagnostic code")
                .to_string(),
            "wasm::registry::inspect_no_manifest",
        );
        assert!(
            no_manifest.help().is_some(),
            "NoManifest must have a help message"
        );

        let no_layer = InspectError::NoWasmLayer {
            reference: "ghcr.io/example/comp:1.0".to_string(),
        };
        assert_eq!(
            no_layer
                .code()
                .expect("NoWasmLayer must have a diagnostic code")
                .to_string(),
            "wasm::registry::inspect_no_wasm_layer",
        );
        assert!(
            no_layer.help().is_some(),
            "NoWasmLayer must have a help message"
        );

        let degraded = SyncError::Degraded {
            reason: "connection refused".to_string(),
        };
        assert_eq!(
            degraded
                .code()
                .expect("Degraded must have a diagnostic code")
                .to_string(),
            "wasm::registry::sync_failed",
        );
        assert!(
            degraded.help().is_some(),
            "Degraded must have a help message"
        );
    }
}
