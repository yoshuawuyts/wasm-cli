#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::Result;
use comfy_table::{ContentArrangement, Table};
use wasm_package_manager::manager::Manager;
use wasm_package_manager::oci::{ImageEntry, InsertResult};
use wasm_package_manager::{Reference, format_size};

mod inspect;
mod search;
mod sync;

/// Manage Wasm Components and WIT interfaces in OCI registries
#[derive(clap::Parser)]
pub(crate) enum Opts {
    /// Fetch OCI metadata for a component
    Show,
    /// Pull a component from the registry
    Pull(PullOpts),
    Push,
    /// List all available tags for a component
    Tags(TagsOpts),
    /// Search for packages across configured registries
    Search(search::SearchOpts),
    /// Force-sync the package index from the configured meta-registry
    Sync(sync::SyncOpts),
    /// Delete a package from the local store
    Delete(DeleteOpts),
    /// List all installed packages
    List(ListOpts),
    /// List all known packages (previously synced or pulled)
    Known(KnownOpts),
    /// Inspect the metadata of a package on the registry
    Inspect(inspect::InspectOpts),
}

#[derive(clap::Args)]
pub(crate) struct PullOpts {
    /// The reference to pull
    #[arg(value_parser = crate::util::parse_reference)]
    reference: Reference,
}

#[derive(clap::Args)]
pub(crate) struct TagsOpts {
    /// The reference to list tags for (e.g., ghcr.io/example/component or oci://ghcr.io/example/component)
    #[arg(value_parser = crate::util::parse_reference)]
    reference: Reference,
    /// Include signature tags (ending in .sig)
    #[arg(long)]
    signatures: bool,
    /// Include attestation tags (ending in .att)
    #[arg(long)]
    attestations: bool,
}

#[derive(clap::Args)]
pub(crate) struct DeleteOpts {
    /// The reference to delete (e.g., ghcr.io/example/component:tag)
    #[arg(value_parser = crate::util::parse_reference)]
    reference: Reference,
}

#[derive(clap::Args)]
pub(crate) struct ListOpts {}

#[derive(clap::Args)]
pub(crate) struct KnownOpts {
    /// Maximum number of results to show
    #[arg(long, default_value = "100")]
    limit: u32,
}

impl Opts {
    pub(crate) async fn run(self, offline: bool) -> Result<()> {
        let store = if offline {
            Manager::open_offline().await?
        } else {
            Manager::open().await?
        };
        match self {
            Opts::Show => todo!(),
            Opts::Pull(opts) => {
                let result = store.pull(opts.reference.clone()).await?;
                if result.insert_result == InsertResult::AlreadyExists {
                    tracing::warn!(
                        "package '{}' already exists in the local store",
                        opts.reference.whole()
                    );
                }
                Ok(())
            }
            Opts::Push => todo!(),
            Opts::Tags(opts) => {
                let all_tags = store.list_tags(&opts.reference).await?;

                // Filter tags based on flags
                let tags: Vec<_> = all_tags
                    .into_iter()
                    .filter(|tag| {
                        use std::ffi::OsStr;
                        let ext = std::path::Path::new(tag.as_str()).extension();
                        let is_sig = ext == Some(OsStr::new("sig"));
                        let is_att = ext == Some(OsStr::new("att"));

                        if is_sig {
                            opts.signatures
                        } else if is_att {
                            opts.attestations
                        } else {
                            true // Always include release tags
                        }
                    })
                    .collect();

                if tags.is_empty() {
                    if offline {
                        println!(
                            "No cached tags found for '{}' (offline mode)",
                            opts.reference.whole()
                        );
                    } else {
                        println!("No tags found for '{}'", opts.reference.whole());
                    }
                } else {
                    if offline {
                        println!(
                            "Cached tags for '{}' (offline mode):",
                            opts.reference.whole()
                        );
                    } else {
                        println!("Tags for '{}':", opts.reference.whole());
                    }
                    for tag in tags {
                        println!("  {tag}");
                    }
                }
                Ok(())
            }
            Opts::Search(opts) => opts.run(offline).await,
            Opts::Sync(opts) => opts.run().await,
            Opts::Delete(opts) => {
                let deleted = store.delete(opts.reference.clone()).await?;
                if deleted {
                    println!("Deleted '{}'", opts.reference.whole());
                } else {
                    println!(
                        "Package '{}' not found in local store",
                        opts.reference.whole()
                    );
                }
                Ok(())
            }
            Opts::List(_opts) => {
                let images = store.list_all()?;
                if images.is_empty() {
                    println!("No installed packages");
                } else {
                    println!("{}", render_list_table(&images));
                }
                Ok(())
            }
            Opts::Known(opts) => {
                let packages = store.list_known_packages(0, opts.limit)?;
                if packages.is_empty() {
                    println!("No known packages");
                } else {
                    println!("{}", search::render_search_table(&packages));
                }
                Ok(())
            }
            Opts::Inspect(opts) => opts.run(&store).await,
        }
    }
}

/// Render a list of [`ImageEntry`]s as a `comfy-table` table string.
///
/// Extracted for testability — the CLI calls this via `Opts::run`,
/// but unit tests can call it directly without a database.
#[must_use]
fn render_list_table(images: &[ImageEntry]) -> String {
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["PACKAGE", "TAG", "SIZE"]);

    for image in images {
        let reference = format!("{}/{}", image.ref_registry, image.ref_repository);
        let tag = image.ref_tag.as_deref().unwrap_or("-");
        let size = format_size(image.size_on_disk);
        table.add_row(vec![&reference, tag, &size]);
    }

    table.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oci_client::manifest::OciImageManifest;

    #[test]
    fn test_render_list_table_with_images() {
        let images = vec![
            ImageEntry {
                ref_registry: "ghcr.io".into(),
                ref_repository: "example/http-server".into(),
                ref_mirror_registry: None,
                ref_tag: Some("0.1.0".into()),
                ref_digest: None,
                manifest: OciImageManifest::default(),
                size_on_disk: 1024 * 1024, // 1 MB
            },
            ImageEntry {
                ref_registry: "ghcr.io".into(),
                ref_repository: "example/logger".into(),
                ref_mirror_registry: None,
                ref_tag: None,
                ref_digest: Some("sha256:abc123".into()),
                manifest: OciImageManifest::default(),
                size_on_disk: 512,
            },
        ];

        let output = render_list_table(&images);

        // Header row
        assert!(output.contains("PACKAGE"));
        assert!(output.contains("TAG"));
        assert!(output.contains("SIZE"));

        // First image
        assert!(output.contains("ghcr.io/example/http-server"));
        assert!(output.contains("0.1.0"));
        assert!(output.contains("1.00 MB"));

        // Second image (no tag → dash)
        assert!(output.contains("ghcr.io/example/logger"));
        assert!(output.contains("512 B"));
    }

    #[test]
    fn test_render_list_table_empty() {
        let output = render_list_table(&[]);
        assert!(output.contains("PACKAGE"));
        // Table has headers but no data rows
        assert!(!output.contains("ghcr.io"));
    }
}
