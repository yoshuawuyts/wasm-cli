#![allow(clippy::print_stdout, clippy::print_stderr)]

mod errors;
mod progress_bar;

use futures_concurrency::prelude::*;
use indicatif::MultiProgress;
use miette::{IntoDiagnostic, WrapErr};
use wasm_package_manager::manager::{
    InstallResult, Manager, SyncPolicy, SyncResult, derive_component_name,
};
use wasm_package_manager::resolver::ResolveError;
use wasm_package_manager::types::DependencyItem;
use wasm_package_manager::{ProgressEvent, Reference};

use crate::util::write_lock_file;
use errors::InstallError;
use progress_bar::{ProgressTree, oci_repo_display_name, package_display_parts, run_progress_bars};

/// Default meta-registry URL.
const REGISTRY_URL: &str = "http://localhost:8080";

/// Default sync interval in seconds (1 hour).
const SYNC_INTERVAL: u64 = 3600;

/// Options for the `install` command.
#[derive(clap::Parser)]
pub(crate) struct Opts {
    /// Components to install. Accepts OCI references
    /// (e.g., ghcr.io/webassembly/wasi-logging:1.0.0) or manifest keys
    /// using scope:component syntax (e.g., wasi:logging).
    /// If no arguments are provided, installs all packages listed in the manifest.
    #[arg(value_name = "COMPONENT", num_args = 0..)]
    inputs: Vec<String>,
}

impl Opts {
    pub(crate) async fn run(self, offline: bool) -> miette::Result<()> {
        let manifest_path = std::path::PathBuf::from("wasm.toml");
        let lockfile_path = std::path::PathBuf::from("wasm.lock.toml");
        let wasm_vendor_dir = std::path::PathBuf::from("vendor/wasm");
        let wit_vendor_dir = std::path::PathBuf::from("vendor/wit");

        // Abort early if `wasm.toml` does not exist — guide the user
        if !manifest_path.exists() {
            return Err(InstallError::NoManifest.into());
        }

        // Read existing manifest
        let manifest_str = tokio::fs::read_to_string(&manifest_path)
            .await
            .into_diagnostic()
            .wrap_err_with(|| format!("could not read '{}'", manifest_path.display()))?;
        let mut manifest: wasm_manifest::Manifest =
            toml::from_str(&manifest_str).into_diagnostic()?;

        // Read existing lockfile — create a fresh one when none exists yet.
        let mut lockfile = match tokio::fs::read_to_string(&lockfile_path).await {
            Ok(s) => toml::from_str::<wasm_manifest::Lockfile>(&s).into_diagnostic()?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                wasm_manifest::Lockfile::default()
            }
            Err(e) => {
                return Err(miette::miette!(
                    "could not read '{}': {e}",
                    lockfile_path.display()
                ));
            }
        };

        // Open manager
        let manager = if offline {
            Manager::open_offline()
                .await
                .map_err(crate::util::into_miette)?
        } else {
            Manager::open().await.map_err(crate::util::into_miette)?
        };

        // Sync the local package index from the meta-registry so WIT-style
        // names and search-based lookups can be resolved.
        if !offline {
            match manager
                .sync_from_meta_registry(REGISTRY_URL, SYNC_INTERVAL, SyncPolicy::IfStale)
                .await
            {
                Ok(SyncResult::Degraded { error }) => {
                    tracing::warn!("registry sync failed: {error}");
                }
                Err(e) => {
                    tracing::warn!("{e}");
                }
                // Skipped (interval not elapsed), NotModified (ETag matched),
                // and Updated (new data stored) are all success paths that need
                // no user-visible output.
                Ok(_) => {}
            }
        }

        let start_time = std::time::Instant::now();

