use anyhow::Result;
use wasm_package_manager::{InsertResult, Manager, Reference};

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
                        let is_sig = tag.ends_with(".sig");
                        let is_att = tag.ends_with(".att");

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
                        println!("  {}", tag);
                    }
                }
                Ok(())
            }
            Opts::Search(opts) => opts.run(offline).await,
            Opts::Sync(opts) => opts.run().await,
        }
    }
}
