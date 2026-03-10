//! Install helpers — core logic for resolving inputs, managing lockfiles,
//! and unpacking WIT files.
//!
//! These functions underpin both the CLI `wasm install` command and any
//! programmatic consumers of the package manager library.

use oci_client::Reference;

use crate::manager::{InstallResult, Manager};
use crate::types::DependencyItem;

mod errors;

pub use errors::InstallError;

// ---------------------------------------------------------------------------
// WIT name detection & resolution
// ---------------------------------------------------------------------------

/// Check whether `input` looks like a WIT-style name (`namespace:package`).
///
/// WIT-style names use `namespace:package` syntax (e.g. `wasi:http`) or
/// `namespace:package@version` (e.g. `wasi:http@0.2.10`) without dots or
/// slashes in the namespace/package part, which distinguishes them from OCI
/// references (e.g. `ghcr.io/user/repo:tag`).
///
/// Inputs with an empty version after `@` (e.g. `wasi:http@`) or multiple
/// `@` signs are rejected.
#[must_use]
pub fn looks_like_wit_name(input: &str) -> bool {
    let Some((scope, rest)) = input.split_once(':') else {
        return false;
    };
    // Split the component from an optional `@version` suffix.
    let component = match rest.split_once('@') {
        Some((comp, ver)) => {
            // Reject empty version or multiple `@` signs.
            if ver.is_empty() || ver.contains('@') {
                return false;
            }
            comp
        }
        None => rest,
    };
    !scope.is_empty()
        && !component.is_empty()
        && !scope.contains('/')
        && !scope.contains('.')
        && !component.contains('/')
        && !component.contains('.')
}

/// Resolve a WIT-style name (e.g. `wasi:http` or `wasi:http@0.2.10`) to
/// an OCI [`Reference`] via the known-package database.
///
/// The caller must ensure the input passes [`looks_like_wit_name`] first,
/// which rejects empty versions and multiple `@` signs.
///
/// # Errors
///
/// Returns [`InstallError::UnknownPackage`] when the package cannot be found
/// in the known-package index.
pub fn resolve_wit_name(input: &str, manager: &Manager) -> anyhow::Result<Reference> {
    let (package, version) = match input.split_once('@') {
        Some((pkg, ver)) if !ver.is_empty() => (pkg.to_string(), Some(ver.to_string())),
        _ => (input.to_string(), None),
    };
    let dep = DependencyItem { package, version };
    match manager.resolve_wit_dependency(&dep)? {
        Some(reference) => Ok(reference),
        None => Err(InstallError::UnknownPackage {
            input: input.to_string(),
        }
        .into()),
    }
}

// ---------------------------------------------------------------------------
// Reference resolution
// ---------------------------------------------------------------------------

/// Convert a manifest [`wasm_manifest::Dependency`] into an OCI [`Reference`].
///
/// Both the compact string format (`"ghcr.io/webassembly/wasi-logging:1.0.0"`) and
/// the explicit table format (`registry`/`namespace`/`package`:`version`) are
/// supported. Returns an error if the resulting reference string cannot be parsed
/// as a valid OCI reference.
pub fn reference_from_dependency(dep: &wasm_manifest::Dependency) -> anyhow::Result<Reference> {
    let s = match dep {
        wasm_manifest::Dependency::Compact(s) => s.clone(),
        wasm_manifest::Dependency::Explicit {
            registry,
            namespace,
            package,
            version,
            ..
        } => format!("{registry}/{namespace}/{package}:{version}"),
    };
    crate::parse_reference(&s).map_err(|e| InstallError::InvalidReference { reason: e }.into())
}

