use anyhow::Context;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use super::config::StateInfo;
use super::known_package::KnownPackageParams;
use super::models::{Migrations, RawKnownPackage};
use crate::components::{ComponentTarget, WasmComponent};
use crate::oci::{
    InsertResult, OciLayer, OciLayerAnnotation, OciManifest, OciReferrer, OciRepository, OciTag,
    RawImageEntry,
};
use crate::types::{
    RawWitPackage, WitPackageDependency, WitWorld, WitWorldExport, WitWorldImport,
    extract_wit_metadata,
};
use futures_concurrency::prelude::*;
use oci_client::{Reference, client::ImageData, manifest::OciImageManifest};
use rusqlite::{Connection, OptionalExtension};

/// Calculate the total size of a directory recursively
async fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                total += metadata.len();
            }
        }
    }
    total
}

#[derive(Debug)]
pub(crate) struct Store {
    pub(crate) state_info: StateInfo,
    conn: Connection,
}

/// A raw row from the `oci_manifest` table, used as an intermediate
/// representation in [`Store::get_package_versions`].
///
/// This struct exists solely to avoid a 15-element tuple when collecting
/// query results (clippy's `type_complexity` lint). Each field maps 1:1 to
/// a column in `oci_manifest`. After collection, the loop enriches every
/// row with tags, worlds, components, dependencies, and referrers to
/// produce the final [`PackageVersion`] API type.
///
/// The `oci_*` fields correspond to the [OCI image annotation keys][spec].
///
/// [spec]: https://github.com/opencontainers/image-spec/blob/main/annotations.md
struct ManifestRow {
    /// Primary key (`oci_manifest.id`), used to join against child tables.
    id: i64,
    /// Content-addressable digest of the manifest (e.g. `sha256:abc…`).
    digest: String,
    /// Total size in bytes, if known.
    size_bytes: Option<i64>,
    /// ISO-8601 timestamp when the row was first inserted into the local DB.
    synced_at: String,
    // — OCI annotation columns ——————————————————————————
    oci_created: Option<String>,
    oci_authors: Option<String>,
    oci_url: Option<String>,
    oci_documentation: Option<String>,
    oci_source: Option<String>,
    oci_version: Option<String>,
    oci_revision: Option<String>,
    oci_vendor: Option<String>,
    oci_licenses: Option<String>,
    oci_title: Option<String>,
    oci_description: Option<String>,
}

impl Store {
    /// Open the store and run any pending migrations.
    pub(crate) async fn open() -> anyhow::Result<Self> {
        let data_dir = dirs::data_local_dir()
            .context("No local data dir known for the current OS")?
            .join("wasm");
        let config_file = crate::xdg_config_home()
            .context("Could not determine config directory (set $XDG_CONFIG_HOME or $HOME)")?
            .join("wasm")
            .join("config.toml");
        Self::open_inner(data_dir, config_file).await
    }

    /// Open the store at a custom data directory and run any pending migrations.
    pub(crate) async fn open_at(data_dir: impl Into<std::path::PathBuf>) -> anyhow::Result<Self> {
        let data_dir = data_dir.into();
        let config_file = data_dir.join("config.toml");
        Self::open_inner(data_dir, config_file).await
    }

    /// Shared implementation for opening a store at a given location.
    async fn open_inner(
        data_dir: std::path::PathBuf,
        config_file: std::path::PathBuf,
    ) -> anyhow::Result<Self> {
        let store_dir = data_dir.join("store");
        let db_dir = data_dir.join("db");
        let metadata_file = db_dir.join("metadata.db3");

        let a = tokio::fs::create_dir_all(&data_dir);
        let b = tokio::fs::create_dir_all(&store_dir);
        let c = tokio::fs::create_dir_all(&db_dir);
        let _ = (a, b, c)
            .try_join()
            .await
            .context("Could not create config directories on disk")?;

        let conn = Connection::open(&metadata_file)?;

        // Configure SQLite for better concurrency, data integrity, and performance
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )?;

        Migrations::run_all(&conn)?;

        let migration_info = Migrations::get(&conn);
        let store_size = dir_size(&store_dir).await;
        let metadata_size = tokio::fs::metadata(&metadata_file)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let state_info = StateInfo::new_at(
            data_dir,
            config_file,
            &migration_info,
            store_size,
            metadata_size,
        );

