#![allow(clippy::print_stdout, clippy::print_stderr)]

mod errors;

use futures_concurrency::prelude::*;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use miette::{IntoDiagnostic, WrapErr};
use wasm_package_manager::manager::{
    InstallResult, Manager, SyncPolicy, SyncResult, derive_component_name,
};
use wasm_package_manager::types::DependencyItem;
use wasm_package_manager::{ProgressEvent, Reference};

use crate::util::write_lock_file;
use errors::InstallError;

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
        let deps = std::path::Path::new("deps");
        let manifest_path = deps.join("wasm.toml");
        let lockfile_path = deps.join("wasm.lock.toml");
        let wasm_vendor_dir = deps.join("vendor/wasm");
        let wit_vendor_dir = deps.join("vendor/wit");

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

        // Read existing lockfile
        let lockfile_str = tokio::fs::read_to_string(&lockfile_path)
            .await
            .into_diagnostic()
            .wrap_err_with(|| format!("could not read '{}'", lockfile_path.display()))?;
        let mut lockfile: wasm_manifest::Lockfile =
            toml::from_str(&lockfile_str).into_diagnostic()?;

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

        // Determine the list of (reference, update_manifest) pairs to install.
        // When no inputs are provided, install everything from the manifest.
        // When inputs are provided, each can be:
        //   - An OCI reference → install and add to manifest
        //   - A scope:component manifest key → resolve from manifest and install
        //   - A WIT-style name (e.g. wasi:http) → resolve via known-package DB
        let to_install: Vec<(Reference, bool)> = if self.inputs.is_empty() {
            manifest
                .all_dependencies()
                .map(|(_, dep, _)| reference_from_dependency(dep).map(|r| (r, false)))
                .collect::<anyhow::Result<Vec<_>>>()
                .map_err(crate::util::into_miette)?
        } else {
            resolve_install_inputs(&self.inputs, &manifest, &manager)?
        };

        // Shared progress display for all concurrent installs.
        let multi = MultiProgress::new();

        // `&Manager` is Copy, so each async-move block captures its own copy of
        // the reference without requiring Arc or any synchronisation primitive.
        let manager_ref: &Manager = &manager;

        // Run all installs concurrently.
        let results: anyhow::Result<Vec<_>> = to_install
            .into_co_stream()
            .map(|(reference, update_manifest)| {
                let multi = multi.clone();
                let vendor_dir = wasm_vendor_dir.clone();
                let wit_vendor_dir = wit_vendor_dir.clone();
                async move {
                    let result =
                        install_one(manager_ref, multi, offline, &reference, &vendor_dir).await?;
                    re_vendor_wit_files(&result, &wit_vendor_dir).await?;
                    anyhow::Ok((result, reference, update_manifest))
                }
            })
            .collect()
            .await;

        for (result, _reference, update_manifest) in results.map_err(crate::util::into_miette)? {
            // Derive the dependency name.
            // For components, use `derive_component_name` which tries WIT metadata,
            // OCI title annotation, last repository segment, then full path.
            // For interfaces, use the WIT package name (always available).
            let dep_name = if result.is_component {
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

            // Build lockfile dependencies from WIT metadata
            let lockfile_deps: Vec<wasm_manifest::PackageDependency> = result
                .dependencies
                .iter()
                .map(|d| wasm_manifest::PackageDependency {
                    name: d.package.clone(),
                    version: d.version.clone().unwrap_or_default(),
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
                    &multi,
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

/// Install a single package and report progress.
///
/// In offline mode a plain status line is printed. In online mode a
/// [`MultiProgress`] header bar is created for the package and per-layer
/// bars are rendered by a background task.
async fn install_one(
    manager: &Manager,
    multi: MultiProgress,
    offline: bool,
    reference: &Reference,
    vendor_dir: &std::path::Path,
) -> anyhow::Result<InstallResult> {
    let reference_display = reference.whole().clone();

    if offline {
        // No progress bars in offline mode — just print the line
        println!(
            "{:>12} {}",
            console::style("Installing").cyan().bold(),
            reference_display,
        );
        return manager.install(reference.clone(), vendor_dir).await;
    }

    let (progress_tx, progress_rx) = tokio::sync::mpsc::channel::<ProgressEvent>(64);

    // Add a header line managed by the shared multi-progress so it
    // stays above the per-layer bars and can be rewritten.
    let header = multi.add(ProgressBar::new_spinner());
    header.set_style(ProgressStyle::with_template("{msg}").expect("valid progress bar template"));
    header.set_message(format!(
        "{:>12} {}",
        console::style("Installing").cyan().bold(),
        reference_display,
    ));

    // Spawn progress rendering task
    let progress_handle = tokio::task::spawn(run_progress_bars(multi, progress_rx));

    let result = manager
        .install_with_progress(reference.clone(), vendor_dir, &progress_tx)
        .await;

    // Drop the sender to signal the progress task to finish
    drop(progress_tx);

    // Wait for progress bars to finish rendering
    let _ = progress_handle.await;

    // Rewrite the header line: blue → green
    header.set_message(format!(
        "{:>12} {}",
        console::style("Installing").green().bold(),
        reference_display,
    ));
    header.finish();

    result
}

/// Move vendored WIT files from the wasm vendor dir into the wit vendor dir.
///
/// WIT-only packages (types) are initially stored alongside components in
/// `deps/vendor/wasm/`. This function moves them to `deps/vendor/wit/` so that
/// WIT tooling can find them at the conventional location.
async fn re_vendor_wit_files(
    result: &InstallResult,
    wit_vendor_dir: &std::path::Path,
) -> anyhow::Result<()> {
    if result.is_component {
        return Ok(());
    }
    for file in &result.vendored_files {
        if let Some(filename) = file.file_name() {
            let wit_dest = wit_vendor_dir.join(filename);
            tokio::fs::create_dir_all(wit_vendor_dir).await?;
            let _ = tokio::fs::remove_file(&wit_dest).await;
            tokio::fs::rename(file, &wit_dest).await?;
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
    multi: &MultiProgress,
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

        let dep_result =
            match install_one(manager, multi.clone(), false, &dep_ref, wasm_vendor_dir).await {
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
            .map(|d| wasm_manifest::PackageDependency {
                name: d.package.clone(),
                version: d.version.clone().unwrap_or_default(),
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
) -> miette::Result<Vec<(Reference, bool)>> {
    let mut result = Vec::with_capacity(inputs.len());
    for input in inputs {
        // Try as scope:component manifest key first
        let dep = manifest
            .dependencies
            .components
            .get(input)
            .or_else(|| manifest.dependencies.interfaces.get(input));

        if let Some(dep) = dep {
            let reference = reference_from_dependency(dep).map_err(crate::util::into_miette)?;
            result.push((reference, false));
            continue;
        }

        // If it looks like a WIT-style name (e.g. `wasi:http`), resolve via
        // the known-package database instead of treating it as a bare OCI
        // reference (which would incorrectly default to docker.io/library/).
        if looks_like_wit_name(input) {
            let reference = resolve_wit_name(input, manager).map_err(crate::util::into_miette)?;
            result.push((reference, true));
            continue;
        }

        // Try as OCI reference
        match crate::util::parse_reference(input) {
            Ok(reference) => result.push((reference, true)),
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
    let (component, version_part) = match rest.split_once('@') {
        Some((comp, ver)) => {
            // Reject empty version or multiple `@` signs.
            if ver.is_empty() || ver.contains('@') {
                return false;
            }
            (comp, Some(ver))
        }
        None => (rest, None),
    };
    // Reject empty version (already caught above) — belt and suspenders.
    let _ = version_part;
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

/// Consume progress events and render tree-style multi-progress bars.
async fn run_progress_bars(
    multi: MultiProgress,
    mut rx: tokio::sync::mpsc::Receiver<ProgressEvent>,
) {
    let mut bars: Vec<ProgressBar> = Vec::new();
    let mut layer_count: usize = 0;

    // In-progress style: blue bar + blue bytes + eta
    let bar_style_progress = ProgressStyle::with_template(
        "{prefix} {bar:12.blue} {bytes:.blue}/{total_bytes:.blue} {eta}",
    )
    .expect("valid progress bar template")
    .progress_chars("━━┄");

    // In-progress spinner style (unknown size)
    let bar_style_spinner = ProgressStyle::with_template("{prefix} {spinner:.blue} {bytes}")
        .expect("valid progress bar template");

    // Completed style: green filled bar + green bytes
    let bar_style_done = ProgressStyle::with_template("{prefix} {bar:12.green} {total_bytes}")
        .expect("valid progress bar template")
        .progress_chars("━━━");

    while let Some(event) = rx.recv().await {
        match event {
            ProgressEvent::ManifestFetched {
                layer_count: count, ..
            } => {
                layer_count = count;
            }
            ProgressEvent::LayerStarted {
                index,
                ref digest,
                total_bytes,
                ref title,
                ref media_type,
            } => {
                // Tree glyph: ├── for non-last, └── for last
                let tree_glyph = if layer_count > 0 && index + 1 < layer_count {
                    "├──"
                } else {
                    "└──"
                };

                let short_digest = digest
                    .strip_prefix("sha256:")
                    .unwrap_or(digest)
                    .get(..5)
                    .unwrap_or(digest);

                // Prefer title annotation, fall back to media type
                let label = title.as_deref().unwrap_or(media_type);
                let prefix = format!("   {tree_glyph} [{short_digest}] {label}");

                let pb = if let Some(total) = total_bytes {
                    let pb = multi.add(ProgressBar::new(total));
                    pb.set_style(bar_style_progress.clone());
                    pb
                } else {
                    let pb = multi.add(ProgressBar::new_spinner());
                    pb.set_style(bar_style_spinner.clone());
                    pb
                };
                pb.set_prefix(prefix);

                // Ensure the bars vec is large enough
                while bars.len() <= index {
                    bars.push(ProgressBar::hidden());
                }
                if let Some(slot) = bars.get_mut(index) {
                    *slot = pb;
                }
            }
            ProgressEvent::LayerProgress {
                index,
                bytes_downloaded,
            } => {
                if let Some(pb) = bars.get(index) {
                    pb.set_position(bytes_downloaded);
                }
            }
            ProgressEvent::LayerDownloaded { .. } => {
                // Download complete — will be marked done on LayerStored
            }
            ProgressEvent::LayerStored { index } => {
                if let Some(pb) = bars.get(index) {
                    pb.set_style(bar_style_done.clone());
                    pb.finish();
                }
            }
            ProgressEvent::InstallComplete => {
                for pb in &bars {
                    if !pb.is_finished() {
                        pb.set_style(bar_style_done.clone());
                        pb.finish();
                    }
                }
            }
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