        // Pre-install conflict detection: attempt to resolve dependencies for
        // each WIT-style manifest entry.  This catches version conflicts and
        // missing packages *before* any network I/O, so the user gets a clear
        // error without a partial download.
        //
        // We only run the resolver for packages that:
        //   (a) use a WIT-style name as their manifest key, and
        //   (b) declare a parseable semver version.
        // Other entries (bare OCI references, etc.) are not checked here —
        // the sequential install step will surface errors for those.
        //
        // A `Db` error from the resolver means dep-graph data is not yet
        // available for this package (e.g. the meta-registry hasn't indexed
        // its deps, or the sync was skipped in offline mode). In that case we
        // skip silently and let the existing sequential installer handle it.
        if !offline {
            for (key, dep, _) in manifest.all_dependencies() {
                let version_str = match dep {
                    wasm_manifest::Dependency::Compact(s)
                        if looks_like_wit_name(key) && !s.contains('/') =>
                    {
                        s.as_str()
                    }
                    _ => continue,
                };
                let Ok(version) = version_str
                    .trim_start_matches('v')
                    .parse::<wasm_package_manager::resolver::WitVersion>()
                else {
                    continue;
                };
                match manager.resolve_dependencies(key, version) {
                    Ok(_) => {}
                    Err(ResolveError::NoSolution(msg)) => {
                        return Err(InstallError::DependencyConflict { message: msg }.into());
                    }
                    Err(ResolveError::Db(_)) => {} // dep data not yet available; skip
                }
            }
        }

        // Determine the list of (reference, update_manifest) pairs to install.
        // When no inputs are provided, install everything from the manifest.
        // When inputs are provided, each can be:
        //   - An OCI reference → install and add to manifest
        //   - A scope:component manifest key → resolve from manifest and install
        //   - A WIT-style name (e.g. wasi:http) → resolve via known-package DB
        // Each entry is (reference, update_manifest, explicit_name).
        // `explicit_name` is set when the user provided a WIT-style name
        // (e.g. `ba:sample-wasi-http-rust`) so that we use it as the manifest
        // key instead of re-deriving from binary metadata.
        let to_install: Vec<(Reference, bool, Option<String>)> = if self.inputs.is_empty() {
            manifest
                .all_dependencies()
                .map(|(key, dep, _)| {
                    resolve_manifest_dependency(key, dep, &manager)
                        .map(|(r, name)| (r, false, name))
                })
                .collect::<anyhow::Result<Vec<_>>>()
                .map_err(crate::util::into_miette)?
        } else {
            resolve_install_inputs(&self.inputs, &manifest, &manager)?
        };

        // Shared progress display for all concurrent installs.
        let multi = MultiProgress::new();
        let tree = std::sync::Arc::new(tokio::sync::Mutex::new(ProgressTree::new(multi)));

        // `&Manager` is Copy, so each async-move block captures its own copy of
        // the reference without requiring Arc or any synchronisation primitive.
        let manager_ref: &Manager = &manager;

        // Run all installs concurrently.
        let results: anyhow::Result<Vec<_>> = to_install
            .into_co_stream()
            .map(|(reference, update_manifest, explicit_name)| {
                let tree = SharedTree::clone(&tree);
                let vendor_dir = wasm_vendor_dir.clone();
                let wit_vendor_dir = wit_vendor_dir.clone();
                async move {
                    let (name, version) =
                        package_display_parts(explicit_name.as_deref(), reference.tag());
                    let display_name = if name.is_empty() {
                        oci_repo_display_name(reference.repository())
                    } else {
                        name
                    };
                    let result = install_one(
                        manager_ref,
                        &tree,
                        offline,
                        &reference,
                        &vendor_dir,
                        &display_name,
                        version.as_deref(),
                    )
                    .await?;
                    re_vendor_wit_files(&result, &wit_vendor_dir).await?;
                    anyhow::Ok((result, reference, update_manifest, explicit_name))
                }
            })
            .collect()
            .await;

