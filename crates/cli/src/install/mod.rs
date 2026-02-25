use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use wasm_package_manager::{Manager, ProgressEvent, Reference};

use crate::util::write_lock_file;

/// Options for the `install` command.
#[derive(clap::Parser)]
pub(crate) struct Opts {
    /// The OCI reference to install (e.g., ghcr.io/webassembly/wasi-logging:1.0.0)
    reference: Reference,
}

impl Opts {
    pub(crate) async fn run(self, offline: bool) -> Result<()> {
        let deps = std::path::Path::new("deps");
        let manifest_path = deps.join("wasm.toml");
        let lockfile_path = deps.join("wasm.lock.toml");
        let vendor_dir = deps.join("vendor/wasm");

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
        let manager = if offline {
            Manager::open_offline().await?
        } else {
            Manager::open().await?
        };

        // Install the package with progress reporting
        let result = if offline {
            // No progress bars in offline mode
            manager.install(self.reference.clone(), &vendor_dir).await?
        } else {
            let (progress_tx, progress_rx) = tokio::sync::mpsc::channel::<ProgressEvent>(64);
            let multi = MultiProgress::new();
            let reference_str = self.reference.whole().to_string();

            // Spawn progress rendering task
            let progress_handle =
                tokio::task::spawn(run_progress_bars(multi, progress_rx, reference_str));

            let result = manager
                .install_with_progress(self.reference.clone(), &vendor_dir, &progress_tx)
                .await;

            // Drop the sender to signal the progress task to finish
            drop(progress_tx);

            // Wait for progress bars to finish rendering
            let _ = progress_handle.await;

            result?
        };

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

        // Add to manifest dependencies (compact format)
        let reference_str = self.reference.whole().to_string();
        manifest.dependencies.insert(
            dep_name.clone(),
            wasm_manifest::Dependency::Compact(reference_str),
        );

        // Add to lockfile packages
        let registry_path = format!("{}/{}", result.registry, result.repository);
        let digest = result.digest.unwrap_or_default();

        // Check if package already exists in lockfile
        let existing = lockfile
            .packages
            .iter()
            .position(|p| p.name == dep_name && p.registry == registry_path);
        let package = wasm_manifest::Package {
            name: dep_name.clone(),
            version,
            registry: registry_path,
            digest,
            dependencies: vec![],
        };
        if let Some(existing_pkg) = existing.and_then(|idx| lockfile.packages.get_mut(idx)) {
            *existing_pkg = package;
        } else {
            lockfile.packages.push(package);
        }

        // Write updated manifest
        let manifest_str = toml::to_string_pretty(&manifest)?;
        tokio::fs::write(&manifest_path, manifest_str.as_bytes()).await?;

        // Write updated lockfile
        write_lock_file(&lockfile_path, &lockfile).await?;

        // Print success message
        let vendored: Vec<_> = result
            .vendored_files
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        if vendored.is_empty() {
            println!("Installed '{}'", self.reference.whole());
        } else {
            println!(
                "Installed '{}' -> {}",
                self.reference.whole(),
                vendored.join(", ")
            );
        }

        Ok(())
    }
}

/// Consume progress events and render tree-style multi-progress bars.
async fn run_progress_bars(
    multi: MultiProgress,
    mut rx: tokio::sync::mpsc::Receiver<ProgressEvent>,
    reference: String,
) {
    let mut bars: Vec<ProgressBar> = Vec::new();
    let mut layer_count: usize = 0;

    // In-progress style: yellow tree prefix + bar + bytes + eta
    let bar_style_progress = ProgressStyle::with_template(
        "{prefix:.yellow} {bar:12.yellow} {bytes}/{total_bytes} {eta}",
    )
    .expect("valid progress bar template")
    .progress_chars("━━┄");

    // In-progress spinner style (unknown size)
    let bar_style_spinner =
        ProgressStyle::with_template("{prefix:.yellow} {spinner:.yellow} {bytes}")
            .expect("valid progress bar template");

    // Completed style: green tree prefix + checkmark + total bytes
    let bar_style_done = ProgressStyle::with_template("{prefix:.green} ✓ {bytes:.green}")
        .expect("valid progress bar template");

    while let Some(event) = rx.recv().await {
        match event {
            ProgressEvent::ManifestFetched {
                layer_count: count,
                ref image_digest,
            } => {
                layer_count = count;
                let short_digest = image_digest
                    .strip_prefix("sha256:")
                    .unwrap_or(image_digest)
                    .get(..5)
                    .unwrap_or(image_digest);
                let _ = multi.println(format!("{reference} [{short_digest}]"));
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
                let prefix = format!("{tree_glyph} [{short_digest}] {label}");

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
