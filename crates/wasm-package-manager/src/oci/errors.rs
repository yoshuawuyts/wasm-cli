//! Error types for OCI layer validation.

use miette::Diagnostic;

/// Error type for OCI layer validation failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
///
/// # Example
///
/// ```rust
/// use miette::Diagnostic;
/// use wasm_package_manager::oci::OciLayerError;
///
/// let err = OciLayerError::InvalidLayerCount { found: 3 };
/// assert_eq!(
///     err.code().expect("should have a code").to_string(),
///     "wasm::oci::invalid_layer_count",
/// );
///
/// let err = OciLayerError::InvalidMediaType {
///     found: "application/octet-stream".to_string(),
/// };
/// assert_eq!(
///     err.code().expect("should have a code").to_string(),
///     "wasm::oci::invalid_media_type",
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub enum OciLayerError {
    /// The OCI bundle does not contain exactly one layer.
    ///
    /// See spec: `r[oci.layers.reject-multi]`
    #[diagnostic(
        code(wasm::oci::invalid_layer_count),
        help(
            "expected exactly 1 layer but found {found}; see \
             https://tag-runtime.cncf.io/wgs/wasm/deliverables/wasm-oci-artifact/#faq"
        )
    )]
    InvalidLayerCount {
        /// The number of layers found in the bundle.
        found: usize,
    },
    /// The single layer does not have the expected `application/wasm` media type.
    ///
    /// See spec: `r[oci.layers.require-wasm-content-type]`
    #[diagnostic(
        code(wasm::oci::invalid_media_type),
        help("found media type '{found}'; the layer must have media type 'application/wasm'")
    )]
    InvalidMediaType {
        /// The media type that was found.
        found: String,
    },
}

impl std::fmt::Display for OciLayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OciLayerError::InvalidLayerCount { found } => {
                write!(f, "expected exactly 1 layer in OCI bundle, found {found}",)
            }
            OciLayerError::InvalidMediaType { found } => {
                write!(
                    f,
                    "expected layer media type `application/wasm`, found `{found}`",
                )
            }
        }
    }
}

impl std::error::Error for OciLayerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_variants_have_error_codes() {
        use miette::Diagnostic;

        let invalid_count = OciLayerError::InvalidLayerCount { found: 3 };
        assert_eq!(
            invalid_count
                .code()
                .expect("InvalidLayerCount must have a diagnostic code")
                .to_string(),
            "wasm::oci::invalid_layer_count",
        );
        assert!(
            invalid_count.help().is_some(),
            "InvalidLayerCount must have a help message"
        );

        let invalid_media = OciLayerError::InvalidMediaType {
            found: "application/octet-stream".to_string(),
        };
        assert_eq!(
            invalid_media
                .code()
                .expect("InvalidMediaType must have a diagnostic code")
                .to_string(),
            "wasm::oci::invalid_media_type",
        );
        assert!(
            invalid_media.help().is_some(),
            "InvalidMediaType must have a help message"
        );
    }
}