/// Resolve a manifest dependency to an OCI [`Reference`].
///
/// When the dependency uses the compact format with just a version string
/// (e.g. `"0.1.6"`) rather than a full OCI reference, the manifest key
/// (e.g. `ba:sample-wasi-http-rust`) is used to look up the package in the
/// known-package database.
///
/// Returns `(reference, explicit_name)` where `explicit_name` is set when
/// the user provided a WIT-style name.
pub fn resolve_manifest_dependency(
    key: &str,
    dep: &wasm_manifest::Dependency,
    manager: &Manager,
) -> anyhow::Result<(Reference, Option<String>)> {
    match dep {
        wasm_manifest::Dependency::Compact(s) if !s.contains('/') && looks_like_wit_name(key) => {
            // The compact value contains no '/' so it is a version string
            // (e.g. "0.1.6") rather than an OCI reference path.
            // Resolve through the known-package DB using the manifest key.
            let input = format!("{key}@{s}");
            let reference = resolve_wit_name(&input, manager)?;
            Ok((reference, Some(key.to_string())))
        }
        _ => {
            let reference = reference_from_dependency(dep)?;
            Ok((reference, None))
        }
    }
}

/// Resolve CLI install inputs into `(Reference, update_manifest, explicit_name)` triples.
///
/// Each input is first checked against manifest keys (e.g., `wasi:logging`).
/// If no match is found and the input looks like a WIT-style name
/// (`namespace:package`), it is resolved via the known-package database.
/// Otherwise, it is tried as an OCI reference. Returns an error when
/// neither interpretation works.
pub fn resolve_install_inputs(
    inputs: &[String],
    manifest: &wasm_manifest::Manifest,
    manager: &Manager,
) -> Result<Vec<(Reference, bool, Option<String>)>, InstallError> {
    let mut result = Vec::with_capacity(inputs.len());
    for input in inputs {
        // Try as scope:component manifest key first
        let dep = manifest
            .dependencies
            .components
            .get(input)
            .or_else(|| manifest.dependencies.interfaces.get(input));

        if let Some(dep) = dep {
            let (reference, explicit_name) = resolve_manifest_dependency(input, dep, manager)
                .map_err(|e| InstallError::ResolveFailure {
                    reason: e.to_string(),
                })?;
            result.push((reference, false, explicit_name));
            continue;
        }

        // If it looks like a WIT-style name (e.g. `wasi:http`), resolve via
        // the known-package database instead of treating it as a bare OCI
        // reference (which would incorrectly default to docker.io/library/).
        // Preserve the user's input as the explicit name so it becomes the
        // manifest key — the embedded WIT metadata may use a placeholder.
        if looks_like_wit_name(input) {
            let reference =
                resolve_wit_name(input, manager).map_err(|e| InstallError::ResolveFailure {
                    reason: e.to_string(),
                })?;
            result.push((reference, true, Some(input.clone())));
            continue;
        }

        // Try as OCI reference
        match crate::parse_reference(input) {
            Ok(reference) => result.push((reference, true, None)),
            Err(_) => {
                return Err(InstallError::InvalidInput {
                    input: input.clone(),
                });
            }
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Lockfile management
// ---------------------------------------------------------------------------

/// Build a [`wasm_manifest::Package`] from an [`InstallResult`] and upsert it
/// into `lockfile.interfaces`.
pub fn upsert_lockfile_type(lockfile: &mut wasm_manifest::Lockfile, result: &InstallResult) {
    let name = result.package_name.as_deref().map_or_else(
        || format!("{}/{}", result.registry, result.repository),
        |n| n.split('@').next().unwrap_or(n).to_string(),
    );
    let registry = format!("{}/{}", result.registry, result.repository);
    let package = wasm_manifest::Package {
        name: name.clone(),
        version: result.tag.clone().unwrap_or_default(),
        registry: registry.clone(),
        digest: result.digest.clone().unwrap_or_default(),
        dependencies: result
            .dependencies
            .iter()
            // Only include dependencies with a resolved version.
            // Registry and digest are resolved later by
            // `Lockfile::resolve_dependency_details()`.
            .filter_map(|d| {
                d.version
                    .clone()
                    .map(|version| wasm_manifest::PackageDependency {
                        name: d.package.clone(),
                        version,
                        registry: String::new(),
                        digest: String::new(),
                    })
            })
            .collect(),
    };

    if let Some(existing) = lockfile
        .interfaces
        .iter_mut()
        .find(|p| p.name == name && p.registry == registry)
    {
        *existing = package;
    } else {
        lockfile.interfaces.push(package);
    }
}

/// Upsert a package into the appropriate lockfile section (components or interfaces).
///
/// If a matching entry (same `name` and `registry`) already exists, it is
/// replaced; otherwise the package is appended.
pub fn upsert_lockfile_package(
    lockfile: &mut wasm_manifest::Lockfile,
    is_component: bool,
    dep_name: &str,
    registry_path: &str,
    package: wasm_manifest::Package,
) {
    let packages = if is_component {
        &mut lockfile.components
    } else {
        &mut lockfile.interfaces
    };
    match packages
        .iter_mut()
        .find(|p| p.name == dep_name && p.registry == registry_path)
    {
        Some(existing) => *existing = package,
        None => packages.push(package),
    }
}

// ---------------------------------------------------------------------------
// WIT unpack
// ---------------------------------------------------------------------------

/// Unpack vendored WIT `.wasm` binaries into `.wit` text files.
///
/// WIT-only packages (types) are initially stored alongside components in
/// `vendor/wasm/`. This function decodes each binary into its textual WIT
/// representation and writes it to `vendor/wit/` so that WIT tooling can
/// find them at the conventional location.
// r[impl install.wit-unpack]
pub async fn re_vendor_wit_files(
    result: &InstallResult,
    wit_vendor_dir: &std::path::Path,
) -> anyhow::Result<()> {
    if result.is_component {
        return Ok(());
    }
    for file in &result.vendored_files {
        let wasm_bytes = tokio::fs::read(file).await?;
        let wit_text = crate::types::extract_wit_text(&wasm_bytes).ok_or_else(|| {
            anyhow::anyhow!(
                "'{}' is not a valid WIT package — could not decode binary to WIT text",
                file.display()
            )
        })?;

        if let Some(filename) = file.file_name() {
            let wit_dest = wit_vendor_dir.join(filename).with_extension("wit");
            tokio::fs::create_dir_all(wit_vendor_dir).await?;
            tokio::fs::write(&wit_dest, wit_text).await?;
        }

        // Remove the original binary now that it has been unpacked.
        match tokio::fs::remove_file(file).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Transitive dependency resolution
// ---------------------------------------------------------------------------

/// Try to resolve a [`DependencyItem`] to an OCI [`Reference`].
///
/// Returns `None` (with a debug log) if the dependency cannot be resolved.
#[must_use]
pub fn resolve_dep_reference(manager: &Manager, dep: &DependencyItem) -> Option<Reference> {
    match manager.resolve_wit_dependency(dep) {
        Ok(Some(r)) => Some(r),
        Ok(None) => {
            tracing::debug!(
                "Could not resolve WIT dependency '{}' — skipping",
                dep.package
            );
            None
        }
        Err(e) => {
            tracing::debug!(
                "Error resolving WIT dependency '{}': {e} — skipping",
                dep.package,
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_wit_name_bare() {
        assert!(looks_like_wit_name("wasi:http"));
        assert!(looks_like_wit_name("wasi:logging"));
    }

    #[test]
    fn looks_like_wit_name_with_version() {
        assert!(looks_like_wit_name("wasi:http@0.2.10"));
        assert!(looks_like_wit_name("wasi:http@0.3.0-preview-2026-02-20"));
    }

    #[test]
    fn looks_like_wit_name_rejects_oci() {
        assert!(!looks_like_wit_name("ghcr.io/user/repo:tag"));
        assert!(!looks_like_wit_name("docker.io/library/nginx:latest"));
    }

    #[test]
    fn looks_like_wit_name_rejects_invalid() {
        assert!(!looks_like_wit_name("no-colon"));
        assert!(!looks_like_wit_name(":missing-scope"));
        assert!(!looks_like_wit_name("missing-component:"));
    }

    #[test]
    fn looks_like_wit_name_rejects_empty_version() {
        assert!(!looks_like_wit_name("wasi:http@"));
    }

    #[test]
    fn looks_like_wit_name_rejects_multiple_at() {
        assert!(!looks_like_wit_name("wasi:http@0.2@extra"));
    }
}
