use std::path::Path;

use wasm_manifest::Lockfile;
use wasm_package_manager::Reference;

/// Parse an OCI reference string, stripping the optional `oci://` scheme prefix.
///
/// Delegates to [`wasm_package_manager::parse_reference`].
pub(crate) fn parse_reference(s: &str) -> Result<Reference, String> {
    wasm_package_manager::parse_reference(s)
}

/// Convert an error into a [`miette::Report`], preserving the cause chain.
///
/// This bridges subsystems that return [`anyhow::Error`], [`wasmtime::Error`],
/// or other `Display` errors with the top-level CLI that uses [`miette`] for
/// rich error display.
#[allow(dead_code, clippy::needless_pass_by_value)]
pub(crate) fn into_miette(err: impl std::fmt::Display) -> miette::Report {
    // Use the alternate Display format which renders the full cause chain
    // as "outer: inner: root cause".
    miette::miette!("{err:#}")
}

/// Write a lockfile to disk with a header comment.
///
/// Delegates to [`wasm_package_manager::write_lock_file`].
#[allow(dead_code)]
pub(crate) async fn write_lock_file<P: AsRef<Path>>(
    path: P,
    lock: &Lockfile,
) -> std::io::Result<()> {
    wasm_package_manager::write_lock_file(path, lock).await
}
