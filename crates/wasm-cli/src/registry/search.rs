//! `wasm registry search` subcommand.

use anyhow::Result;
use comfy_table::{ContentArrangement, Table};
use wasm_package_manager::manager::{Manager, SyncPolicy, SyncResult};

/// Default meta-registry URL.
const REGISTRY_URL: &str = "http://localhost:8080";

/// Default sync interval in seconds (1 hour).
const SYNC_INTERVAL: u64 = 3600;

/// Search for packages across configured registries.
#[derive(clap::Args)]
pub(crate) struct SearchOpts {
    /// Search query (matches package name and description).
    #[arg(required_unless_present_any = ["exports", "imports"])]
    query: Option<String>,

    /// Filter to packages that export a given interface (e.g. wasi:http).
    #[arg(long, conflicts_with = "imports")]
    exports: Option<String>,

    /// Filter to packages that import a given interface (e.g. wasi:http).
    #[arg(long, conflicts_with = "exports")]
    imports: Option<String>,

    /// Maximum number of results to show.
    #[arg(long, default_value = "20")]
    limit: u32,
}

impl SearchOpts {
    pub(crate) async fn run(self, offline: bool) -> Result<()> {
        let manager = if offline {
            Manager::open_offline().await?
        } else {
            Manager::open().await?
        };

        // Attempt to sync from meta-registry if not offline.
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

        let packages = match (&self.exports, &self.imports) {
            (Some(iface), _) => manager.search_packages_by_export(iface, 0, self.limit)?,
            (_, Some(iface)) => manager.search_packages_by_import(iface, 0, self.limit)?,
            _ => {
                let query = self.query.as_deref().unwrap_or_default();
                manager.search_packages(query, 0, self.limit)?
            }
        };

        if packages.is_empty() {
            let message = match (&self.exports, &self.imports) {
                (Some(iface), _) => format!("No packages found exporting '{iface}'"),
                (_, Some(iface)) => format!("No packages found importing '{iface}'"),
                _ => format!(
                    "No packages found matching '{}'",
                    self.query.as_deref().unwrap_or_default()
                ),
            };
            println!("{message}");
            return Ok(());
        }

        println!("{}", render_search_table(&packages));
        Ok(())
    }
}

/// Render a list of [`KnownPackage`]s as a `comfy-table` table string.
///
/// Extracted for testability — the CLI calls this via `SearchOpts::run`,
/// but unit tests can call it directly without a database.
#[must_use]
pub(crate) fn render_search_table(
    packages: &[wasm_package_manager::storage::KnownPackage],
) -> String {
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["PACKAGE", "DESCRIPTION", "TAGS"]);

    for pkg in packages {
        let reference = pkg.reference();
        let description = pkg.description.as_deref().unwrap_or("-");
        let tags = if pkg.tags.is_empty() {
            "-".to_string()
        } else {
            pkg.tags.join(", ")
        };
        table.add_row(vec![&reference, description, &tags]);
    }

    table.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_package_manager::storage::KnownPackage;

    #[test]
    fn test_render_search_table_with_results() {
        let packages = vec![
            KnownPackage {
                registry: "ghcr.io".into(),
                repository: "example/http-server".into(),
                description: Some("A simple HTTP server component".into()),
                tags: vec!["0.1.0".into(), "0.2.0".into()],
                signature_tags: vec![],
                attestation_tags: vec![],
                last_seen_at: "2025-01-01 00:00:00".into(),
                created_at: "2025-01-01 00:00:00".into(),
            },
            KnownPackage {
                registry: "ghcr.io".into(),
                repository: "example/logger".into(),
                description: None,
                tags: vec![],
                signature_tags: vec![],
                attestation_tags: vec![],
                last_seen_at: "2025-01-01 00:00:00".into(),
                created_at: "2025-01-01 00:00:00".into(),
            },
        ];

        let output = render_search_table(&packages);

        // Header row
        assert!(output.contains("PACKAGE"));
        assert!(output.contains("DESCRIPTION"));
        assert!(output.contains("TAGS"));

        // First package
        assert!(output.contains("ghcr.io/example/http-server"));
        assert!(output.contains("A simple HTTP server component"));
        assert!(output.contains("0.1.0, 0.2.0"));

        // Second package (no description / no tags → dashes)
        assert!(output.contains("ghcr.io/example/logger"));
    }

    #[test]
    fn test_render_search_table_empty() {
        let output = render_search_table(&[]);
        assert!(output.contains("PACKAGE"));
        // Table has headers but no data rows
        assert!(!output.contains("ghcr.io"));
    }
}
