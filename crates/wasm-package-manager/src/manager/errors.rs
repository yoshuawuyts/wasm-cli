//! Error types for the package manager.

use miette::Diagnostic;

/// Error type for package manager operation failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
///
/// # Example
///
/// ```rust
/// use miette::Diagnostic;
/// use wasm_package_manager::manager::ManagerError;
///
/// let err = ManagerError::OfflinePull;
/// assert_eq!(
///     err.code().expect("should have a code").to_string(),
///     "wasm::manager::offline_pull",
/// );
///
/// let err = ManagerError::OfflineIndex;
/// assert_eq!(
///     err.code().expect("should have a code").to_string(),
///     "wasm::manager::offline_index",
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub enum ManagerError {
    /// An attempt was made to pull a package while in offline mode.
    #[diagnostic(
        code(wasm::manager::offline_pull),
        help("run without `--offline` to pull packages from the registry")
    )]
    OfflinePull,

    /// An attempt was made to index a package while in offline mode.
    #[diagnostic(
        code(wasm::manager::offline_index),
        help("run without `--offline` to index packages from the registry")
    )]
    OfflineIndex,

    /// A previously indexed package could not be retrieved from the database.
    #[diagnostic(
        code(wasm::manager::index_retrieval_failed),
        help("try re-indexing the package with `wasm registry sync`")
    )]
    IndexRetrievalFailed,

    /// Syncing the package index failed and no local data is available.
    #[diagnostic(
        code(wasm::manager::sync_no_local_data),
        help(
            "{reason}; check your network connection and run \
             `wasm registry sync` to fetch the package index"
        )
    )]
    SyncNoLocalData {
        /// The underlying error message from the failed sync.
        reason: String,
    },
}

impl std::fmt::Display for ManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManagerError::OfflinePull => {
                write!(f, "cannot pull packages in offline mode")
            }
            ManagerError::OfflineIndex => {
                write!(f, "cannot index packages in offline mode")
            }
            ManagerError::IndexRetrievalFailed => {
                write!(f, "failed to retrieve indexed package")
            }
            ManagerError::SyncNoLocalData { reason } => {
                write!(f, "{reason}. No local data available",)
            }
        }
    }
}

impl std::error::Error for ManagerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_variants_have_error_codes() {
        use miette::Diagnostic;

        let offline_pull = ManagerError::OfflinePull;
        assert_eq!(
            offline_pull
                .code()
                .expect("OfflinePull must have a diagnostic code")
                .to_string(),
            "wasm::manager::offline_pull",
        );
        assert!(
            offline_pull.help().is_some(),
            "OfflinePull must have a help message"
        );

        let offline_index = ManagerError::OfflineIndex;
        assert_eq!(
            offline_index
                .code()
                .expect("OfflineIndex must have a diagnostic code")
                .to_string(),
            "wasm::manager::offline_index",
        );
        assert!(
            offline_index.help().is_some(),
            "OfflineIndex must have a help message"
        );

        let index_failed = ManagerError::IndexRetrievalFailed;
        assert_eq!(
            index_failed
                .code()
                .expect("IndexRetrievalFailed must have a diagnostic code")
                .to_string(),
            "wasm::manager::index_retrieval_failed",
        );
        assert!(
            index_failed.help().is_some(),
            "IndexRetrievalFailed must have a help message"
        );

        let sync_failed = ManagerError::SyncNoLocalData {
            reason: "connection refused".to_string(),
        };
        assert_eq!(
            sync_failed
                .code()
                .expect("SyncNoLocalData must have a diagnostic code")
                .to_string(),
            "wasm::manager::sync_no_local_data",
        );
        assert!(
            sync_failed.help().is_some(),
            "SyncNoLocalData must have a help message"
        );
    }
}
