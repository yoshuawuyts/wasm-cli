use std::sync::Arc;

use anyhow::{Context, Result};
use futures_concurrency::prelude::*;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use wasm_package_manager::{Manager, ProgressEvent, Reference};

use crate::util::write_lock_file;

/// Options for the `install` command.
#[derive(clap::Parser)]
pub(crate) struct Opts {
    /// The OCI references to install (e.g., ghcr.io/webassembly/wasi-logging:1.0.0 or oci://ghcr.io/webassembly/wasi-logging:1.0.0).
    /// If no references are provided, installs all packages listed in the manifest.
    #[arg(value_parser = crate::util::parse_reference, value_name = "REFERENCE", num_args = 0..)]
    references: Vec<Reference>,
}

impl Opts {
    pub(crate) async fn run(self, offline: bool) -> Result<()> {
        let deps = std::path::Path::new("deps");
        let manifest_path = deps.join("wasm.toml");
        let lockfile_path = deps.join("wasm.lock.toml");
        let wasm_vendor_dir = deps.join("vendor/wasm");
        let wit_vendor_dir = deps.join("vendor/wit");

        // Read existing manifest — error if not found, recommend `wasm init`
        let manifest_str = tokio::fs::read_to_string(&manifest_path)
            .await
            .with_context(|| {
                format!(
                    "could not read '{}'. Run `wasm init` first to create the project files",
                    manifest_path.display()
                )
            })?;
        let mut manifest: wasm_manifest::Manifest = toml::from_str(&manifest_str)?;

        // Read existing lockfile — error if not found, recommend `wasm init`
        let lockfile_str = tokio::fs::read_to_string(&lockfile_path)
            .await
            .with_context(|| {
                format!(
                    "could not read '{}'. Run `wasm init` first to create the project files",
                    lockfile_path.display()
                )
            })?;
        let mut lockfile: wasm_manifest::Lockfile = toml::from_str(&lockfile_str)?;

        // Open manager
        let manager = Arc::new(if offline {
            Manager::open_offline().await?
        } else {
            Manager::open().await?
        });

        let start_time = std::time::Instant::now();

        // Determine the list of (reference, update_manifest) pairs to install.
        // When no references are provided, install everything from the manifest
        // and skip re-adding those entries to the manifest. When references are
        // provided explicitly, add each one to the manifest after installing.
        let to_install: Vec<(Reference, bool)> = if self.references.is_empty() {
            manifest
                .all_dependencies()
                .map(|(_, dep, _)| reference_from_dependency(dep).map(|r| (r, false)))
                .collect::<Result<Vec<_>>>()?
        } else {
            self.references.into_iter().map(|r| (r, true)).collect()
        };

        // Shared progress display for all concurrent installs.
        let multi = MultiProgress::new();

        // Run all installs concurrently.
        let results: Result<Vec<_>> = to_install
            .into_co_stream()
            .map(|(reference, update_manifest)| {
                let manager = manager.clone();
                let multi = multi.clone();
                let vendor_dir = wasm_vendor_dir.clone();
                let wit_vendor_dir = wit_vendor_dir.clone();
                async move {
                    let result =
                        install_one(&manager, multi, offline, &reference, &vendor_dir).await?;
                    re_vendor_wit_files(&result, &wit_vendor_dir).await?;
                    anyhow::Ok((result, reference, update_manifest))
                }
            })
            .collect()
            .await;

        for (result, reference, update_manifest) in results? {
            // Use the package name from WIT metadata if available,
            // otherwise fall back to the full OCI path (registry/repository).
            // Strip the version suffix (e.g., "@0.2.10") from the package name
            // so that "wasi:http@0.2.10" becomes "wasi:http" in wasm.toml.
            let dep_name = result
                .package_name
                .as_deref()
                .map(|name| name.split('@').next().unwrap_or(name).to_string())
                .unwrap_or_else(|| format!("{}/{}", result.registry, result.repository));

            // Determine the version from the tag
            let version = result.tag.clone().unwrap_or_default();

            // Add to manifest (compact format) — route to components or interfaces.
            // Only update the manifest when a reference was explicitly provided;
            // for the 0-args case the entries are already in the manifest.
            if update_manifest {
                let reference_str = reference.whole().to_string();
                let dep = wasm_manifest::Dependency::Compact(reference_str);
                if result.is_component {
                    manifest.components.insert(dep_name.clone(), dep);
                } else {
                    manifest.interfaces.insert(dep_name.clone(), dep);
                }
            }

            // Add to lockfile — route to components or interfaces
            let registry_path = format!("{}/{}", result.registry, result.repository);
            let digest = result.digest.unwrap_or_default();

            let package = wasm_manifest::Package {
                name: dep_name.clone(),
                version,
                registry: registry_path.clone(),
                digest,
                dependencies: vec![],
            };

            if result.is_component {
                let existing = lockfile
                    .components
                    .iter()
                    .position(|p| p.name == dep_name && p.registry == registry_path);
                if let Some(existing_pkg) =
                    existing.and_then(|idx| lockfile.components.get_mut(idx))
                {
                    *existing_pkg = package;
                } else {
                    lockfile.components.push(package);
                }
            } else {
                let existing = lockfile
                    .interfaces
                    .iter()
                    .position(|p| p.name == dep_name && p.registry == registry_path);
                if let Some(existing_pkg) =
                    existing.and_then(|idx| lockfile.interfaces.get_mut(idx))
                {
                    *existing_pkg = package;
                } else {
                    lockfile.interfaces.push(package);
                }
            }
        }

        // Write updated manifest
        let manifest_str = toml::to_string_pretty(&manifest)?;
        tokio::fs::write(&manifest_path, manifest_str.as_bytes()).await?;

        // Write updated lockfile
        write_lock_file(&lockfile_path, &lockfile).await?;

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
) -> Result<wasm_package_manager::InstallResult> {
    let reference_display = reference.whole().to_string();

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
/// WIT-only packages (interfaces) are initially stored alongside components in
/// `deps/vendor/wasm/`. This function moves them to `deps/vendor/wit/` so that
/// WIT tooling can find them at the conventional location.
async fn re_vendor_wit_files(
    result: &wasm_package_manager::InstallResult,
    wit_vendor_dir: &std::path::Path,
) -> Result<()> {
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

/// Convert a manifest [`wasm_manifest::Dependency`] into an OCI [`Reference`].
///
/// Both the compact string format (`"ghcr.io/webassembly/wasi-logging:1.0.0"`) and
/// the explicit table format (`registry`/`namespace`/`package`:`version`) are
/// supported. Returns an error if the resulting reference string cannot be parsed
/// as a valid OCI reference.
fn reference_from_dependency(dep: &wasm_manifest::Dependency) -> Result<Reference> {
    let s = match dep {
        wasm_manifest::Dependency::Compact(s) => s.clone(),
        wasm_manifest::Dependency::Explicit {
            registry,
            namespace,
            package,
            version,
        } => format!("{registry}/{namespace}/{package}:{version}"),
    };
    crate::util::parse_reference(&s).map_err(|e| anyhow::anyhow!("{e}"))
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

                let pb = match total_bytes {
                    Some(total) => {
                        let pb = multi.add(ProgressBar::new(total));
                        pb.set_style(bar_style_progress.clone());
                        pb
                    }
                    None => {
                        let pb = multi.add(ProgressBar::new_spinner());
                        pb.set_style(bar_style_spinner.clone());
                        pb
                    }
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