        for (result, _reference, update_manifest, explicit_name) in
            results.map_err(crate::util::into_miette)?
        {
            // Derive the dependency name.
            // When the user provided an explicit WIT-style name (e.g.
            // `ba:sample-wasi-http-rust`), use that directly — the embedded
            // WIT metadata may contain a placeholder like `root:component`.
            // Otherwise, for components use `derive_component_name` which
            // tries WIT metadata, OCI title, last repository segment, then
            // full path.  For interfaces, use the WIT package name.
            let dep_name = if let Some(name) = explicit_name {
                name
            } else if result.is_component {
                let existing_names: std::collections::HashSet<String> = manifest
                    .dependencies
                    .components
                    .keys()
                    .chain(manifest.dependencies.interfaces.keys())
                    .cloned()
                    .collect();
                derive_component_name(
                    result.package_name.as_deref(),
                    result.oci_title.as_deref(),
                    &result.repository,
                    &existing_names,
                )
            } else {
                result.package_name.as_deref().map_or_else(
                    || format!("{}/{}", result.registry, result.repository),
                    |name| name.split('@').next().unwrap_or(name).to_string(),
                )
            };

            // Determine the version from the tag
            let version = result.tag.clone().unwrap_or_default();

            // Add to manifest (compact format) — route to components or interfaces.
            // Only update the manifest when a reference was explicitly provided;
            // for the 0-args case the entries are already in the manifest.
            // The compact format stores the resolved version string (not the
            // full OCI reference), so bare "1.2.3" means ^1.2.3 per Cargo
            // semantics.
            if update_manifest {
                let dep = wasm_manifest::Dependency::Compact(version.clone());
                if result.is_component {
                    manifest
                        .dependencies
                        .components
                        .insert(dep_name.clone(), dep);
                } else {
                    manifest
                        .dependencies
                        .interfaces
                        .insert(dep_name.clone(), dep);
                }
            }

            // Build lockfile dependencies from WIT metadata.
            // Only include dependencies that have a resolved version.
            // Registry and digest are left empty here and resolved later
            // by `Lockfile::resolve_dependency_details()` once all transitive
            // dependencies have been installed.
            let lockfile_deps: Vec<wasm_manifest::PackageDependency> = result
                .dependencies
                .iter()
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
                .collect();

            // Add to lockfile — route to components or interfaces
            let registry_path = format!("{}/{}", result.registry, result.repository);
            let digest = result.digest.unwrap_or_default();

            let package = wasm_manifest::Package {
                name: dep_name.clone(),
                version,
                registry: registry_path.clone(),
                digest,
                dependencies: lockfile_deps,
            };

            upsert_lockfile_package(
                &mut lockfile,
                result.is_component,
                &dep_name,
                &registry_path,
                package,
            );

            // Queue WIT dependencies for recursive installation (transitive deps).
            // These are only added to the lockfile, not the manifest.
            if !offline {
                install_transitive_deps(
                    result.dependencies,
                    manager_ref,
                    &tree,
                    &wasm_vendor_dir,
                    &wit_vendor_dir,
                    &mut lockfile,
                )
                .await
                .map_err(crate::util::into_miette)?;
            }
        }

        // Write updated manifest
        let manifest_str = toml::to_string_pretty(&manifest).into_diagnostic()?;
        tokio::fs::write(&manifest_path, manifest_str.as_bytes())
            .await
            .into_diagnostic()?;

        // Resolve registry and digest for all dependency entries from their
        // matching top-level package entries. Dependency entries whose
        // packages are not in the lockfile (e.g. offline / skipped) are
        // silently removed.
        lockfile.resolve_dependency_details();

        // Write updated lockfile
        write_lock_file(&lockfile_path, &lockfile)
            .await
            .into_diagnostic()?;

        // Print completion message with elapsed time
        let elapsed = start_time.elapsed();
        println!(
            "\n{:>12} installation in {:.1}s",
            console::style("Finished").green().bold(),
            elapsed.as_secs_f64()
        );

        Ok(())
    }
}

/// Shared handle to a [`ProgressTree`] for use across concurrent tasks.
type SharedTree = std::sync::Arc<tokio::sync::Mutex<ProgressTree>>;

