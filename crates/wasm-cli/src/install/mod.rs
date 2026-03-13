#![allow(clippy::print_stdout, clippy::print_stderr)]

mod errors;
mod progress_bar;

use std::collections::{HashMap, HashSet};

use futures_concurrency::prelude::*;
use indicatif::MultiProgress;
use miette::{IntoDiagnostic, WrapErr};
use wasm_package_manager::manager::{
    InstallResult, Manager, SyncPolicy, SyncResult, derive_component_name,
    install::{
        looks_like_wit_name, re_vendor_wit_files, resolve_dep_reference, resolve_install_inputs,
        resolve_manifest_dependency, upsert_lockfile_package, upsert_lockfile_type,
    },
};
use wasm_package_manager::resolver::ResolveError;
use wasm_package_manager::types::DependencyItem;
use wasm_package_manager::{ProgressEvent, Reference};

use crate::util::write_lock_file;
use errors::InstallError;
use progress_bar::{
    InstallDisplay, oci_repo_display_name, package_display_parts, run_progress_bars,
};

/// Default meta-registry URL.
const REGISTRY_URL: &str = Manager::DEFAULT_REGISTRY_URL;

/// Default sync interval in seconds (1 hour).
const SYNC_INTERVAL: u64 = Manager::DEFAULT_SYNC_INTERVAL;

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

        // Shared progress display for all concurrent installs.
        let multi = MultiProgress::new();
        let display = std::sync::Arc::new(tokio::sync::Mutex::new(InstallDisplay::new(multi)));

        // Sync the local package index from the meta-registry so WIT-style
        // names and search-based lookups can be resolved.
        if !offline {
            display.lock().await.start_sync();
            let sync_result = manager
                .sync_from_meta_registry(REGISTRY_URL, SYNC_INTERVAL, SyncPolicy::IfStale)
                .await;
            match sync_result {
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

        // Start the planning phase spinner.
        if !offline {
            display.lock().await.start_planning();
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
        //
        // Built *before* the resolver so that both manifest entries and CLI
        // inputs can be fed into the PubGrub planning pass.
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

        // Pre-install conflict detection + transitive dependency planning.
        //
        // Collect all packages to install that have WIT-style names (both
        // interfaces and registry components) with parseable semver versions,
        // then resolve them *all* in a single PubGrub pass via
        // `resolve_all_dependencies`.  This covers both manifest entries and
        // CLI inputs so that `wasm install ba:foo` on a fresh project also
        // gets its transitive deps planned upfront.
        //
        // Entries that don't qualify (bare OCI URL references, unparseable
        // versions, etc.) are skipped here — a fallback step after the
        // concurrent batch handles their transitive deps.
        //
        // A `Db` error means dep-graph data is not yet available (e.g. the
        // meta-registry hasn't indexed deps, or sync was skipped in offline
        // mode).  We skip silently and let the fallback installer handle it.
        let mut resolved_transitive: HashMap<String, wasm_package_manager::resolver::WitVersion> =
            HashMap::new();
        let mut resolver_root_names: HashSet<String> = HashSet::new();
        if !offline {
            let mut roots = Vec::new();

            // Feed CLI inputs / manifest entries into the resolver via
            // `to_install`.  Entries with an `explicit_name` (WIT-style)
            // plus a parseable semver tag qualify as resolver roots.
            for (reference, _update, explicit_name) in &to_install {
                let Some(name) = explicit_name.as_deref() else {
                    continue;
                };
                if !looks_like_wit_name(name) {
                    continue;
                }
                let tag = reference.tag().unwrap_or_default();
                let Ok(version) = tag
                    .trim_start_matches('v')
                    .parse::<wasm_package_manager::resolver::WitVersion>()
                else {
                    continue;
                };
                roots.push((name.to_string(), version));
                resolver_root_names.insert(name.to_string());
            }

            if !roots.is_empty() {
                match manager.resolve_all_dependencies(&roots) {
                    Ok(deps) => {
                        resolved_transitive = deps;
                    }
                    Err(ResolveError::NoSolution(msg)) => {
                        return Err(InstallError::DependencyConflict(msg).into());
                    }
                    Err(ResolveError::Db(_)) => {} // dep data not yet available; skip
                }
            }

            // Remove top-level entries — they are installed as part of the
            // main install batch, not as transitive dependencies.
            for name in &resolver_root_names {
                resolved_transitive.remove(name);
            }
        }

        // `&Manager` is Copy, so each async-move block captures its own copy of
        // the reference without requiring Arc or any synchronisation primitive.
        let manager_ref: &Manager = &manager;

        // Build a unified install list: top-level packages from the manifest
        // plus transitive dependencies discovered by the resolver.  Transitive
        // entries that are already in the lockfile are skipped to avoid
        // redundant downloads.
        let existing_interface_names: HashSet<_> =
            lockfile.interfaces.iter().map(|p| p.name.clone()).collect();
        let transitive_installs: Vec<PlannedInstall> = resolved_transitive
            .into_iter()
            .filter(|(name, _)| !existing_interface_names.contains(name))
            .filter_map(|(name, version)| {
                let dep = DependencyItem {
                    package: name.clone(),
                    version: Some(version.to_string()),
                };
                resolve_dep_reference(&manager, &dep).map(|r| PlannedInstall::Transitive {
                    reference: r,
                    package_name: name,
                })
            })
            .collect();

        let all_installs: Vec<PlannedInstall> = to_install
            .into_iter()
            .map(
                |(reference, update_manifest, explicit_name)| PlannedInstall::TopLevel {
                    reference,
                    update_manifest,
                    explicit_name,
                },
            )
            .chain(transitive_installs)
            .collect();

        // Display the resolved plan and transition to the installing phase.
        // r[impl cli.progress-bar.plan-timing]
        if !offline {
            let plan_entries: Vec<(String, Option<String>)> = all_installs
                .iter()
                .map(PlannedInstall::display_info)
                .collect();
            let plan_refs: Vec<(&str, Option<&str>)> = plan_entries
                .iter()
                .map(|(n, v)| (n.as_str(), v.as_deref()))
                .collect();

            let mut d = display.lock().await;
            d.show_plan(&plan_refs);
            d.start_installing();
        }

        // Run all installs (top-level + transitive) concurrently.
        // Top-level failures are fatal; transitive failures are logged and
        // skipped to preserve the soft-failure semantics of the old
        // sequential installer.
        let results: anyhow::Result<Vec<Option<(InstallResult, PlannedInstall)>>> = all_installs
            .into_co_stream()
            .map(|entry| {
                let display = SharedDisplay::clone(&display);
                let vendor_dir = wasm_vendor_dir.clone();
                let wit_vendor_dir = wit_vendor_dir.clone();
                async move {
                    let (display_name, version) = entry.display_info();
                    let install_result = install_one(
                        manager_ref,
                        &display,
                        offline,
                        entry.reference(),
                        &vendor_dir,
                        &display_name,
                        version.as_deref(),
                    )
                    .await;

                    match install_result {
                        Ok(result) => {
                            if entry.is_transitive() {
                                if let Err(e) = re_vendor_wit_files(&result, &wit_vendor_dir).await
                                {
                                    tracing::debug!(
                                        "Failed to vendor WIT files for '{}': {e} — skipping",
                                        display_name,
                                    );
                                }
                            } else {
                                re_vendor_wit_files(&result, &wit_vendor_dir).await?;
                            }
                            anyhow::Ok(Some((result, entry)))
                        }
                        Err(e) if entry.is_transitive() => {
                            tracing::debug!(
                                "Failed to install transitive dependency '{}': {e} — skipping",
                                display_name,
                            );
                            anyhow::Ok(None)
                        }
                        Err(e) => Err(e),
                    }
                }
            })
            .collect()
            .await;

        // Process results — top-level entries update the manifest and
        // lockfile; transitive entries only update the lockfile.
        for (result, entry) in results
            .map_err(crate::util::into_miette)?
            .into_iter()
            .flatten()
        {
            match entry {
                PlannedInstall::TopLevel {
                    update_manifest,
                    explicit_name,
                    ..
                } => {
                    process_top_level_result(
                        result,
                        update_manifest,
                        explicit_name,
                        &mut manifest,
                        &mut lockfile,
                    );
                }
                PlannedInstall::Transitive { .. } => {
                    upsert_lockfile_type(&mut lockfile, &result);
                }
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

        // Display the final completion summary.
        let elapsed = start_time.elapsed();
        if offline {
            // Offline mode never starts the phased display — print a plain
            // summary line so the user still sees the result.
            println!(
                "{} Installed in {:.1}s",
                console::style("✓").green().bold(),
                elapsed.as_secs_f64()
            );
        } else {
            let mut d = display.lock().await;
            let completed_count = d.completed_count();
            d.finish_all(completed_count, elapsed);
        }

        Ok(())
    }
}

/// Shared handle to an [`InstallDisplay`] for use across concurrent tasks.
type SharedDisplay = std::sync::Arc<tokio::sync::Mutex<InstallDisplay>>;

/// A package planned for installation, tagged as either a top-level
/// manifest dependency or a transitive dependency discovered by the
/// PubGrub resolver.
enum PlannedInstall {
    /// A top-level dependency from the manifest or user input.
    TopLevel {
        reference: Reference,
        update_manifest: bool,
        explicit_name: Option<String>,
    },
    /// A transitive dependency discovered by the resolver.
    Transitive {
        reference: Reference,
        package_name: String,
    },
}

impl PlannedInstall {
    /// Returns a reference to the OCI [`Reference`] for this entry.
    fn reference(&self) -> &Reference {
        match self {
            PlannedInstall::TopLevel { reference, .. }
            | PlannedInstall::Transitive { reference, .. } => reference,
        }
    }

    /// Returns `true` when this entry is a transitive dependency.
    fn is_transitive(&self) -> bool {
        matches!(self, PlannedInstall::Transitive { .. })
    }

    /// Returns a `(display_name, version)` pair for progress display.
    fn display_info(&self) -> (String, Option<String>) {
        match self {
            PlannedInstall::TopLevel {
                reference,
                explicit_name,
                ..
            } => {
                let (name, ver) = package_display_parts(explicit_name.as_deref(), reference.tag());
                let display = if name.is_empty() {
                    oci_repo_display_name(reference.repository())
                } else {
                    name
                };
                (display, ver)
            }
            PlannedInstall::Transitive {
                reference,
                package_name,
            } => {
                let (name, ver) =
                    package_display_parts(Some(package_name.as_str()), reference.tag());
                let display = if name.is_empty() {
                    package_name.clone()
                } else {
                    name
                };
                (display, ver)
            }
        }
    }
}

/// Install a single package and report progress.
///
/// In offline mode a plain status line is printed. In online mode a
/// progress bar is created for the package showing aggregated download
/// progress across all layers.
async fn install_one(
    manager: &Manager,
    display: &SharedDisplay,
    offline: bool,
    reference: &Reference,
    vendor_dir: &std::path::Path,
    display_name: &str,
    display_version: Option<&str>,
) -> anyhow::Result<InstallResult> {
    if offline {
        // No progress bars in offline mode — print a simple status line.
        let version_str = display_version.map(|v| format!(" {v}")).unwrap_or_default();
        println!("{display_name}{version_str}");
        return manager.install(reference.clone(), vendor_dir).await;
    }

    let (progress_tx, progress_rx) = tokio::sync::mpsc::channel::<ProgressEvent>(64);

    let (pb, bar_id) = display.lock().await.add_bar(display_name, display_version);

    // Spawn progress rendering task
    let progress_handle = tokio::task::spawn(run_progress_bars(pb.clone(), progress_rx));

    let result = manager
        .install_with_progress(reference.clone(), vendor_dir, &progress_tx)
        .await;

    // Drop the sender to signal the progress task to finish
    drop(progress_tx);

    // Wait for progress bars to finish rendering
    let _ = progress_handle.await;

    // Only mark the bar as complete (green checkmark) on successful installs.
    if result.is_ok() {
        display.lock().await.finish_bar(bar_id);
    }

    result
}

/// Process a top-level install result: update the manifest (if requested)
/// and upsert the lockfile entry.  Returns the result's dependency list
/// so the caller can handle any unplanned transitive deps.
fn process_top_level_result(
    result: InstallResult,
    update_manifest: bool,
    explicit_name: Option<String>,
    manifest: &mut wasm_manifest::Manifest,
    lockfile: &mut wasm_manifest::Lockfile,
) -> Vec<DependencyItem> {
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
        let existing_names: HashSet<String> = manifest
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
        lockfile,
        result.is_component,
        &dep_name,
        &registry_path,
        package,
    );

    result.dependencies
}

#[cfg(test)]
mod tests {
    use wasm_package_manager::manager::InstallResult;
    use wasm_package_manager::manager::install::{looks_like_wit_name, re_vendor_wit_files};

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