        Ok(Self { state_info, conn })
    }

    /// Create a Store directly from an in-memory SQLite connection.
    ///
    /// The connection MUST already have all migrations applied. The `StateInfo`
    /// created here has dummy paths and is only suitable for unit tests that do
    /// not exercise the content-addressable layer cache.
    #[cfg(test)]
    pub(crate) fn from_conn(conn: Connection) -> Self {
        // Use a per-call unique temp directory so concurrent tests cannot
        // interfere with each other if any code path writes to state_info paths.
        let tmp = tempfile::tempdir()
            .expect("failed to create temp dir for test Store")
            .keep();
        let migration_info = Migrations {
            current: 0,
            total: 0,
        };
        let state_info = StateInfo::new_at(
            tmp.join("store"),
            tmp.join("config.toml"),
            &migration_info,
            0,
            0,
        );
        Self { state_info, conn }
    }

    pub(crate) async fn insert(
        &self,
        reference: &Reference,
        image: ImageData,
    ) -> anyhow::Result<(
        InsertResult,
        Option<String>,
        Option<OciImageManifest>,
        Option<i64>,
    )> {
        let digest = reference.digest().map(str::to_owned).or(image.digest);
        let manifest_str = serde_json::to_string(&image.manifest)?;

        // Calculate total size on disk from all layers
        let size_on_disk: u64 = image
            .layers
            .iter()
            .map(|l| u64::try_from(l.data.len()).unwrap_or(u64::MAX))
            .sum();

        // 1. Upsert oci_repository
        let repo_id =
            OciRepository::upsert(&self.conn, reference.registry(), reference.repository())?;

        // 2. Extract annotations from the manifest (convert BTreeMap → HashMap)
        let annotations: HashMap<String, String> = image
            .manifest
            .as_ref()
            .and_then(|m| m.annotations.clone())
            .unwrap_or_default()
            .into_iter()
            .collect();

        // 3. Upsert manifest (atomic insert-or-find)
        let (manifest_id, was_inserted) = OciManifest::upsert(
            &self.conn,
            repo_id,
            digest.as_deref().unwrap_or("unknown"),
            image
                .manifest
                .as_ref()
                .and_then(|m| m.media_type.as_deref()),
            Some(&manifest_str),
            Some(i64::try_from(size_on_disk).unwrap_or(i64::MAX)),
            image
                .manifest
                .as_ref()
                .and_then(|m| m.artifact_type.as_deref()),
            image
                .manifest
                .as_ref()
                .map(|m| m.config.media_type.as_str()),
            image.manifest.as_ref().map(|m| m.config.digest.as_str()),
            &annotations,
        )?;

        let result = if was_inserted {
            InsertResult::Inserted
        } else {
            InsertResult::AlreadyExists
        };

        // 4. Upsert tag if present
        if let Some(tag) = reference.tag()
            && let Some(ref d) = digest
        {
            OciTag::upsert(&self.conn, repo_id, tag, d)?;
        }

        let manifest = image.manifest.clone();

        // Store layers when the manifest is newly inserted, or when it was a
        // placeholder (e.g. from referrer discovery) that has no layers yet.
        let needs_layers = was_inserted || {
            let layer_count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM oci_layer WHERE oci_manifest_id = ?1",
                [manifest_id],
                |row| row.get(0),
            )?;
            layer_count == 0
        };

        if needs_layers && let Some(ref manifest) = image.manifest {
            for (idx, layer) in image.layers.iter().enumerate() {
                let cache = self.state_info.store_dir();
                let fallback_key = reference.whole().clone();
                let layer_digest = manifest
                    .layers
                    .get(idx)
                    .map_or(fallback_key.as_str(), |l| l.digest.as_str());
                let layer_media_type = manifest.layers.get(idx).map(|l| l.media_type.as_str());
                let layer_size = manifest.layers.get(idx).map(|l| l.size);
                let data = &layer.data;
                let _integrity = cacache::write(&cache, layer_digest, data).await?;

                // Record the layer in oci_layer
                let layer_id = OciLayer::insert(
                    &self.conn,
                    manifest_id,
                    layer_digest,
                    layer_media_type,
                    layer_size.map(|s| s.max(0)),
                    i32::try_from(idx).unwrap_or(i32::MAX),
                )?;

                // Store layer-level annotations
                if let Some(descriptor) = manifest.layers.get(idx)
                    && let Some(ref annotations) = descriptor.annotations
                {
                    for (key, value) in annotations {
                        if let Err(e) = OciLayerAnnotation::insert(&self.conn, layer_id, key, value)
                        {
                            tracing::warn!("Failed to insert layer annotation '{}': {}", key, e);
                        }
                    }
                }

                self.try_extract_wit_package(manifest_id, Some(layer_id), data);
            }
        }
        let manifest_id_opt = if result == InsertResult::Inserted {
            Some(manifest_id)
        } else {
            None
        };
        Ok((result, digest, manifest, manifest_id_opt))
    }

    /// Insert only the metadata (SQLite entry) for an image, without storing layers.
    ///
    /// Returns the insert result and the optional manifest ID.
    pub(crate) fn insert_metadata(
        &self,
        reference: &Reference,
        digest: Option<&str>,
        manifest: &OciImageManifest,
        size_on_disk: u64,
    ) -> anyhow::Result<(InsertResult, Option<i64>)> {
        let manifest_str = serde_json::to_string(manifest)?;

        let repo_id =
            OciRepository::upsert(&self.conn, reference.registry(), reference.repository())?;

        let annotations: HashMap<String, String> = manifest
            .annotations
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect();

        // Atomic upsert — insert or find existing
        let (manifest_id, was_inserted) = OciManifest::upsert(
            &self.conn,
            repo_id,
            digest.unwrap_or("unknown"),
            manifest.media_type.as_deref(),
            Some(&manifest_str),
            Some(i64::try_from(size_on_disk).unwrap_or(i64::MAX)),
            manifest.artifact_type.as_deref(),
            Some(manifest.config.media_type.as_str()),
            Some(manifest.config.digest.as_str()),
            &annotations,
        )?;

        let result = if was_inserted {
            InsertResult::Inserted
        } else {
            InsertResult::AlreadyExists
        };

        // Upsert tag if present
        if let Some(tag) = reference.tag()
            && let Some(d) = digest
        {
            OciTag::upsert(&self.conn, repo_id, tag, d)?;
        }

        if result == InsertResult::Inserted {
            Ok((result, Some(manifest_id)))
        } else {
            Ok((result, None))
        }
    }

    /// Insert a single layer into the content-addressable store.
    ///
    /// Optionally records the layer in `oci_layer` and extracts WIT package
    /// metadata if a `manifest_id` is provided. The `position` specifies the
    /// layer's ordering within the manifest (0-based index). If
    /// `layer_annotations` is provided, each key-value pair is stored in the
    /// `oci_layer_annotation` table.
    pub(crate) async fn insert_layer(
        &self,
        layer_digest: &str,
        data: &[u8],
        manifest_id: Option<i64>,
        media_type: Option<&str>,
        position: i32,
        layer_annotations: Option<&BTreeMap<String, String>>,
    ) -> anyhow::Result<()> {
        let cache = self.state_info.store_dir();
        let _integrity = cacache::write(&cache, layer_digest, data).await?;

        let Some(manifest_id) = manifest_id else {
            return Ok(());
        };

        let layer_id = OciLayer::insert(
            &self.conn,
            manifest_id,
            layer_digest,
            media_type,
            Some(i64::try_from(data.len()).unwrap_or(i64::MAX)),
            position,
        )?;

        // Store layer-level annotations
        if let Some(annotations) = layer_annotations {
            for (key, value) in annotations {
                if let Err(e) = OciLayerAnnotation::insert(&self.conn, layer_id, key, value) {
                    tracing::warn!("Failed to insert layer annotation '{}': {}", key, e);
                }
            }
        }

        self.try_extract_wit_package(manifest_id, Some(layer_id), data);

        Ok(())
    }

    /// Attempt to extract WIT package from wasm component bytes.
    /// This is best-effort - if extraction fails, we log a warning and skip.
    fn try_extract_wit_package(
        &self,
        manifest_id: i64,
        layer_id: Option<i64>,
        wasm_bytes: &[u8],
    ) -> bool {
        let Some(metadata) = extract_wit_metadata(wasm_bytes) else {
            return false; // Not a valid wasm component, skip
        };

        // Insert the WIT package (best-effort; skip if no package name)
        let Some(raw_name) = metadata.package_name.as_deref() else {
            return false;
        };

        // Split "namespace:name@version" into (package_name, version).
        let (package_name, version) = split_package_version(raw_name);

        let wit_package_id = match RawWitPackage::insert(
            &self.conn,
            package_name,
            version,
            None,
            Some(&metadata.wit_text),
            Some(manifest_id),
            layer_id,
        ) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(
                    "Failed to insert WIT package for manifest {}: {}",
                    manifest_id,
                    e
                );
                return false;
            }
        };

        // Insert worlds, imports, and exports; collect world IDs for component targets
        let mut world_ids: HashMap<String, i64> = HashMap::new();
        for world in &metadata.worlds {
            let wit_world_id = match WitWorld::insert(&self.conn, wit_package_id, &world.name, None)
            {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!("Failed to insert WIT world '{}': {}", world.name, e);
                    continue;
                }
            };
            world_ids.insert(world.name.clone(), wit_world_id);

            for item in &world.imports {
                if let Err(e) = WitWorldImport::insert(
                    &self.conn,
                    wit_world_id,
                    &item.package,
                    item.interface.as_deref(),
                    item.version.as_deref(),
                    None,
                ) {
                    tracing::warn!("Failed to insert WIT world import: {}", e);
                }
            }

            for item in &world.exports {
                if let Err(e) = WitWorldExport::insert(
                    &self.conn,
                    wit_world_id,
                    &item.package,
                    item.interface.as_deref(),
                    item.version.as_deref(),
                    None,
                ) {
                    tracing::warn!("Failed to insert WIT world export: {}", e);
                }
            }
        }

        // Insert type dependencies
        for dep in &metadata.dependencies {
            if let Err(e) = WitPackageDependency::insert(
                &self.conn,
                wit_package_id,
                &dep.package,
                dep.version.as_deref(),
                None,
            ) {
                tracing::warn!("Failed to insert WIT package dependency: {}", e);
            }
        }

        // For compiled components, create wasm_component and component_target rows
        if metadata.is_component {
            // Extract rich metadata (name, producers) via wasm-metadata
            let (comp_name, comp_desc, producers_json) = extract_component_metadata(wasm_bytes);

            let component_id = match WasmComponent::insert(
                &self.conn,
                manifest_id,
                layer_id,
                comp_name.as_deref(),
                comp_desc.as_deref(),
                producers_json.as_deref(),
            ) {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!("Failed to insert WasmComponent: {}", e);
                    return false;
                }
            };

            for world in &metadata.worlds {
                let wit_world_id = world_ids.get(&world.name).copied();

                if let Err(e) = ComponentTarget::insert(
                    &self.conn,
                    component_id,
                    package_name,
                    &world.name,
                    version,
                    wit_world_id,
                ) {
                    tracing::warn!("Failed to insert ComponentTarget: {}", e);
                }
            }
        }

        // Best-effort resolution of cross-package foreign keys
        self.try_resolve_foreign_keys(wit_package_id, manifest_id);
        true
    }

    /// Best-effort resolution of cross-package foreign keys.
    ///
    /// After inserting all worlds, imports, exports, and dependencies, attempt
    /// to resolve `resolved_package_id` on import/export/dependency rows and
    /// `wit_world_id` on component_target rows by matching declared packages
    /// against existing `wit_package` and `wit_world` rows.
    ///
    /// Resolution may fail if a dependency hasn't been pulled yet — this is
    /// expected. Future pulls can re-resolve.
    fn try_resolve_foreign_keys(&self, wit_package_id: i64, manifest_id: i64) {
        if let Err(e) = resolve_import_foreign_keys(&self.conn, wit_package_id) {
            tracing::warn!("Failed to resolve import foreign keys: {}", e);
        }
        if let Err(e) = resolve_export_foreign_keys(&self.conn, wit_package_id) {
            tracing::warn!("Failed to resolve export foreign keys: {}", e);
        }
        if let Err(e) = resolve_dependency_foreign_keys(&self.conn, wit_package_id) {
            tracing::warn!("Failed to resolve dependency foreign keys: {}", e);
        }
        if let Err(e) = resolve_component_target_foreign_keys(&self.conn, manifest_id) {
            tracing::warn!("Failed to resolve component target foreign keys: {}", e);
        }
    }

    /// Store a referrer relationship between two manifests.
    ///
    /// The referrer manifest is upserted into the database if needed, and the
    /// relationship is recorded in `oci_referrer`. Uses the provided
    /// `registry`/`repository` to look up the repo for the referrer manifest.
    pub(crate) fn store_referrer(
        &self,
        subject_manifest_id: i64,
        registry: &str,
        repository: &str,
        referrer_digest: &str,
        artifact_type: &str,
    ) -> anyhow::Result<()> {
        let repo_id = OciRepository::upsert(&self.conn, registry, repository)?;

        // Upsert a minimal manifest entry for the referrer
        let (referrer_manifest_id, _) = OciManifest::upsert(
            &self.conn,
            repo_id,
            referrer_digest,
            None,
            None,
            None,
            Some(artifact_type),
            None,
            None,
            &HashMap::new(),
        )?;

        OciReferrer::insert(
            &self.conn,
            subject_manifest_id,
            referrer_manifest_id,
            artifact_type,
        )?;

        Ok(())
    }

    /// Re-extract WIT metadata for every package that has a cached OCI layer.
    ///
    /// This reads the raw wasm bytes back from the cacache store and, for each
    /// package, replaces the existing `wit_package` row (cascading to worlds,
    /// imports, exports, and dependencies) inside a savepoint only after
    /// extraction succeeds. Derived fields like `wit_text` can then pick up any
    /// improvements to the extraction logic.
    ///
    /// Original OCI data (manifests, layers, blobs) is never modified.
    pub(crate) async fn reindex_wit_packages(&self) -> anyhow::Result<u64> {
        // Collect (wit_package.id, oci_manifest_id, oci_layer_id, layer_digest)
        // tuples.  Two sources:
        //   1. wit_package rows that already have an oci_layer_id.
        //   2. wit_package rows with only an oci_manifest_id (legacy data) —
        //      we resolve the layer via the manifest's first wasm layer.
        let mut stmt = self.conn.prepare(
            "SELECT wp.id, wp.oci_manifest_id, ol.id, ol.digest
             FROM wit_package wp
             JOIN oci_layer ol ON ol.id = wp.oci_layer_id
             UNION ALL
             SELECT wp.id, wp.oci_manifest_id, ol.id, ol.digest
             FROM wit_package wp
             JOIN oci_layer ol ON ol.oci_manifest_id = wp.oci_manifest_id
             WHERE wp.oci_layer_id IS NULL
               AND wp.oci_manifest_id IS NOT NULL
               AND ol.position = (
                   SELECT MIN(ol2.position) FROM oci_layer ol2
                   WHERE ol2.oci_manifest_id = wp.oci_manifest_id
               )",
        )?;
        let rows: Vec<(i64, i64, i64, String)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let store_dir = self.state_info.store_dir().to_path_buf();
        let mut reindexed = 0u64;

        for (wit_id, manifest_id, layer_id, digest) in &rows {
            // Read the raw bytes from cacache.
            let bytes = match cacache::read(&store_dir, digest).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("reindex: failed to read layer {digest} from cache: {e}");
                    continue;
                }
            };

            if let Err(e) = self.conn.execute_batch("SAVEPOINT reindex_wit_package") {
                tracing::warn!("reindex: failed to start savepoint for wit_package {wit_id}: {e}");
                continue;
            }

            // Delete old rows and re-extract with current logic atomically.
            // Delete both wit_package and wasm_component rows so stale cached
            // JSON (producers, imports, exports) is fully cleared.
            let replaced = self
                .conn
                .execute("DELETE FROM wit_package WHERE id = ?1", [wit_id])
                .and_then(|_| {
                    self.conn.execute(
                        "DELETE FROM wasm_component WHERE oci_manifest_id = ?1",
                        [manifest_id],
                    )
                })
                .map_or_else(
                    |e| {
                        tracing::warn!(
                            "reindex: failed to delete rows for wit_package {wit_id}: {e}"
                        );
                        false
                    },
                    |_| self.try_extract_wit_package(*manifest_id, Some(*layer_id), &bytes),
                );

            let savepoint_sql = if replaced {
                "RELEASE SAVEPOINT reindex_wit_package"
            } else {
                "ROLLBACK TO SAVEPOINT reindex_wit_package; RELEASE SAVEPOINT reindex_wit_package"
            };

            if let Err(e) = self.conn.execute_batch(savepoint_sql) {
                tracing::warn!(
                    "reindex: failed to finalize savepoint for wit_package {wit_id}: {e}"
                );
                continue;
            }

            if replaced {
                reindexed += 1;
            }
        }

        Ok(reindexed)
    }

    /// Returns all currently stored images and their metadata.
    pub(crate) fn list_all(&self) -> anyhow::Result<Vec<RawImageEntry>> {
        RawImageEntry::get_all(&self.conn)
    }

    /// Deletes an image by its reference.
    /// Only removes cached layers if no other images reference them.
    pub(crate) async fn delete(&self, reference: &Reference) -> anyhow::Result<bool> {
        // Find the repository
        let repo = OciRepository::find(&self.conn, reference.registry(), reference.repository())?;
        let Some(repo) = repo else {
            return Ok(false);
        };

        // Resolve the manifest(s) to delete
        let repo_id = repo.id();
        let manifests_to_delete = match (reference.tag(), reference.digest()) {
            (Some(tag), Some(digest)) => {
                // Both tag and digest specified — verify tag matches digest, then delete
                if let Some(oci_tag) = OciTag::find_by_tag(&self.conn, repo_id, tag)? {
                    if oci_tag.manifest_digest == digest {
                        OciManifest::find(&self.conn, repo_id, digest)?
                            .into_iter()
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            (Some(tag), None) => {
                // Resolve tag to digest
                if let Some(oci_tag) = OciTag::find_by_tag(&self.conn, repo_id, tag)? {
                    OciManifest::find(&self.conn, repo_id, &oci_tag.manifest_digest)?
                        .into_iter()
                        .collect()
                } else {
                    Vec::new()
                }
            }
            (None, Some(digest)) => OciManifest::find(&self.conn, repo_id, digest)?
                .into_iter()
                .collect(),
            (None, None) => {
                // Delete all manifests for this repo
                OciManifest::list_by_repository(&self.conn, repo_id)?
            }
        };

        if manifests_to_delete.is_empty() {
            return Ok(false);
        }

        // Collect all layer digests from manifests being deleted
        let mut layer_digests: HashSet<String> = HashSet::new();
        let mut manifest_ids: Vec<i64> = Vec::new();
        for manifest in &manifests_to_delete {
            manifest_ids.push(manifest.id());
            if let Ok(layers) = OciLayer::list_by_manifest(&self.conn, manifest.id()) {
                for l in layers {
                    layer_digests.insert(l.digest);
                }
            }
        }

        // Find layers still needed by other manifests (ones NOT being deleted)
        let all_manifests = OciManifest::list_by_repository(&self.conn, repo_id)?;
        let mut retained_digests: HashSet<String> = HashSet::new();
        for other in &all_manifests {
            if manifest_ids.contains(&other.id()) {
                continue;
            }
            if let Ok(other_layers) = OciLayer::list_by_manifest(&self.conn, other.id()) {
                for l in other_layers {
                    retained_digests.insert(l.digest);
                }
            }
        }

        // Remove cached layers that are no longer needed
        let orphaned = crate::oci::compute_orphaned_layers(&layer_digests, &retained_digests);
        for layer_digest in &orphaned {
            let _ = cacache::remove(self.state_info.store_dir(), layer_digest).await;
        }

        // Delete the manifests (FK cascade handles layers, tags, etc.)
        for manifest in &manifests_to_delete {
            OciManifest::delete(&self.conn, manifest.id())?;
        }

        Ok(true)
    }

    /// Search for known packages by query string.
    pub(crate) fn search_known_packages(
        &self,
        query: &str,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        RawKnownPackage::search(&self.conn, query, offset, limit)
    }

    /// Search for known packages that import a given interface.
    pub(crate) fn search_known_packages_by_import(
        &self,
        interface: &str,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        RawKnownPackage::search_by_import(&self.conn, interface, offset, limit)
    }

    /// Search for known packages that export a given interface.
    pub(crate) fn search_known_packages_by_export(
        &self,
        interface: &str,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        RawKnownPackage::search_by_export(&self.conn, interface, offset, limit)
    }

    /// Get all known packages.
    pub(crate) fn list_known_packages(
        &self,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        RawKnownPackage::get_all(&self.conn, offset, limit)
    }

    /// Get recently updated known packages.
    pub(crate) fn list_recent_known_packages(
        &self,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        RawKnownPackage::get_recent(&self.conn, offset, limit)
    }

    /// Get a known package by registry and repository.
    pub(crate) fn get_known_package(
        &self,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<Option<RawKnownPackage>> {
        RawKnownPackage::get(&self.conn, registry, repository)
    }

    /// Add or update a known package.
    pub(crate) fn add_known_package(
        &self,
        registry: &str,
        repository: &str,
        tag: Option<&str>,
        description: Option<&str>,
    ) -> anyhow::Result<()> {
        RawKnownPackage::upsert(&self.conn, registry, repository, tag, description)
    }

    /// Add or update a known package with optional WIT namespace mapping.
    pub(crate) fn add_known_package_with_params(
        &self,
        params: &KnownPackageParams<'_>,
    ) -> anyhow::Result<()> {
        RawKnownPackage::upsert_with_params(&self.conn, params)
    }

    /// Get all WIT packages.
    #[allow(dead_code)]
    pub(crate) fn list_wit_packages(&self) -> anyhow::Result<Vec<RawWitPackage>> {
        RawWitPackage::get_all(&self.conn)
    }

    /// Get all WIT packages with their associated component references.
    pub(crate) fn list_wit_packages_with_components(
        &self,
    ) -> anyhow::Result<Vec<(RawWitPackage, String)>> {
        RawWitPackage::get_all_with_images(&self.conn)
    }

    /// Find the OCI reference for a WIT package by name and optional version.
    pub(crate) fn find_oci_reference_by_wit_name(
        &self,
        package_name: &str,
        version: Option<&str>,
    ) -> anyhow::Result<Option<(String, String)>> {
        RawWitPackage::find_oci_reference(&self.conn, package_name, version)
    }

    /// Search for a known package by WIT name (e.g. "wasi:http" → "wasi/http").
    pub(crate) fn search_known_package_by_wit_name(
        &self,
        wit_name: &str,
    ) -> anyhow::Result<Option<RawKnownPackage>> {
        RawKnownPackage::search_by_wit_name(&self.conn, wit_name)
    }

    /// Get a value from the `_sync_meta` table.
    #[allow(dead_code)]
    pub(crate) fn get_sync_meta(&self, key: &str) -> anyhow::Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM _sync_meta WHERE key = ?1")?;
        let mut rows = stmt.query_map([key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Set a value in the `_sync_meta` table.
    #[allow(dead_code)]
    pub(crate) fn set_sync_meta(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO _sync_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (key, value),
        )?;
        Ok(())
    }

    /// Return all declared dependencies for the package at the given OCI
    /// registry and repository.
    ///
    /// The query resolves dependencies for the **latest** indexed manifest
    /// (highest manifest `id`) so that it always reflects the most recent
    /// version. Two cases are covered:
    ///
    /// 1. **Pulled packages** — dependencies stored via the
    ///    `oci_manifest` → `oci_repository` chain after a full layer pull.
    ///    Only the latest manifest is considered to avoid mixing deps across
    ///    versions.
    /// 2. **Synced stubs** — `wit_package` rows with `oci_manifest_id IS NULL`
    ///    whose `package_name` matches the WIT namespace+name derived from
    ///    the OCI repository metadata.
    // r[impl db.wit-package-dependency.get-for-package]
    pub(crate) fn get_package_dependencies(
        &self,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::PackageDependencyRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT wpd.declared_package, wpd.declared_version
             FROM wit_package_dependency wpd
             JOIN wit_package wp ON wpd.dependent_id = wp.id
             WHERE
               wp.oci_manifest_id = (
                 SELECT om.id
                 FROM oci_manifest om
                 JOIN oci_repository repo ON om.oci_repository_id = repo.id
                 WHERE repo.registry = ?1 AND repo.repository = ?2
                 ORDER BY om.id DESC
                 LIMIT 1
               )
               OR (
                 wp.oci_manifest_id IS NULL
                 AND wp.package_name = (
                   SELECT repo.wit_namespace || ':' || repo.wit_name
                   FROM oci_repository repo
                   WHERE repo.registry = ?1 AND repo.repository = ?2
                     AND repo.wit_namespace IS NOT NULL
                     AND repo.wit_name IS NOT NULL
                   LIMIT 1
                 )
               )
             ORDER BY wpd.declared_package",
        )?;

        let rows = stmt.query_map(rusqlite::params![registry, repository], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (package, version) = row?;
            result.push(wasm_meta_registry_types::PackageDependencyRef { package, version });
        }
        Ok(result)
    }

    /// Return all declared dependencies for a package looked up by WIT name
    /// and optional version.
    ///
    /// This is a direct query on `wit_package_dependency` keyed by
    /// `wit_package.package_name` (and optional version), bypassing the OCI
    /// registry/repository path. Useful for tests and for the dependency
    /// resolver which works with WIT names, not OCI coordinates.
    ///
    /// When multiple `wit_package` rows exist for the same `(name, version)`
    /// (e.g. a sync stub and a pulled manifest row), the query selects the
    /// single *canonical* row — preferring a pulled row (`oci_manifest_id IS NOT NULL`)
    /// over a stub, then the newest `id` as a tiebreaker — before fetching
    /// its dependency edges.  This guarantees deterministic results for the
    /// pubgrub resolver.
    pub(crate) fn get_package_dependencies_by_name(
        &self,
        package_name: &str,
        version: Option<&str>,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::PackageDependencyRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT wpd.declared_package, wpd.declared_version
             FROM wit_package_dependency wpd
             WHERE wpd.dependent_id = (
                 SELECT id FROM wit_package
                 WHERE package_name = ?1
                   AND COALESCE(version, '') = COALESCE(?2, '')
                 ORDER BY (oci_manifest_id IS NOT NULL) DESC, id DESC
                 LIMIT 1
             )
             ORDER BY wpd.declared_package",
        )?;

        let rows = stmt.query_map(rusqlite::params![package_name, version], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (package, version) = row?;
            result.push(wasm_meta_registry_types::PackageDependencyRef { package, version });
        }
        Ok(result)
    }

    /// Return all known versions for a package, as stored in the `wit_package`
    /// table.  The list is sorted by insertion order (newest first) and then
    /// filtered / sorted by the caller as needed.
    ///
    /// Used by the dependency resolver to enumerate candidate versions when
    /// selecting the best match for a version range.
    // r[impl resolution.per-version-deps]
    pub(crate) fn list_wit_package_versions(
        &self,
        package_name: &str,
    ) -> anyhow::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT version
             FROM wit_package
             WHERE package_name = ?1
               AND version IS NOT NULL
             ORDER BY id DESC",
        )?;

        let rows = stmt.query_map([package_name], |row| row.get::<_, String>(0))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Find any existing `wit_package` row matching `(package_name, version)`,
    /// or create a new stub row (without OCI references) if none exists.
    ///
    /// Used during sync to anchor dependency edges to a canonical package row
    /// even before the package has been pulled from the registry.
    #[cfg(feature = "http-sync")]
    fn find_or_insert_wit_package(
        &self,
        package_name: &str,
        version: Option<&str>,
    ) -> anyhow::Result<i64> {
        let result = self.conn.query_row(
            "SELECT id FROM wit_package
             WHERE package_name = ?1
               AND COALESCE(version, '') = COALESCE(?2, '')
             ORDER BY (oci_manifest_id IS NOT NULL) DESC, id DESC
             LIMIT 1",
            rusqlite::params![package_name, version],
            |row| row.get::<_, i64>(0),
        );

        match result {
            Ok(id) => Ok(id),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                RawWitPackage::insert(&self.conn, package_name, version, None, None, None, None)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Store WIT package dependency information received from a meta-registry
    /// sync response.
    ///
    /// Creates (or reuses) a `wit_package` stub row for `package_name` /
    /// `version` and records the provided dependency edges in
    /// `wit_package_dependency`. Duplicate edges are silently ignored.
    ///
    /// This allows the dependency graph to be queried for pre-planned
    /// installation without performing a full layer pull first.
    // r[impl db.wit-package-dependency.populate-on-sync]
    // r[impl db.wit-package-dependency.upsert-idempotent]
    #[cfg(feature = "http-sync")]
    pub(crate) fn upsert_package_dependencies_from_sync(
        &self,
        package_name: &str,
        version: Option<&str>,
        dependencies: &[wasm_meta_registry_types::PackageDependencyRef],
    ) -> anyhow::Result<()> {
        // Always create (or reuse) the wit_package stub so the resolver
        // can enumerate available versions even for leaf packages with no
        // dependencies.
        let pkg_id = self.find_or_insert_wit_package(package_name, version)?;

        for dep in dependencies {
            if let Err(e) = WitPackageDependency::insert(
                &self.conn,
                pkg_id,
                &dep.package,
                dep.version.as_deref(),
                None,
            ) {
                tracing::warn!(
                    "Failed to insert synced dependency {} → {}: {}",
                    package_name,
                    dep.package,
                    e
                );
            }
        }

        Ok(())
    }

    // ================================================================
    // Rich query methods for the meta-registry API
    // ================================================================

    /// Return all versions of a package, identified by OCI registry and
    /// repository.  Each version is a (tag, manifest) pair joined across
    /// `oci_repository → oci_manifest → oci_tag`.
    ///
    /// Results are ordered by manifest insertion order (newest first).
    // r[verify db.package-versions.list]
    pub(crate) fn get_package_versions(
        &self,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::PackageVersion>> {
        use wasm_meta_registry_types::{OciAnnotations, PackageVersion};

        // First, find the repository.
        let repo_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM oci_repository
                 WHERE registry = ?1 AND repository = ?2",
                rusqlite::params![registry, repository],
                |row| row.get(0),
            )
            .optional()?;
        let Some(repo_id) = repo_id else {
            return Ok(Vec::new());
        };

        // Fetch all manifests for this repository, newest first.
        let mut manifest_stmt = self.conn.prepare(
            "SELECT id, digest, size_bytes, created_at,
                    oci_created, oci_authors, oci_url, oci_documentation,
                    oci_source, oci_version, oci_revision, oci_vendor,
                    oci_licenses, oci_title, oci_description
             FROM oci_manifest
             WHERE oci_repository_id = ?1
             ORDER BY id DESC",
        )?;

        let manifests = manifest_stmt
            .query_map([repo_id], |row| {
                Ok(ManifestRow {
                    id: row.get(0)?,
                    digest: row.get(1)?,
                    size_bytes: row.get(2)?,
                    synced_at: row.get(3)?,
                    oci_created: row.get(4)?,
                    oci_authors: row.get(5)?,
                    oci_url: row.get(6)?,
                    oci_documentation: row.get(7)?,
                    oci_source: row.get(8)?,
                    oci_version: row.get(9)?,
                    oci_revision: row.get(10)?,
                    oci_vendor: row.get(11)?,
                    oci_licenses: row.get(12)?,
                    oci_title: row.get(13)?,
                    oci_description: row.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut versions = Vec::with_capacity(manifests.len());
        for m in manifests {
            let manifest_created_at = m.oci_created.clone();

            // Find tags pointing to this manifest.
            let tag = self
                .conn
                .query_row(
                    "SELECT tag FROM oci_tag
                 WHERE oci_repository_id = ?1 AND manifest_digest = ?2
                 ORDER BY id DESC LIMIT 1",
                    rusqlite::params![repo_id, &m.digest],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            // Collect custom annotations.
            let custom = self.get_custom_annotations(m.id)?;

            let has_annotations = m.oci_created.is_some()
                || m.oci_authors.is_some()
                || m.oci_url.is_some()
                || m.oci_documentation.is_some()
                || m.oci_source.is_some()
                || m.oci_version.is_some()
                || m.oci_revision.is_some()
                || m.oci_vendor.is_some()
                || m.oci_licenses.is_some()
                || m.oci_title.is_some()
                || m.oci_description.is_some()
                || !custom.is_empty();

            let annotations = if has_annotations {
                Some(OciAnnotations {
                    created: manifest_created_at.clone(),
                    authors: m.oci_authors,
                    url: m.oci_url,
                    documentation: m.oci_documentation,
                    source: m.oci_source,
                    version: m.oci_version,
                    revision: m.oci_revision,
                    vendor: m.oci_vendor,
                    licenses: m.oci_licenses,
                    title: m.oci_title,
                    description: m.oci_description,
                    custom,
                })
            } else {
                None
            };

            let worlds = self.get_wit_worlds_for_manifest(m.id)?;
            let components = self.get_components_for_manifest(m.id)?;
            let dependencies = self.get_dependencies_for_manifest(m.id)?;
            let referrers = self.get_referrers_for_manifest(m.id)?;
            let layers = self.get_layers_for_manifest(m.id)?;
            let wit_text = self.get_wit_text_for_manifest(m.id)?;
            let type_docs = self.build_type_docs(&dependencies);

            versions.push(PackageVersion {
                tag,
                digest: m.digest,
                size_bytes: m.size_bytes,
                created_at: manifest_created_at,
                synced_at: Some(m.synced_at),
                annotations,
                worlds,
                components,
                dependencies,
                referrers,
                layers,
                wit_text,
                type_docs,
            });
        }

        Ok(versions)
    }

    /// Return a single version of a package by tag.
    ///
    /// Looks up the manifest directly via the `oci_tag` table rather than
    /// iterating all versions, so it works even when a manifest has multiple
    /// tags.
    // r[verify db.package-versions.get]
    pub(crate) fn get_package_version(
        &self,
        registry: &str,
        repository: &str,
        version_tag: &str,
    ) -> anyhow::Result<Option<wasm_meta_registry_types::PackageVersion>> {
        use wasm_meta_registry_types::{OciAnnotations, PackageVersion};

        // Find the repository.
        let repo_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM oci_repository
                 WHERE registry = ?1 AND repository = ?2",
                rusqlite::params![registry, repository],
                |row| row.get(0),
            )
            .optional()?;
        let Some(repo_id) = repo_id else {
            return Ok(None);
        };

        // Find the manifest digest for this tag.
        let digest: Option<String> = self
            .conn
            .query_row(
                "SELECT manifest_digest FROM oci_tag
                 WHERE oci_repository_id = ?1 AND tag = ?2
                 LIMIT 1",
                rusqlite::params![repo_id, version_tag],
                |row| row.get(0),
            )
            .optional()?;
        let Some(digest) = digest else {
            return Ok(None);
        };

        // Fetch the manifest row.
        let m: ManifestRow = self.conn.query_row(
            "SELECT id, digest, size_bytes, created_at,
                        oci_created, oci_authors, oci_url, oci_documentation,
                        oci_source, oci_version, oci_revision, oci_vendor,
                        oci_licenses, oci_title, oci_description
                 FROM oci_manifest
                 WHERE oci_repository_id = ?1 AND digest = ?2",
            rusqlite::params![repo_id, &digest],
            |row| {
                Ok(ManifestRow {
                    id: row.get(0)?,
                    digest: row.get(1)?,
                    size_bytes: row.get(2)?,
                    synced_at: row.get(3)?,
                    oci_created: row.get(4)?,
                    oci_authors: row.get(5)?,
                    oci_url: row.get(6)?,
                    oci_documentation: row.get(7)?,
                    oci_source: row.get(8)?,
                    oci_version: row.get(9)?,
                    oci_revision: row.get(10)?,
                    oci_vendor: row.get(11)?,
                    oci_licenses: row.get(12)?,
                    oci_title: row.get(13)?,
                    oci_description: row.get(14)?,
                })
            },
        )?;

        let manifest_created_at = m.oci_created.clone();
        let custom = self.get_custom_annotations(m.id)?;

        let has_annotations = m.oci_created.is_some()
            || m.oci_authors.is_some()
            || m.oci_url.is_some()
            || m.oci_documentation.is_some()
            || m.oci_source.is_some()
            || m.oci_version.is_some()
            || m.oci_revision.is_some()
            || m.oci_vendor.is_some()
            || m.oci_licenses.is_some()
            || m.oci_title.is_some()
            || m.oci_description.is_some()
            || !custom.is_empty();

        let annotations = if has_annotations {
            Some(OciAnnotations {
                created: manifest_created_at.clone(),
                authors: m.oci_authors,
                url: m.oci_url,
                documentation: m.oci_documentation,
                source: m.oci_source,
                version: m.oci_version,
                revision: m.oci_revision,
                vendor: m.oci_vendor,
                licenses: m.oci_licenses,
                title: m.oci_title,
                description: m.oci_description,
                custom,
            })
        } else {
            None
        };

        let worlds = self.get_wit_worlds_for_manifest(m.id)?;
        let components = self.get_components_for_manifest(m.id)?;
        let dependencies = self.get_dependencies_for_manifest(m.id)?;
        let referrers = self.get_referrers_for_manifest(m.id)?;
        let layers = self.get_layers_for_manifest(m.id)?;
        let wit_text = self.get_wit_text_for_manifest(m.id)?;
        let type_docs = self.build_type_docs(&dependencies);

        Ok(Some(PackageVersion {
            tag: Some(version_tag.to_string()),
            digest: m.digest,
            size_bytes: m.size_bytes,
            created_at: manifest_created_at,
            synced_at: Some(m.synced_at),
            annotations,
            worlds,
            components,
            dependencies,
            referrers,
            layers,
            wit_text,
            type_docs,
        }))
    }

    /// Return all WIT worlds (with imports and exports) found in WIT packages
    /// linked to the given OCI manifest.
    fn get_wit_worlds_for_manifest(
        &self,
        manifest_id: i64,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::WitWorldSummary>> {
        use wasm_meta_registry_types::WitWorldSummary;

        // Find all wit_package rows for this manifest.
        let mut pkg_stmt = self
            .conn
            .prepare("SELECT id FROM wit_package WHERE oci_manifest_id = ?1")?;
        let pkg_ids: Vec<i64> = pkg_stmt
            .query_map([manifest_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut worlds = Vec::new();
        for pkg_id in pkg_ids {
            let db_worlds = WitWorld::list_by_type(&self.conn, pkg_id)?;
            for w in db_worlds {
                let mut imports = self.get_world_imports(w.id())?;
                let mut exports = self.get_world_exports(w.id())?;
                self.enrich_iface_docs(&mut imports);
                self.enrich_iface_docs(&mut exports);
                worlds.push(WitWorldSummary {
                    name: w.name,
                    description: w.description,
                    imports,
                    exports,
                });
            }
        }

        Ok(worlds)
    }

    /// Return all Wasm components (with targets) found in the given manifest.
    fn get_components_for_manifest(
        &self,
        manifest_id: i64,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::ComponentSummary>> {
        use wasm_meta_registry_types::{ComponentSummary, ComponentTargetRef};
        type ComponentRow = (i64, Option<String>, Option<String>, Option<String>);

        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, producers_json FROM wasm_component
             WHERE oci_manifest_id = ?1
             ORDER BY name ASC",
        )?;

        let components: Vec<ComponentRow> = stmt
            .query_map([manifest_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut result = Vec::new();
        for (comp_id, name, description, metadata_json) in components {
            let targets = ComponentTarget::list_by_component(&self.conn, comp_id)?;
            let target_refs: Vec<ComponentTargetRef> = targets
                .into_iter()
                .map(|t| ComponentTargetRef {
                    package: t.declared_package,
                    world: t.declared_world,
                    version: t.declared_version,
                })
                .collect();

            // Try to deserialize the full ComponentSummary tree from JSON.
            // If available, merge in the DB-stored targets. Otherwise
            // fall back to a basic summary.
            let mut summary: ComponentSummary = metadata_json
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok())
                .unwrap_or_else(|| ComponentSummary {
                    name: name.clone(),
                    description: description.clone(),
                    targets: vec![],
                    producers: vec![],
                    kind: None,
                    size_bytes: None,
                    languages: vec![],
                    children: vec![],
                    source: None,
                    homepage: None,
                    licenses: None,
                    authors: None,
                    revision: None,
                    component_version: None,
                    bill_of_materials: vec![],
                    imports: vec![],
                    exports: vec![],
                });

            summary.targets = target_refs;

            // Enrich imports/exports with docs from stored WIT packages.
            self.enrich_iface_docs(&mut summary.imports);
            self.enrich_iface_docs(&mut summary.exports);

            result.push(summary);
        }

        Ok(result)
    }

    /// Return dependency refs for WIT packages linked to the given manifest.
    fn get_dependencies_for_manifest(
        &self,
        manifest_id: i64,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::PackageDependencyRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT wpd.declared_package, wpd.declared_version
             FROM wit_package_dependency wpd
             JOIN wit_package wp ON wpd.dependent_id = wp.id
             WHERE wp.oci_manifest_id = ?1
             ORDER BY wpd.declared_package",
        )?;

        let rows = stmt.query_map([manifest_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (package, version) = row?;
            result.push(wasm_meta_registry_types::PackageDependencyRef { package, version });
        }
        Ok(result)
    }

    /// Return referrers (signatures, SBOMs, attestations) for a manifest.
    fn get_referrers_for_manifest(
        &self,
        manifest_id: i64,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::ReferrerSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.artifact_type, m.digest
             FROM oci_referrer r
             JOIN oci_manifest m ON r.referrer_manifest_id = m.id
             WHERE r.subject_manifest_id = ?1
             ORDER BY r.created_at DESC",
        )?;

        let rows = stmt.query_map([manifest_id], |row| {
            Ok(wasm_meta_registry_types::ReferrerSummary {
                artifact_type: row.get(0)?,
                digest: row.get(1)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Enrich `WitInterfaceRef` entries with docs from stored WIT packages.
    ///
    /// For each import/export that has a package name and version but no docs,
    /// look up the stored `wit_text` for that WIT package, parse it, and
    /// extract the interface's doc comment.
    fn enrich_iface_docs(&self, refs: &mut [wasm_meta_registry_types::WitInterfaceRef]) {
        for iface_ref in refs.iter_mut() {
            if iface_ref.docs.is_some() {
                continue;
            }
            let Some(iface_name) = &iface_ref.interface else {
                continue;
            };

            // Look up the WIT text for this package+version.
            let wit_text: Option<String> = self
                .conn
                .query_row(
                    "SELECT wit_text FROM wit_package
                     WHERE package_name = ?1
                       AND wit_text IS NOT NULL
                     ORDER BY (COALESCE(version, '') = COALESCE(?2, '')) DESC, id DESC
                     LIMIT 1",
                    rusqlite::params![&iface_ref.package, &iface_ref.version],
                    |row| row.get(0),
                )
                .ok();

            let Some(wit_text) = wit_text else {
                continue;
            };

            // Parse the WIT text and find the interface's docs.
            if let Ok(pkg) = wit_parser::UnresolvedPackageGroup::parse("lookup.wit", &wit_text) {
                for (_, iface) in &pkg.main.interfaces {
                    if iface.name.as_deref() == Some(iface_name.as_str())
                        && let Some(doc) = &iface.docs.contents
                    {
                        iface_ref.docs = Some(first_doc_sentence(doc));
                    }
                }
            }
        }
    }

    /// Build a map of cross-package type documentation.
    ///
    /// For each dependency that has stored WIT text, parse it and extract
    /// docs for all types in all interfaces. Returns a map from
    /// `"package/interface/type"` → doc string.
    fn build_type_docs(
        &self,
        deps: &[wasm_meta_registry_types::PackageDependencyRef],
    ) -> HashMap<String, String> {
        let mut result = HashMap::new();

        for dep in deps {
            let wit_text: Option<String> = self
                .conn
                .query_row(
                    "SELECT wit_text FROM wit_package
                     WHERE package_name = ?1
                       AND wit_text IS NOT NULL
                     ORDER BY (version = ?2) DESC, id DESC LIMIT 1",
                    rusqlite::params![&dep.package, &dep.version],
                    |row| row.get(0),
                )
                .ok();

            let Some(wit_text) = wit_text else { continue };
            let Ok(pkg) = wit_parser::UnresolvedPackageGroup::parse("dep.wit", &wit_text) else {
                continue;
            };

            for (_, iface) in &pkg.main.interfaces {
                let Some(iface_name) = &iface.name else {
                    continue;
                };
                for (type_name, type_id) in &iface.types {
                    if let Some(type_def) = pkg.main.types.get(*type_id)
                        && let Some(docs) = &type_def.docs.contents
                        && !docs.is_empty()
                    {
                        let key = format!("{}/{iface_name}/{type_name}", dep.package);
                        let first = docs
                            .split_once("\n\n")
                            .map_or_else(|| docs.trim().to_owned(), |(f, _)| f.trim().to_owned());
                        result.insert(key, first);
                    }
                }
            }
        }

        result
    }

    /// Return the full OCI layout for a manifest: manifest descriptor,
    /// config descriptor, and content layers.
    fn get_layers_for_manifest(
        &self,
        manifest_id: i64,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::LayerInfo>> {
        use wasm_meta_registry_types::LayerInfo;

        let mut result = Vec::new();

        // Manifest descriptor.
        if let Ok((digest, media_type, size)) = self.conn.query_row(
            "SELECT digest, media_type, size_bytes FROM oci_manifest WHERE id = ?1",
            [manifest_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        ) {
            result.push(LayerInfo {
                digest,
                media_type,
                size_bytes: size,
            });
        }

        // Config descriptor.
        if let Ok((config_digest, config_media_type)) = self.conn.query_row(
            "SELECT config_digest, config_media_type FROM oci_manifest
             WHERE id = ?1 AND config_digest IS NOT NULL",
            [manifest_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        ) {
            result.push(LayerInfo {
                digest: config_digest,
                media_type: config_media_type,
                size_bytes: None,
            });
        }

        // Content layers.
        let layers = OciLayer::list_by_manifest(&self.conn, manifest_id)?;
        result.extend(layers.into_iter().map(|l| LayerInfo {
            digest: l.digest,
            media_type: l.media_type,
            size_bytes: l.size_bytes,
        }));

        Ok(result)
    }

    /// Return the WIT source text for the first WIT package linked to a manifest.
    fn get_wit_text_for_manifest(&self, manifest_id: i64) -> anyhow::Result<Option<String>> {
        let result = self.conn.query_row(
            "SELECT wit_text FROM wit_package
             WHERE oci_manifest_id = ?1 AND wit_text IS NOT NULL
             LIMIT 1",
            [manifest_id],
            |row| row.get::<_, String>(0),
        );

        match result {
            Ok(text) => Ok(Some(text)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Return custom annotations (non-well-known) for a manifest.
    fn get_custom_annotations(
        &self,
        manifest_id: i64,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::AnnotationEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT `key`, `value` FROM oci_manifest_annotation
             WHERE oci_manifest_id = ?1
             ORDER BY `key` ASC",
        )?;

        let rows = stmt.query_map([manifest_id], |row| {
            Ok(wasm_meta_registry_types::AnnotationEntry {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Return all imports for a WIT world.
    fn get_world_imports(
        &self,
        world_id: i64,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::WitInterfaceRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT declared_package, declared_interface, declared_version
             FROM wit_world_import
             WHERE wit_world_id = ?1
             ORDER BY declared_package ASC",
        )?;

        let rows = stmt.query_map([world_id], |row| {
            Ok(wasm_meta_registry_types::WitInterfaceRef {
                package: row.get(0)?,
                interface: row.get(1)?,
                version: row.get(2)?,
                docs: None,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Return all exports for a WIT world.
    fn get_world_exports(
        &self,
        world_id: i64,
    ) -> anyhow::Result<Vec<wasm_meta_registry_types::WitInterfaceRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT declared_package, declared_interface, declared_version
             FROM wit_world_export
             WHERE wit_world_id = ?1
             ORDER BY declared_package ASC",
        )?;

        let rows = stmt.query_map([world_id], |row| {
            Ok(wasm_meta_registry_types::WitInterfaceRef {
                package: row.get(0)?,
                interface: row.get(1)?,
                version: row.get(2)?,
                docs: None,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Build a full [`PackageDetail`] for a package identified by OCI
    /// registry and repository.
    // r[verify db.package-detail]
    pub(crate) fn get_package_detail(
        &self,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<Option<wasm_meta_registry_types::PackageDetail>> {
        // Look up the OCI repository row.
        let row = self.conn.query_row(
            "SELECT id, wit_namespace, wit_name, kind FROM oci_repository
             WHERE registry = ?1 AND repository = ?2",
            rusqlite::params![registry, repository],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        );

        let (_repo_id, wit_namespace, wit_name, kind_str) = match row {
            Ok(r) => r,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let kind = match kind_str.as_deref() {
            Some("component") => Some(wasm_meta_registry_types::PackageKind::Component),
            Some("interface") => Some(wasm_meta_registry_types::PackageKind::Interface),
            _ => None,
        };

        // Get the description from the latest known-package entry.
        let description = self
            .get_known_package(registry, repository)?
            .and_then(|pkg| pkg.description);

        let versions = self.get_package_versions(registry, repository)?;

        Ok(Some(wasm_meta_registry_types::PackageDetail {
            registry: registry.to_string(),
            repository: repository.to_string(),
            kind,
            description,
            wit_namespace,
            wit_name,
            versions,
        }))
    }
}

/// Resolve `wit_world_import.resolved_package_id` for imports belonging to
/// the given `wit_package_id` by matching `(declared_package, declared_version)`
/// against existing `wit_package` rows.
fn resolve_import_foreign_keys(conn: &Connection, wit_package_id: i64) -> anyhow::Result<usize> {
    let updated = conn.execute(
        "UPDATE wit_world_import
         SET resolved_package_id = (
             SELECT wi.id FROM wit_package wi
             WHERE wi.package_name = wit_world_import.declared_package
               AND COALESCE(wi.version, '') = COALESCE(wit_world_import.declared_version, '')
             LIMIT 1
         )
         WHERE wit_world_id IN (SELECT id FROM wit_world WHERE wit_package_id = ?1)
           AND resolved_package_id IS NULL",
        [wit_package_id],
    )?;
    Ok(updated)
}

/// Resolve `wit_world_export.resolved_package_id` for exports belonging to
/// the given `wit_package_id`.
fn resolve_export_foreign_keys(conn: &Connection, wit_package_id: i64) -> anyhow::Result<usize> {
    let updated = conn.execute(
        "UPDATE wit_world_export
         SET resolved_package_id = (
             SELECT wi.id FROM wit_package wi
             WHERE wi.package_name = wit_world_export.declared_package
               AND COALESCE(wi.version, '') = COALESCE(wit_world_export.declared_version, '')
             LIMIT 1
         )
         WHERE wit_world_id IN (SELECT id FROM wit_world WHERE wit_package_id = ?1)
           AND resolved_package_id IS NULL",
        [wit_package_id],
    )?;
    Ok(updated)
}

/// Resolve `wit_package_dependency.resolved_package_id` for deps of the
/// given `wit_package_id`.
fn resolve_dependency_foreign_keys(
    conn: &Connection,
    wit_package_id: i64,
) -> anyhow::Result<usize> {
    let updated = conn.execute(
        "UPDATE wit_package_dependency
         SET resolved_package_id = (
             SELECT wi.id FROM wit_package wi
             WHERE wi.package_name = wit_package_dependency.declared_package
               AND COALESCE(wi.version, '') = COALESCE(wit_package_dependency.declared_version, '')
             LIMIT 1
         )
         WHERE dependent_id = ?1
           AND resolved_package_id IS NULL",
        [wit_package_id],
    )?;
    Ok(updated)
}

/// Resolve `component_target.wit_world_id` for targets of components under
/// the given `manifest_id` by matching against `wit_world` + `wit_package`.
fn resolve_component_target_foreign_keys(
    conn: &Connection,
    manifest_id: i64,
) -> anyhow::Result<usize> {
    let updated = conn.execute(
        "UPDATE component_target
         SET wit_world_id = (
             SELECT ww.id FROM wit_world ww
             JOIN wit_package wi ON ww.wit_package_id = wi.id
             WHERE wi.package_name = component_target.declared_package
               AND COALESCE(wi.version, '') = COALESCE(component_target.declared_version, '')
               AND ww.name = component_target.declared_world
             LIMIT 1
         )
         WHERE wasm_component_id IN (
             SELECT id FROM wasm_component WHERE oci_manifest_id = ?1
         )
           AND wit_world_id IS NULL",
        [manifest_id],
    )?;
    Ok(updated)
}

/// Split a WIT package name like `"wasi:http@0.2.0"` into `("wasi:http", Some("0.2.0"))`.
///
/// If no `@` is present, returns the original string with `None` for the version.
fn split_package_version(raw: &str) -> (&str, Option<&str>) {
    if let Some(at_pos) = raw.rfind('@') {
        (&raw[..at_pos], Some(&raw[at_pos + 1..]))
    } else {
        (raw, None)
    }
}

/// Extract component-level metadata using `wasm-metadata`.
///
/// Returns `(name, description, metadata_json)` where `metadata_json` is the
/// full `ComponentSummary` tree serialized as JSON (producers, children, kind,
/// size, languages). Targets are not included — they come from the DB.
fn extract_component_metadata(
    wasm_bytes: &[u8],
) -> (Option<String>, Option<String>, Option<String>) {
    let Ok(payload) = wasm_metadata::Payload::from_binary(wasm_bytes) else {
        return (None, None, None);
    };

    // Decode WIT once for the full component — used by all children.
    let root_wit = wit_parser::decoding::decode(wasm_bytes).ok();

    let summary = payload_to_summary(&payload, wasm_bytes, root_wit.as_ref());
    let name = summary.name.clone();
    let description = summary.description.clone();
    let json = serde_json::to_string(&summary).ok();

    (name, description, json)
}

/// Convert a `wasm_metadata::Payload` tree into a `ComponentSummary` tree.
///
/// `parent_bytes` is the full wasm binary; child byte ranges are sliced from
/// it to extract WIT imports/exports via `wit-parser`.
/// `root_wit` is the decoded WIT from the full parent binary, used to resolve
/// inner component names back to WIT-qualified names.
fn payload_to_summary(
    payload: &wasm_metadata::Payload,
    parent_bytes: &[u8],
    root_wit: Option<&wit_parser::decoding::DecodedWasm>,
) -> wasm_meta_registry_types::ComponentSummary {
    use wasm_meta_registry_types::{BomEntry, ComponentSummary, ProducerEntry};

    let meta = payload.metadata();

    let (kind, children) = match payload {
        wasm_metadata::Payload::Component { children, .. } => (
            "component",
            children
                .iter()
                .map(|c| payload_to_summary(c, parent_bytes, root_wit))
                .collect(),
        ),
        wasm_metadata::Payload::Module(_) => ("module", vec![]),
    };

    let producers: Vec<ProducerEntry> = meta
        .producers
        .iter()
        .flat_map(|p| {
            p.iter().flat_map(|(field, pairs)| {
                pairs.iter().map(move |(tool, ver)| ProducerEntry {
                    field: field.clone(),
                    name: tool.clone(),
                    version: ver.clone(),
                })
            })
        })
        .collect();

    let languages: Vec<String> = producers
        .iter()
        .filter(|e| e.field == "language")
        .map(|e| e.name.clone())
        .collect();

    #[allow(clippy::cast_sign_loss)]
    let size_bytes = {
        let r = &meta.range;
        let size = r.end.saturating_sub(r.start);
        if size > 0 { Some(size as u64) } else { None }
    };

    let bill_of_materials: Vec<BomEntry> = meta
        .dependencies
        .as_ref()
        .map(|deps| {
            deps.version_info()
                .packages
                .iter()
                .map(|p| BomEntry {
                    name: p.name.clone(),
                    version: p.version.to_string(),
                    source: Some(String::from(p.source.clone())),
                })
                .collect()
        })
        .unwrap_or_default();

    // Extract WIT imports/exports from this component's byte range.
    let (imports, exports) = extract_wit_imports_exports(parent_bytes, &meta.range, root_wit);

    ComponentSummary {
        name: meta.name.clone(),
        description: meta.description.as_ref().map(ToString::to_string),
        targets: vec![],
        producers,
        kind: Some(kind.to_string()),
        size_bytes,
        languages,
        children,
        source: meta.source.as_ref().map(ToString::to_string),
        homepage: meta.homepage.as_ref().map(ToString::to_string),
        licenses: meta.licenses.as_ref().map(ToString::to_string),
        authors: meta.authors.as_ref().map(ToString::to_string),
        revision: meta.revision.as_ref().map(ToString::to_string),
        component_version: meta.version.as_ref().map(ToString::to_string),
        bill_of_materials,
        imports,
        exports,
    }
}

/// Extract WIT imports and exports from a wasm binary at the given byte range.
///
/// First tries `wit-parser` for a full decode (works for self-contained
/// components). Falls back to using the root component's decoded WIT to
/// provide the correct WIT-qualified names. If neither works, uses
/// `wasmparser` to read raw component import/export names.
fn extract_wit_imports_exports(
    parent_bytes: &[u8],
    range: &std::ops::Range<usize>,
    root_wit: Option<&wit_parser::decoding::DecodedWasm>,
) -> (
    Vec<wasm_meta_registry_types::WitInterfaceRef>,
    Vec<wasm_meta_registry_types::WitInterfaceRef>,
) {
    let Some(bytes) = parent_bytes.get(range.start..range.end) else {
        return (vec![], vec![]);
    };

    // Try wit-parser on the child's own bytes (works for self-contained components).
    if let Ok(decoded) = wit_parser::decoding::decode(bytes) {
        let resolve = decoded.resolve();
        if let wit_parser::decoding::DecodedWasm::Component(_, world_id) = &decoded
            && let Some(world) = resolve.worlds.get(*world_id)
        {
            let imports = world
                .imports
                .keys()
                .filter_map(|k| world_key_to_iface_ref(resolve, k))
                .collect();
            let exports = world
                .exports
                .keys()
                .filter_map(|k| world_key_to_iface_ref(resolve, k))
                .collect();
            return (imports, exports);
        }
    }

    // For inner components that can't be decoded standalone, use the root
    // component's already-decoded WIT world. The root world's imports/exports
    // are the WIT-level view of what the inner component implements.
    if let Some(decoded) = root_wit {
        let resolve = decoded.resolve();
        if let wit_parser::decoding::DecodedWasm::Component(_, world_id) = decoded
            && let Some(world) = resolve.worlds.get(*world_id)
        {
            let imports = world
                .imports
                .keys()
                .filter_map(|k| world_key_to_iface_ref(resolve, k))
                .collect();
            let exports = world
                .exports
                .keys()
                .filter_map(|k| world_key_to_iface_ref(resolve, k))
                .collect();
            return (imports, exports);
        }
    }

    // Last resort: raw wasmparser names.
    extract_wit_imports_exports_wasmparser(bytes)
}

/// Use `wasmparser` to extract component import/export names from raw bytes.
///
/// This handles inner components that can't be decoded by `wit-parser` because
/// they reference types from the outer component scope.
fn extract_wit_imports_exports_wasmparser(
    bytes: &[u8],
) -> (
    Vec<wasm_meta_registry_types::WitInterfaceRef>,
    Vec<wasm_meta_registry_types::WitInterfaceRef>,
) {
    use wasmparser::{Parser, Payload};

    let mut imports = Vec::new();
    let mut exports = Vec::new();

    for payload in Parser::new(0).parse_all(bytes) {
        let Ok(payload) = payload else { continue };
        match payload {
            Payload::ComponentImportSection(reader) => {
                for import in reader {
                    let Ok(import) = import else { continue };
                    imports.push(parse_component_extern_name(import.name.0));
                }
            }
            Payload::ComponentExportSection(reader) => {
                for export in reader {
                    let Ok(export) = export else { continue };
                    exports.push(parse_component_extern_name(export.name.0));
                }
            }
            _ => {}
        }
    }

    (imports, exports)
}

/// Parse a component extern name like `"wasi:http/types@0.2.3"` into a
/// `WitInterfaceRef`.
fn parse_component_extern_name(name: &str) -> wasm_meta_registry_types::WitInterfaceRef {
    // Component extern names follow the pattern:
    //   "namespace:package/interface@version"
    //   "namespace:package@version"
    //   or just a plain name
    if !name.contains(':') {
        return wasm_meta_registry_types::WitInterfaceRef {
            package: name.to_string(),
            interface: None,
            version: None,
            docs: None,
        };
    }

    let (name_part, version) = match name.rsplit_once('@') {
        Some((n, v)) => (n, Some(v.to_string())),
        None => (name, None),
    };

    let (package, interface) = match name_part.split_once('/') {
        Some((pkg, iface)) => (pkg.to_string(), Some(iface.to_string())),
        None => (name_part.to_string(), None),
    };

    wasm_meta_registry_types::WitInterfaceRef {
        package,
        interface,
        version,
        docs: None,
    }
}

/// Convert a `wit_parser::WorldKey` to a `WitInterfaceRef`, including docs.
fn world_key_to_iface_ref(
    resolve: &wit_parser::Resolve,
    key: &wit_parser::WorldKey,
) -> Option<wasm_meta_registry_types::WitInterfaceRef> {
    match key {
        wit_parser::WorldKey::Name(name) => Some(wasm_meta_registry_types::WitInterfaceRef {
            package: name.clone(),
            interface: None,
            version: None,
            docs: None,
        }),
        wit_parser::WorldKey::Interface(id) => {
            let iface = resolve.interfaces.get(*id)?;
            let pkg_id = iface.package?;
            let pkg = resolve.packages.get(pkg_id)?;
            let docs = iface.docs.contents.as_deref().map(first_doc_sentence);
            Some(wasm_meta_registry_types::WitInterfaceRef {
                package: format!("{}:{}", pkg.name.namespace, pkg.name.name),
                interface: iface.name.clone(),
                version: pkg.name.version.as_ref().map(ToString::to_string),
                docs,
            })
        }
    }
}

/// Extract the first sentence from a doc comment.
fn first_doc_sentence(text: &str) -> String {
    text.split_once("\n\n").map_or_else(
        || text.trim().to_owned(),
        |(first, _)| first.trim().to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_package_version_with_version() {
        let (name, version) = split_package_version("wasi:http@0.2.0");
        assert_eq!(name, "wasi:http");
        assert_eq!(version, Some("0.2.0"));
    }

    #[test]
    fn test_split_package_version_without_version() {
        let (name, version) = split_package_version("wasi:http");
        assert_eq!(name, "wasi:http");
        assert_eq!(version, None);
    }

    #[test]
    fn test_split_package_version_complex() {
        let (name, version) = split_package_version("wasi:io/streams@1.0.0-rc1");
        assert_eq!(name, "wasi:io/streams");
        assert_eq!(version, Some("1.0.0-rc1"));
    }

    /// Create an in-memory database with migrations applied for testing.
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();
        conn
    }

    /// Helper: create a repo + manifest in the test DB, returning the manifest ID.
    fn insert_test_manifest(conn: &Connection) -> i64 {
        let repo_id = OciRepository::upsert(conn, "ghcr.io", "test/pkg").unwrap();
        let annotations = HashMap::new();
        let (manifest_id, _) = OciManifest::upsert(
            conn,
            repo_id,
            "sha256:abc123",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(1024),
            None,
            None,
            None,
            &annotations,
        )
        .unwrap();
        manifest_id
    }

    // r[verify wit.world.insert]
    #[test]
    fn wit_world_insert_and_query() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        let iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            Some("package wasi:http;"),
            Some(manifest_id),
            None,
        )
        .unwrap();

        let world_id = WitWorld::insert(&conn, iface_id, "proxy", None).unwrap();
        assert!(world_id > 0);

        let found = WitWorld::find_by_name(&conn, iface_id, "proxy")
            .unwrap()
            .expect("world should exist");
        assert_eq!(found.name, "proxy");
        assert_eq!(found.wit_package_id, iface_id);
    }

    // r[verify wit.world.imports-exports]
    #[test]
    fn wit_world_import_export_insert() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        let iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let world_id = WitWorld::insert(&conn, iface_id, "proxy", None).unwrap();

        let import_id = WitWorldImport::insert(
            &conn,
            world_id,
            "wasi:io",
            Some("streams"),
            Some("0.2.0"),
            None,
        )
        .unwrap();
        assert!(import_id > 0);

        let export_id = WitWorldExport::insert(
            &conn,
            world_id,
            "wasi:http",
            Some("handler"),
            Some("0.2.0"),
            None,
        )
        .unwrap();
        assert!(export_id > 0);
    }

    // r[verify wit.interface.dependencies]
    #[test]
    fn wit_package_dependency_insert() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        let iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let dep_id =
            WitPackageDependency::insert(&conn, iface_id, "wasi:io", Some("0.2.0"), None).unwrap();
        assert!(dep_id > 0);
    }

    // r[verify wit.component.insert]
    #[test]
    fn wasm_component_and_target_insert() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);
        let layer_id =
            OciLayer::insert(&conn, manifest_id, "sha256:layer1", None, Some(100), 0).unwrap();

        // Create the WIT interface and world first
        let iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            Some(layer_id),
        )
        .unwrap();

        let world_id = WitWorld::insert(&conn, iface_id, "proxy", None).unwrap();

        // Insert the component
        let comp_id =
            WasmComponent::insert(&conn, manifest_id, Some(layer_id), None, None, None).unwrap();
        assert!(comp_id > 0);

        // Insert a target pointing to the world we just created
        let target_id = ComponentTarget::insert(
            &conn,
            comp_id,
            "wasi:http",
            "proxy",
            Some("0.2.0"),
            Some(world_id),
        )
        .unwrap();
        assert!(target_id > 0);

        // Verify the component can be found by manifest
        let found = WasmComponent::find_by_manifest(&conn, manifest_id)
            .unwrap()
            .expect("component should exist");
        assert_eq!(found.id(), comp_id);
    }

    // r[verify wit.component.wit-only]
    #[test]
    fn no_component_rows_for_wit_only_package() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        // Insert interface and world (as we would for a WIT-only package)
        let iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let _world_id = WitWorld::insert(&conn, iface_id, "proxy", None).unwrap();

        // For a WIT-only package, we should NOT insert a WasmComponent
        let component = WasmComponent::find_by_manifest(&conn, manifest_id).unwrap();
        assert!(
            component.is_none(),
            "WIT-only packages should not have wasm_component rows"
        );
    }

    // r[verify wit.world.idempotent]
    #[test]
    fn wit_world_import_export_idempotent() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        let iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let world_id = WitWorld::insert(&conn, iface_id, "proxy", None).unwrap();

        // Insert same import twice — should be idempotent
        let id1 = WitWorldImport::insert(
            &conn,
            world_id,
            "wasi:io",
            Some("streams"),
            Some("0.2.0"),
            None,
        )
        .unwrap();
        let id2 = WitWorldImport::insert(
            &conn,
            world_id,
            "wasi:io",
            Some("streams"),
            Some("0.2.0"),
            None,
        )
        .unwrap();
        assert_eq!(id1, id2, "duplicate imports should return the same ID");
    }

    // r[verify wit.resolve.import]
    #[test]
    fn resolve_import_resolved_package_id_when_dep_exists() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        // Create the dependency interface (wasi:io@0.2.0) first
        let dep_iface_id = RawWitPackage::insert(
            &conn,
            "wasi:io",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        // Create the main interface and a world that imports wasi:io
        let main_iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let world_id = WitWorld::insert(&conn, main_iface_id, "proxy", None).unwrap();

        // Insert an import with no resolved_package_id
        WitWorldImport::insert(
            &conn,
            world_id,
            "wasi:io",
            Some("streams"),
            Some("0.2.0"),
            None,
        )
        .unwrap();

        // Run the resolution pass
        resolve_import_foreign_keys(&conn, main_iface_id).unwrap();

        // Verify the resolved_package_id was set
        let resolved: Option<i64> = conn
            .query_row(
                "SELECT resolved_package_id FROM wit_world_import WHERE wit_world_id = ?1",
                [world_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            resolved,
            Some(dep_iface_id),
            "import should resolve to the dependency interface"
        );
    }

    // r[verify wit.resolve.import-missing]
    #[test]
    fn resolve_import_stays_null_when_dep_missing() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        let iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let world_id = WitWorld::insert(&conn, iface_id, "proxy", None).unwrap();

        // Insert import for wasi:io — which does NOT exist in the DB
        WitWorldImport::insert(
            &conn,
            world_id,
            "wasi:io",
            Some("streams"),
            Some("0.2.0"),
            None,
        )
        .unwrap();

        // Run the resolution pass
        resolve_import_foreign_keys(&conn, iface_id).unwrap();

        // Verify the resolved_package_id is still NULL
        let resolved: Option<i64> = conn
            .query_row(
                "SELECT resolved_package_id FROM wit_world_import WHERE wit_world_id = ?1",
                [world_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            resolved, None,
            "import should remain unresolved when dependency is not in DB"
        );
    }

    // r[verify wit.resolve.dependency]
    #[test]
    fn resolve_dependency_resolved_package_id() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        // Create the dependency interface
        let dep_iface_id = RawWitPackage::insert(
            &conn,
            "wasi:io",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        // Create the main interface
        let main_iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        // Insert a dependency with no resolved_package_id
        WitPackageDependency::insert(&conn, main_iface_id, "wasi:io", Some("0.2.0"), None).unwrap();

        // Run the resolution pass
        resolve_dependency_foreign_keys(&conn, main_iface_id).unwrap();

        // Verify the resolved_package_id was set
        let resolved: Option<i64> = conn
            .query_row(
                "SELECT resolved_package_id FROM wit_package_dependency WHERE dependent_id = ?1",
                [main_iface_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            resolved,
            Some(dep_iface_id),
            "dependency should resolve to the dependency interface"
        );
    }

    // r[verify wit.resolve.export]
    #[test]
    fn resolve_export_resolved_package_id() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        // Create the target interface for the export
        let handler_iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let world_id = WitWorld::insert(&conn, handler_iface_id, "proxy", None).unwrap();

        // Insert an export with no resolved_package_id
        WitWorldExport::insert(
            &conn,
            world_id,
            "wasi:http",
            Some("handler"),
            Some("0.2.0"),
            None,
        )
        .unwrap();

        // Run the resolution pass
        resolve_export_foreign_keys(&conn, handler_iface_id).unwrap();

        // Verify the resolved_package_id was set
        let resolved: Option<i64> = conn
            .query_row(
                "SELECT resolved_package_id FROM wit_world_export WHERE wit_world_id = ?1",
                [world_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            resolved,
            Some(handler_iface_id),
            "export should resolve to the matching interface"
        );
    }

    // r[verify wit.resolve.component-target]
    #[test]
    fn resolve_component_target_cross_package() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        // Create a second manifest for the component
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "test/component").unwrap();
        let comp_annotations = HashMap::new();
        let (comp_manifest_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:comp456",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(2048),
            None,
            None,
            None,
            &comp_annotations,
        )
        .unwrap();

        // Create the WIT interface and world that the component targets
        let iface_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();
        let world_id = WitWorld::insert(&conn, iface_id, "proxy", None).unwrap();

        // Create a component with a target but NO wit_world_id (cross-package)
        let comp_id =
            WasmComponent::insert(&conn, comp_manifest_id, None, None, None, None).unwrap();
        ComponentTarget::insert(
            &conn,
            comp_id,
            "wasi:http",
            "proxy",
            Some("0.2.0"),
            None, // wit_world_id is NULL — needs resolution
        )
        .unwrap();

        // Run the resolution pass for component targets
        resolve_component_target_foreign_keys(&conn, comp_manifest_id).unwrap();

        // Verify wit_world_id was resolved
        let resolved: Option<i64> = conn
            .query_row(
                "SELECT wit_world_id FROM component_target WHERE wasm_component_id = ?1",
                [comp_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            resolved,
            Some(world_id),
            "component target should resolve to the matching world"
        );
    }

    // r[verify db.wit-package.find-oci-reference]
    #[test]
    fn find_oci_reference_returns_registry_and_repository() {
        let conn = setup_test_db();

        // Set up OCI repository and manifest
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "webassembly/wasi/http").unwrap();
        let annotations = HashMap::new();
        let (manifest_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:http123",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(1024),
            None,
            None,
            None,
            &annotations,
        )
        .unwrap();

        // Insert a WIT package linked to the manifest
        RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        // Lookup should find the OCI reference
        let result = RawWitPackage::find_oci_reference(&conn, "wasi:http", Some("0.2.0")).unwrap();
        assert_eq!(
            result,
            Some(("ghcr.io".to_string(), "webassembly/wasi/http".to_string()))
        );
    }

    // r[verify db.wit-package.find-oci-reference-not-found]
    #[test]
    fn find_oci_reference_returns_none_when_not_found() {
        let conn = setup_test_db();
        let result = RawWitPackage::find_oci_reference(&conn, "wasi:nonexistent", None).unwrap();
        assert!(result.is_none());
    }

    // r[verify db.wit-package-dependency.get-for-package]
    #[test]
    fn get_package_dependencies_returns_empty_without_data() {
        let conn = setup_test_db();
        let store = Store::from_conn(conn);
        let deps = store
            .get_package_dependencies("ghcr.io", "webassembly/wasi-http")
            .unwrap();
        assert!(deps.is_empty());
    }

    // r[verify db.wit-package-dependency.get-for-package]
    // r[verify server.index.dependencies]
    #[test]
    fn get_package_dependencies_returns_deps_for_pulled_package() {
        let conn = setup_test_db();

        // Set up an OCI repository + manifest
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "webassembly/wasi-http").unwrap();
        let annotations = HashMap::new();
        let (manifest_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:http123",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(1024),
            None,
            None,
            None,
            &annotations,
        )
        .unwrap();

        // Insert a WIT package linked to the manifest
        let pkg_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        // Insert dependencies
        WitPackageDependency::insert(&conn, pkg_id, "wasi:io", Some("0.2.0"), None).unwrap();
        WitPackageDependency::insert(&conn, pkg_id, "wasi:clocks", Some("0.2.0"), None).unwrap();

        let store = Store::from_conn(conn);
        let mut deps = store
            .get_package_dependencies("ghcr.io", "webassembly/wasi-http")
            .unwrap();
        deps.sort_by(|a, b| a.package.cmp(&b.package));
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].package, "wasi:clocks");
        assert_eq!(deps[0].version.as_deref(), Some("0.2.0"));
        assert_eq!(deps[1].package, "wasi:io");
        assert_eq!(deps[1].version.as_deref(), Some("0.2.0"));
    }

    // r[verify db.wit-package-dependency.get-for-package]
    #[test]
    fn get_package_dependencies_returns_deps_for_synced_package() {
        let conn = setup_test_db();

        // Register the repository with wit_namespace and wit_name (as sync does)
        OciRepository::upsert_with_wit(
            &conn,
            "ghcr.io",
            "webassembly/wasi-http",
            Some("wasi"),
            Some("http"),
        )
        .unwrap();

        // Insert a wit_package stub without an oci_manifest_id (sync-only)
        let pkg_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();
        WitPackageDependency::insert(&conn, pkg_id, "wasi:io", Some("0.2.0"), None).unwrap();

        let store = Store::from_conn(conn);
        let deps = store
            .get_package_dependencies("ghcr.io", "webassembly/wasi-http")
            .unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].package, "wasi:io");
        assert_eq!(deps[0].version.as_deref(), Some("0.2.0"));
    }

    // r[verify db.wit-package-dependency.get-for-package]
    /// Regression: when both a synced stub (`oci_manifest_id IS NULL`) and a
    /// pulled manifest-linked `wit_package` row exist for the same WIT package
    /// name/version, `get_package_dependencies_by_name` MUST return the deps
    /// from the pulled row and MUST NOT mix in stub deps.
    #[test]
    fn get_package_dependencies_by_name_prefers_pulled_over_stub() {
        let conn = setup_test_db();

        // Insert a synced stub wit_package (no manifest linkage, no layer linkage).
        let stub_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();
        WitPackageDependency::insert(&conn, stub_id, "wasi:stub-only-dep", Some("0.2.0"), None)
            .unwrap();

        // Set up a real OCI repository + manifest + layer so FK constraints are satisfied.
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "webassembly/wasi-http").unwrap();
        let annotations = HashMap::new();
        let (manifest_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:http-pulled",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(1024),
            None,
            None,
            None,
            &annotations,
        )
        .unwrap();
        let layer_id =
            OciLayer::insert(&conn, manifest_id, "sha256:http-layer", None, None, 0).unwrap();

        // Insert a pulled manifest-linked wit_package with a non-NULL oci_layer_id
        // so it gets a distinct unique key and coexists with the stub row.
        let pulled_id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            Some(layer_id),
        )
        .unwrap();
        WitPackageDependency::insert(&conn, pulled_id, "wasi:io", Some("0.2.0"), None).unwrap();

        // Confirm both rows are distinct.
        assert_ne!(stub_id, pulled_id, "stub and pulled must be distinct rows");

        let store = Store::from_conn(conn);
        let deps = store
            .get_package_dependencies_by_name("wasi:http", Some("0.2.0"))
            .unwrap();

        // Must only return the pulled dep, not the stub dep.
        assert_eq!(deps.len(), 1, "expected only pulled deps, got: {deps:?}");
        assert_eq!(deps[0].package, "wasi:io");
        assert_eq!(deps[0].version.as_deref(), Some("0.2.0"));
    }

    // r[verify db.wit-package-dependency.upsert-idempotent]
    // r[verify db.wit-package-dependency.populate-on-sync]
    #[cfg(feature = "http-sync")]
    #[test]
    fn upsert_package_dependencies_from_sync_is_idempotent() {
        use wasm_meta_registry_types::PackageDependencyRef;

        let conn = setup_test_db();
        let store = Store::from_conn(conn);

        let deps = vec![
            PackageDependencyRef {
                package: "wasi:io".into(),
                version: Some("0.2.0".into()),
            },
            PackageDependencyRef {
                package: "wasi:clocks".into(),
                version: None,
            },
        ];

        // First upsert
        store
            .upsert_package_dependencies_from_sync("wasi:http", Some("0.2.0"), &deps)
            .unwrap();

        // Second upsert must not fail and must not duplicate
        store
            .upsert_package_dependencies_from_sync("wasi:http", Some("0.2.0"), &deps)
            .unwrap();

        let mut stored = store
            .get_package_dependencies_by_name("wasi:http", Some("0.2.0"))
            .unwrap();
        stored.sort_by(|a, b| a.package.cmp(&b.package));
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].package, "wasi:clocks");
        assert_eq!(stored[1].package, "wasi:io");
    }

    // r[verify db.package-versions.list]
    // r[verify db.package-versions.get]
    // r[verify db.package-detail]
    #[test]
    fn rich_package_queries_expose_manifest_created_and_synced_timestamps() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "test/pkg").unwrap();
        let mut annotations = HashMap::new();
        annotations.insert(
            "org.opencontainers.image.created".to_string(),
            "2025-01-01T00:00:00Z".to_string(),
        );
        let (manifest_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:abc123",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(1024),
            None,
            None,
            None,
            &annotations,
        )
        .unwrap();
        OciTag::upsert(&conn, repo_id, "0.1.0", "sha256:abc123").unwrap();

        let store = Store::from_conn(conn);
        let versions = store.get_package_versions("ghcr.io", "test/pkg").unwrap();
        assert_eq!(versions.len(), 1);
        let version = &versions[0];
        assert_eq!(version.tag.as_deref(), Some("0.1.0"));
        assert_eq!(version.created_at.as_deref(), Some("2025-01-01T00:00:00Z"));
        assert!(
            version.synced_at.is_some(),
            "synced_at should be populated from local DB insertion time"
        );
        assert_eq!(
            version
                .annotations
                .as_ref()
                .and_then(|annotations| annotations.created.as_deref()),
            Some("2025-01-01T00:00:00Z")
        );

        let single = store
            .get_package_version("ghcr.io", "test/pkg", "0.1.0")
            .unwrap()
            .expect("expected tagged version");
        assert_eq!(single.digest, "sha256:abc123");

        let detail = store
            .get_package_detail("ghcr.io", "test/pkg")
            .unwrap()
            .expect("expected package detail");
        assert_eq!(detail.registry, "ghcr.io");
        assert_eq!(detail.repository, "test/pkg");
        assert_eq!(detail.versions.len(), 1);
        assert_eq!(
            detail.versions[0].created_at.as_deref(),
            Some("2025-01-01T00:00:00Z")
        );

        // Keep `manifest_id` in scope to ensure migration/schema setup accepted row.
        assert!(manifest_id > 0);
    }

    #[test]
    fn get_package_version_resolves_each_tag_when_multiple_tags_share_manifest() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "test/pkg").unwrap();
        let annotations = HashMap::new();
        OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:shared",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(1024),
            None,
            None,
            None,
            &annotations,
        )
        .unwrap();
        OciTag::upsert(&conn, repo_id, "1.0.0", "sha256:shared").unwrap();
        OciTag::upsert(&conn, repo_id, "latest", "sha256:shared").unwrap();

        let store = Store::from_conn(conn);
        let stable = store
            .get_package_version("ghcr.io", "test/pkg", "1.0.0")
            .unwrap()
            .expect("expected 1.0.0 tag");
        let latest = store
            .get_package_version("ghcr.io", "test/pkg", "latest")
            .unwrap()
            .expect("expected latest tag");

        assert_eq!(stable.digest, "sha256:shared");
        assert_eq!(latest.digest, "sha256:shared");
        assert_eq!(stable.tag.as_deref(), Some("1.0.0"));
        assert_eq!(latest.tag.as_deref(), Some("latest"));
    }

    // r[verify db.package-versions.list]
    // r[verify db.package-versions.get]
    // r[verify db.package-detail]
    #[test]
    fn rich_package_queries_return_not_found_for_unknown_repository() {
        let conn = setup_test_db();
        let store = Store::from_conn(conn);

        let versions = store
            .get_package_versions("ghcr.io", "does/not/exist")
            .unwrap();
        assert!(versions.is_empty());

        let version = store
            .get_package_version("ghcr.io", "does/not/exist", "1.0.0")
            .unwrap();
        assert!(version.is_none());

        let detail = store
            .get_package_detail("ghcr.io", "does/not/exist")
            .unwrap();
        assert!(detail.is_none());
    }

    #[test]
    fn parse_component_extern_name_full() {
        let ref_ = parse_component_extern_name("wasi:http/types@0.2.3");
        assert_eq!(ref_.package, "wasi:http");
        assert_eq!(ref_.interface.as_deref(), Some("types"));
        assert_eq!(ref_.version.as_deref(), Some("0.2.3"));
    }

    #[test]
    fn parse_component_extern_name_no_interface() {
        let ref_ = parse_component_extern_name("wasi:http@0.2.3");
        assert_eq!(ref_.package, "wasi:http");
        assert_eq!(ref_.interface, None);
        assert_eq!(ref_.version.as_deref(), Some("0.2.3"));
    }

    #[test]
    fn parse_component_extern_name_no_version() {
        let ref_ = parse_component_extern_name("wasi:http/types");
        assert_eq!(ref_.package, "wasi:http");
        assert_eq!(ref_.interface.as_deref(), Some("types"));
        assert_eq!(ref_.version, None);
    }

    #[test]
    fn parse_component_extern_name_plain() {
        let ref_ = parse_component_extern_name("my-import");
        assert_eq!(ref_.package, "my-import");
        assert_eq!(ref_.interface, None);
        assert_eq!(ref_.version, None);
    }

    #[test]
    fn parse_component_extern_name_prerelease_version() {
        let ref_ = parse_component_extern_name("wasi:cli/run@0.2.0-rc1");
        assert_eq!(ref_.package, "wasi:cli");
        assert_eq!(ref_.interface.as_deref(), Some("run"));
        assert_eq!(ref_.version.as_deref(), Some("0.2.0-rc1"));
    }

    #[test]
    fn extract_wit_imports_exports_returns_empty_for_non_wasm() {
        let (imports, exports) = extract_wit_imports_exports(b"not wasm", &(0..8), None);
        assert!(imports.is_empty());
        assert!(exports.is_empty());
    }

    #[test]
    fn extract_wit_imports_exports_returns_empty_for_out_of_range() {
        let (imports, exports) = extract_wit_imports_exports(b"short", &(0..100), None);
        assert!(imports.is_empty());
        assert!(exports.is_empty());
    }

    #[test]
    fn extract_component_metadata_returns_none_for_invalid_bytes() {
        let (name, desc, json) = extract_component_metadata(b"not wasm");
        assert!(name.is_none());
        assert!(desc.is_none());
        assert!(json.is_none());
    }

    /// Read the sample-wasi-http-rust component fixture if available.
    fn read_sample_component() -> Option<Vec<u8>> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/1-hello-world/vendor/wasm/ghcr-io-bytecodealliance-sample-wasi-http-rust-sample-wasi-http-rust-0.1.6-b33f82f30175.wasm");
        std::fs::read(path).ok()
    }

    #[test]
    fn extract_component_metadata_from_real_binary() {
        let Some(bytes) = read_sample_component() else {
            eprintln!("skipping: sample component not found");
            return;
        };

        let (name, _desc, json) = extract_component_metadata(&bytes);

        // Root component may or may not have an embedded name.
        let _ = name;

        // JSON should be present and deserializable.
        let json = json.expect("metadata JSON should be present");
        let summary: wasm_meta_registry_types::ComponentSummary =
            serde_json::from_str(&json).expect("JSON should deserialize");

        // Should be a component.
        assert_eq!(summary.kind.as_deref(), Some("component"));

        // Should have children (modules + possibly inner components).
        assert!(
            !summary.children.is_empty(),
            "root component should have children"
        );

        // At least one child should be a module.
        let modules: Vec<_> = summary
            .children
            .iter()
            .filter(|c| c.kind.as_deref() == Some("module"))
            .collect();
        assert!(!modules.is_empty(), "should have at least one module child");

        // The main module should have a name.
        let main_module = modules
            .iter()
            .find(|m| m.name.as_deref() == Some("sample_wasi_http_rust.wasm"));
        assert!(main_module.is_some(), "should have the main Rust module");

        // The main module should report languages.
        let main = main_module.unwrap();
        assert!(
            main.languages.contains(&"Rust".to_string()),
            "main module should list Rust as a language"
        );

        // The main module should have producers.
        assert!(
            !main.producers.is_empty(),
            "main module should have producers"
        );
        assert!(
            main.producers.iter().any(|p| p.name == "rustc"),
            "main module should have rustc as a producer"
        );

        // Root component should have producers (wit-component, cargo-component).
        assert!(
            !summary.producers.is_empty(),
            "root component should have producers"
        );
        assert!(
            summary.producers.iter().any(|p| p.name == "wit-component"),
            "root should have wit-component producer"
        );

        // Root component should have WIT imports.
        assert!(
            !summary.imports.is_empty(),
            "root component should have WIT imports"
        );

        // Should import wasi:http interfaces.
        assert!(
            summary.imports.iter().any(|i| i.package == "wasi:http"),
            "should import wasi:http"
        );

        // Should have a size.
        assert!(summary.size_bytes.is_some(), "should have a size");
    }

    #[test]
    fn component_metadata_round_trips_through_store() {
        let Some(bytes) = read_sample_component() else {
            eprintln!("skipping: sample component not found");
            return;
        };

        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        // Extract and store metadata (simulating what try_extract_wit_package does).
        let (comp_name, comp_desc, producers_json) = extract_component_metadata(&bytes);
        WasmComponent::insert(
            &conn,
            manifest_id,
            None,
            comp_name.as_deref(),
            comp_desc.as_deref(),
            producers_json.as_deref(),
        )
        .unwrap();

        // Query it back via the store.
        let store = Store::from_conn(conn);
        let components = store.get_components_for_manifest(manifest_id).unwrap();

        assert_eq!(components.len(), 1, "should have one component");
        let comp = &components[0];

        // Should have children.
        assert!(
            !comp.children.is_empty(),
            "queried component should have children"
        );

        // Should have producers.
        assert!(
            !comp.producers.is_empty(),
            "queried component should have producers"
        );

        // Should have WIT imports.
        assert!(
            !comp.imports.is_empty(),
            "queried component should have WIT imports"
        );

        // Children should be preserved.
        let modules: Vec<_> = comp
            .children
            .iter()
            .filter(|c| c.kind.as_deref() == Some("module"))
            .collect();
        assert!(
            !modules.is_empty(),
            "queried component should have module children"
        );

        // Child producers should be preserved.
        let main = modules
            .iter()
            .find(|m| m.name.as_deref() == Some("sample_wasi_http_rust.wasm"));
        assert!(main.is_some(), "main module should survive round-trip");
        assert!(
            !main.unwrap().producers.is_empty(),
            "child producers should survive round-trip"
        );
    }
}