/// Install a single package and report progress.
///
/// In offline mode a plain status line is printed. In online mode a
/// progress bar is created for the package showing aggregated download
/// progress across all layers.
async fn install_one(
    manager: &Manager,
    tree: &SharedTree,
    offline: bool,
    reference: &Reference,
    vendor_dir: &std::path::Path,
    display_name: &str,
    display_version: Option<&str>,
) -> anyhow::Result<InstallResult> {
    if offline {
        // No progress bars in offline mode — print a simple status line.
        // Use ├── since we cannot rewrite previous lines to fix up └──.
        let version_str = display_version.map(|v| format!("@{v}")).unwrap_or_default();
        println!(
            "├── {}{}",
            console::style(display_name).yellow(),
            console::style(version_str).white(),
        );
        return manager.install(reference.clone(), vendor_dir).await;
    }

    let (progress_tx, progress_rx) = tokio::sync::mpsc::channel::<ProgressEvent>(64);

    let (pb, bar_id) = tree.lock().await.add_bar(display_name, display_version);

    // Spawn progress rendering task
    let progress_handle = tokio::task::spawn(run_progress_bars(pb.clone(), progress_rx));

    let result = manager
        .install_with_progress(reference.clone(), vendor_dir, &progress_tx)
        .await;

    // Drop the sender to signal the progress task to finish
    drop(progress_tx);

    // Wait for progress bars to finish rendering
    let _ = progress_handle.await;

    // Only mark the bar as complete (green, hidden) on successful installs.
    if result.is_ok() {
        tree.lock().await.finish_bar(bar_id);
    }

    result
}

/// Unpack vendored WIT `.wasm` binaries into `.wit` text files.
///
/// WIT-only packages (types) are initially stored alongside components in
/// `vendor/wasm/`. This function decodes each binary into its textual WIT
/// representation and writes it to `vendor/wit/` so that WIT tooling can
/// find them at the conventional location.
// r[impl install.wit-unpack]
async fn re_vendor_wit_files(
    result: &InstallResult,
    wit_vendor_dir: &std::path::Path,
) -> anyhow::Result<()> {
    if result.is_component {
        return Ok(());
    }
    for file in &result.vendored_files {
        let wasm_bytes = tokio::fs::read(file).await?;
        let wit_text =
            wasm_package_manager::types::extract_wit_text(&wasm_bytes).ok_or_else(|| {
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

/// Recursively install transitive WIT dependencies of a component.
///
/// Uses a work queue and visited set to avoid cycles and duplicates.
/// Each resolved dependency is installed, vendored to `wit/`, and added
/// to `lockfile.interfaces`. The manifest is **not** modified.
async fn install_transitive_deps(
    initial_deps: Vec<DependencyItem>,
    manager: &Manager,
    tree: &SharedTree,
    wasm_vendor_dir: &std::path::Path,
    wit_vendor_dir: &std::path::Path,
    lockfile: &mut wasm_manifest::Lockfile,
) -> anyhow::Result<()> {
    let mut work_queue = std::collections::VecDeque::from(initial_deps);
    let mut visited: std::collections::HashSet<(String, Option<String>)> =
        std::collections::HashSet::new();

    while let Some(dep) = work_queue.pop_front() {
        let dep_key = (dep.package.clone(), dep.version.clone());
        // `insert` returns `false` when the key was already present
        if !visited.insert(dep_key) {
            continue;
        }

        // Skip if already present in the lockfile
        if lockfile.interfaces.iter().any(|p| p.name == dep.package) {
            continue;
        }

        let Some(dep_ref) = resolve_dep_reference(manager, &dep) else {
            continue;
        };

        let (name, version) = package_display_parts(Some(&dep.package), dep_ref.tag());
        let display_name = if name.is_empty() {
            dep.package.clone()
        } else {
            name
        };

        let dep_result = match install_one(
            manager,
            tree,
            false,
            &dep_ref,
            wasm_vendor_dir,
            &display_name,
            version.as_deref(),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(
                    "Failed to install WIT dependency '{}': {e} — skipping",
                    dep.package,
                );
                continue;
            }
        };

        if let Err(e) = re_vendor_wit_files(&dep_result, wit_vendor_dir).await {
            tracing::debug!(
                "Failed to vendor WIT files for '{}': {e} — skipping",
                dep.package,
            );
        }

        upsert_lockfile_type(lockfile, &dep_result);

        // Queue transitive dependencies (consuming dep_result.dependencies)
        for transitive_dep in dep_result.dependencies {
            work_queue.push_back(transitive_dep);
        }
    }

    Ok(())
}

/// Try to resolve a [`DependencyItem`] to an OCI [`Reference`].
///
/// Returns `None` (with a debug log) if the dependency cannot be resolved.
fn resolve_dep_reference(manager: &Manager, dep: &DependencyItem) -> Option<Reference> {
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

/// Build a [`wasm_manifest::Package`] from an [`InstallResult`] and upsert it
/// into `lockfile.interfaces`.
fn upsert_lockfile_type(lockfile: &mut wasm_manifest::Lockfile, result: &InstallResult) {
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
fn upsert_lockfile_package(
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

/// Resolve a manifest dependency to an OCI [`Reference`].
///
/// When the dependency uses the compact format with just a version string
/// (e.g. `"0.1.6"`) rather than a full OCI reference, the manifest key
/// (e.g. `ba:sample-wasi-http-rust`) is used to look up the package in the
/// known-package database.
fn resolve_manifest_dependency(
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

/// Convert a manifest [`wasm_manifest::Dependency`] into an OCI [`Reference`].
///
/// Both the compact string format (`"ghcr.io/webassembly/wasi-logging:1.0.0"`) and
/// the explicit table format (`registry`/`namespace`/`package`:`version`) are
/// supported. Returns an error if the resulting reference string cannot be parsed
/// as a valid OCI reference.
fn reference_from_dependency(dep: &wasm_manifest::Dependency) -> anyhow::Result<Reference> {
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
    crate::util::parse_reference(&s)
        .map_err(|e| InstallError::InvalidReference { reason: e }.into())
}

/// Resolve CLI install inputs into `(Reference, update_manifest)` pairs.
///
/// Each input is first checked against manifest keys (e.g., `wasi:logging`).
/// If no match is found and the input looks like a WIT-style name
/// (`namespace:package`), it is resolved via the known-package database.
/// Otherwise, it is tried as an OCI reference. Returns an error when
/// neither interpretation works.
fn resolve_install_inputs(
    inputs: &[String],
    manifest: &wasm_manifest::Manifest,
    manager: &Manager,
) -> miette::Result<Vec<(Reference, bool, Option<String>)>> {
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
                .map_err(crate::util::into_miette)?;
            result.push((reference, false, explicit_name));
            continue;
        }

        // If it looks like a WIT-style name (e.g. `wasi:http`), resolve via
        // the known-package database instead of treating it as a bare OCI
        // reference (which would incorrectly default to docker.io/library/).
        // Preserve the user's input as the explicit name so it becomes the
        // manifest key — the embedded WIT metadata may use a placeholder.
        if looks_like_wit_name(input) {
            let reference = resolve_wit_name(input, manager).map_err(crate::util::into_miette)?;
            result.push((reference, true, Some(input.clone())));
            continue;
        }

        // Try as OCI reference
        match crate::util::parse_reference(input) {
            Ok(reference) => result.push((reference, true, None)),
            Err(_) => {
                return Err(InstallError::InvalidInput {
                    input: input.clone(),
                }
                .into());
            }
        }
    }
    Ok(result)
}

/// Check whether `input` looks like a WIT-style name (`namespace:package`).
///
/// WIT-style names use `namespace:package` syntax (e.g. `wasi:http`) or
/// `namespace:package@version` (e.g. `wasi:http@0.2.10`) without dots or
/// slashes in the namespace/package part, which distinguishes them from OCI
/// references (e.g. `ghcr.io/user/repo:tag`).
///
/// Inputs with an empty version after `@` (e.g. `wasi:http@`) or multiple
/// `@` signs are rejected.
fn looks_like_wit_name(input: &str) -> bool {
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
fn resolve_wit_name(input: &str, manager: &Manager) -> anyhow::Result<Reference> {
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

    /// Build a binary WIT package using `wit-component::encode`.
    fn build_test_wit_wasm() -> Vec<u8> {
        use wit_parser::{PackageName, Resolve};

        let mut resolve = Resolve::default();
        let package = wit_parser::Package {
            name: PackageName {
                namespace: "test".to_string(),
                name: "example".to_string(),
                version: Some(semver::Version::new(1, 0, 0)),
            },
            docs: Default::default(),
            interfaces: Default::default(),
            worlds: Default::default(),
        };
        let pkg_id = resolve.packages.alloc(package);

        let iface = wit_parser::Interface {
            name: Some("greeter".to_string()),
            docs: Default::default(),
            types: Default::default(),
            functions: Default::default(),
            package: Some(pkg_id),
            stability: Default::default(),
            span: Default::default(),
            clone_of: None,
        };
        let iface_id = resolve.interfaces.alloc(iface);
        resolve.packages[pkg_id]
            .interfaces
            .insert("greeter".into(), iface_id);

        wit_component::encode(&resolve, pkg_id).expect("encoding should succeed")
    }

    // r[verify install.wit-unpack]
    #[tokio::test]
    async fn re_vendor_wit_files_unpacks_binary_to_parseable_wit() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let wasm_dir = tmp.path().join("vendor/wasm");
        let wit_dir = tmp.path().join("vendor/wit");
        std::fs::create_dir_all(&wasm_dir).expect("should create wasm vendor dir");

        // Write a binary WIT package into the wasm vendor dir.
        let wasm_bytes = build_test_wit_wasm();
        let wasm_path = wasm_dir.join("test__example.wasm");
        std::fs::write(&wasm_path, &wasm_bytes).expect("should write test .wasm file");

        let result = InstallResult {
            registry: "ghcr.io".into(),
            repository: "test/example".into(),
            tag: Some("v1.0.0".into()),
            digest: None,
            package_name: Some("test:example@1.0.0".into()),
            oci_title: None,
            vendored_files: vec![wasm_path.clone()],
            is_component: false,
            dependencies: vec![],
        };

        // Run the function under test.
        re_vendor_wit_files(&result, &wit_dir)
            .await
            .expect("re_vendor should succeed");

        // The original .wasm must have been removed.
        assert!(
            !wasm_path.exists(),
            "original .wasm should be deleted after unpack"
        );

        // vendor/wit/ must contain exactly one .wit file.
        let wit_entries: Vec<_> = std::fs::read_dir(&wit_dir)
            .expect("wit dir should exist")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(wit_entries.len(), 1, "expected exactly one vendored file");
        let wit_file = &wit_entries[0].path();
        assert_eq!(
            wit_file.extension().and_then(|e| e.to_str()),
            Some("wit"),
            "vendored file must have .wit extension"
        );

        // No .wasm files should remain in vendor/wit/.
        let wasm_in_wit: Vec<_> = std::fs::read_dir(&wit_dir)
            .expect("should read wit vendor dir")
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "wasm"))
            .collect();
        assert!(
            wasm_in_wit.is_empty(),
            "no .wasm files should be in vendor/wit/"
        );

        // The .wit file contents must be valid WIT, parseable by wit-parser.
        let wit_text = std::fs::read_to_string(wit_file).expect("should read .wit file");
        let mut resolve = wit_parser::Resolve::default();
        resolve
            .push_str(
                wit_file.to_str().expect("path should be valid UTF-8"),
                &wit_text,
            )
            .expect("vendored .wit file must be valid WIT");
    }

    #[tokio::test]
    async fn re_vendor_wit_files_skips_components() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let wasm_dir = tmp.path().join("vendor/wasm");
        let wit_dir = tmp.path().join("vendor/wit");
        std::fs::create_dir_all(&wasm_dir).expect("should create wasm vendor dir");

        let wasm_path = wasm_dir.join("component.wasm");
        std::fs::write(&wasm_path, b"irrelevant").expect("should write test .wasm file");

        let result = InstallResult {
            registry: "ghcr.io".into(),
            repository: "test/comp".into(),
            tag: None,
            digest: None,
            package_name: None,
            oci_title: None,
            vendored_files: vec![wasm_path.clone()],
            is_component: true,
            dependencies: vec![],
        };

        re_vendor_wit_files(&result, &wit_dir)
            .await
            .expect("should succeed for component (no-op)");

        // vendor/wit/ should not be created for components.
        assert!(
            !wit_dir.exists(),
            "wit dir should not be created for components"
        );
        // Original .wasm should still be present (not moved).
        assert!(wasm_path.exists(), "component .wasm should be untouched");
    }
}
