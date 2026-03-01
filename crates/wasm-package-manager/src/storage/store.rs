#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use anyhow::Context;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use super::config::StateInfo;
use super::models::{KnownPackage, Migrations};
use crate::components::{ComponentTarget, WasmComponent};
use crate::interfaces::{
    WitInterface, WitInterfaceDependency, WitWorld, WitWorldExport, WitWorldImport,
    extract_wit_metadata,
};
use crate::oci::{ImageEntry, InsertResult, OciLayer, OciManifest, OciRepository, OciTag};
use futures_concurrency::prelude::*;
use oci_client::{Reference, client::ImageData, manifest::OciImageManifest};
use rusqlite::Connection;

/// Calculate the total size of a directory recursively
async fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(metadata) = entry.metadata().await {
                    if metadata.is_dir() {
                        stack.push(entry.path());
                    } else {
                        total += metadata.len();
                    }
                }
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

impl Store {
    /// Open the store and run any pending migrations.
    pub(crate) async fn open() -> anyhow::Result<Self> {
        let data_dir = dirs::data_local_dir()
            .context("No local data dir known for the current OS")?
            .join("wasm");
        let config_file = dirs::config_dir()
            .context("No config dir known for the current OS")?
            .join("wasm")
            .join("config.toml");
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

        let migration_info = Migrations::get(&conn)?;
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

    pub(crate) async fn insert(
        &self,
        reference: &Reference,
        image: ImageData,
    ) -> anyhow::Result<(InsertResult, Option<String>, Option<OciImageManifest>)> {
        let digest = reference.digest().map(str::to_owned).or(image.digest);
        let manifest_str = serde_json::to_string(&image.manifest)?;

        // Calculate total size on disk from all layers
        let size_on_disk: u64 = image.layers.iter().map(|l| l.data.len() as u64).sum();

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
            Some(size_on_disk as i64),
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

        // Only store layers if this is a new entry
        if result == InsertResult::Inserted
            && let Some(ref manifest) = image.manifest
        {
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
                    idx as i32,
                )?;

                self.try_extract_wit_interface(manifest_id, Some(layer_id), data);
            }
        }
        Ok((result, digest, manifest))
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
            Some(size_on_disk as i64),
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
    /// Optionally records the layer in `oci_layer` and extracts WIT interface
    /// metadata if a `manifest_id` is provided. The `position` specifies the
    /// layer's ordering within the manifest (0-based index).
    pub(crate) async fn insert_layer(
        &self,
        layer_digest: &str,
        data: &[u8],
        manifest_id: Option<i64>,
        media_type: Option<&str>,
        position: i32,
    ) -> anyhow::Result<()> {
        let cache = self.state_info.store_dir();
        let _integrity = cacache::write(&cache, layer_digest, data).await?;

        if let Some(manifest_id) = manifest_id {
            let layer_id = OciLayer::insert(
                &self.conn,
                manifest_id,
                layer_digest,
                media_type,
                Some(data.len() as i64),
                position,
            )?;
            self.try_extract_wit_interface(manifest_id, Some(layer_id), data);
        }

        Ok(())
    }

    /// Attempt to extract WIT interface from wasm component bytes.
    /// This is best-effort - if extraction fails, we log a warning and skip.
    fn try_extract_wit_interface(
        &self,
        manifest_id: i64,
        layer_id: Option<i64>,
        wasm_bytes: &[u8],
    ) {
        let Some(metadata) = extract_wit_metadata(wasm_bytes) else {
            return; // Not a valid wasm component, skip
        };

        // Insert the WIT interface (best-effort; skip if no package name)
        let Some(raw_name) = metadata.package_name.as_deref() else {
            return;
        };

        // Split "namespace:name@version" into (package_name, version).
        let (package_name, version) = split_package_version(raw_name);

        let wit_interface_id = match WitInterface::insert(
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
                    "Failed to insert WIT interface for manifest {}: {}",
                    manifest_id,
                    e
                );
                return;
            }
        };

        // Insert worlds, imports, and exports; collect world IDs for component targets
        let mut world_ids: HashMap<String, i64> = HashMap::new();
        for world in &metadata.worlds {
            let wit_world_id =
                match WitWorld::insert(&self.conn, wit_interface_id, &world.name, None) {
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
                    &item.declared_package,
                    item.declared_interface.as_deref(),
                    item.declared_version.as_deref(),
                    None,
                ) {
                    tracing::warn!("Failed to insert WIT world import: {}", e);
                }
            }

            for item in &world.exports {
                if let Err(e) = WitWorldExport::insert(
                    &self.conn,
                    wit_world_id,
                    &item.declared_package,
                    item.declared_interface.as_deref(),
                    item.declared_version.as_deref(),
                    None,
                ) {
                    tracing::warn!("Failed to insert WIT world export: {}", e);
                }
            }
        }

        // Insert interface dependencies
        for dep in &metadata.dependencies {
            if let Err(e) = WitInterfaceDependency::insert(
                &self.conn,
                wit_interface_id,
                &dep.declared_package,
                dep.declared_version.as_deref(),
                None,
            ) {
                tracing::warn!("Failed to insert WIT interface dependency: {}", e);
            }
        }

        // For compiled components, create wasm_component and component_target rows
        if metadata.is_component {
            let component_id =
                match WasmComponent::insert(&self.conn, manifest_id, layer_id, None, None) {
                    Ok(id) => id,
                    Err(e) => {
                        tracing::warn!("Failed to insert WasmComponent: {}", e);
                        return;
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
        self.try_resolve_foreign_keys(wit_interface_id, manifest_id);
    }

    /// Best-effort resolution of cross-package foreign keys.
    ///
    /// After inserting all worlds, imports, exports, and dependencies, attempt
    /// to resolve `resolved_interface_id` on import/export/dependency rows and
    /// `wit_world_id` on component_target rows by matching declared packages
    /// against existing `wit_interface` and `wit_world` rows.
    ///
    /// Resolution may fail if a dependency hasn't been pulled yet — this is
    /// expected. Future pulls can re-resolve.
    fn try_resolve_foreign_keys(&self, wit_interface_id: i64, manifest_id: i64) {
        if let Err(e) = resolve_import_foreign_keys(&self.conn, wit_interface_id) {
            tracing::warn!("Failed to resolve import foreign keys: {}", e);
        }
        if let Err(e) = resolve_export_foreign_keys(&self.conn, wit_interface_id) {
            tracing::warn!("Failed to resolve export foreign keys: {}", e);
        }
        if let Err(e) = resolve_dependency_foreign_keys(&self.conn, wit_interface_id) {
            tracing::warn!("Failed to resolve dependency foreign keys: {}", e);
        }
        if let Err(e) = resolve_component_target_foreign_keys(&self.conn, manifest_id) {
            tracing::warn!("Failed to resolve component target foreign keys: {}", e);
        }
    }

    /// Returns all currently stored images and their metadata.
    pub(crate) fn list_all(&self) -> anyhow::Result<Vec<ImageEntry>> {
        ImageEntry::get_all(&self.conn)
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
    ) -> anyhow::Result<Vec<KnownPackage>> {
        KnownPackage::search(&self.conn, query, offset, limit)
    }

    /// Get all known packages.
    pub(crate) fn list_known_packages(
        &self,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<KnownPackage>> {
        KnownPackage::get_all(&self.conn, offset, limit)
    }

    /// Get a known package by registry and repository.
    pub(crate) fn get_known_package(
        &self,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<Option<KnownPackage>> {
        KnownPackage::get(&self.conn, registry, repository)
    }

    /// Add or update a known package.
    pub(crate) fn add_known_package(
        &self,
        registry: &str,
        repository: &str,
        tag: Option<&str>,
        description: Option<&str>,
    ) -> anyhow::Result<()> {
        KnownPackage::upsert(&self.conn, registry, repository, tag, description)
    }

    /// Get all WIT interfaces.
    #[allow(dead_code)]
    pub(crate) fn list_wit_interfaces(&self) -> anyhow::Result<Vec<WitInterface>> {
        WitInterface::get_all(&self.conn)
    }

    /// Get all WIT interfaces with their associated component references.
    pub(crate) fn list_wit_interfaces_with_components(
        &self,
    ) -> anyhow::Result<Vec<(WitInterface, String)>> {
        WitInterface::get_all_with_images(&self.conn)
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
}

/// Resolve `wit_world_import.resolved_interface_id` for imports belonging to
/// the given `wit_interface_id` by matching `(declared_package, declared_version)`
/// against existing `wit_interface` rows.
fn resolve_import_foreign_keys(conn: &Connection, wit_interface_id: i64) -> anyhow::Result<usize> {
    let updated = conn.execute(
        "UPDATE wit_world_import
         SET resolved_interface_id = (
             SELECT wi.id FROM wit_interface wi
             WHERE wi.package_name = wit_world_import.declared_package
               AND COALESCE(wi.version, '') = COALESCE(wit_world_import.declared_version, '')
             LIMIT 1
         )
         WHERE wit_world_id IN (SELECT id FROM wit_world WHERE wit_interface_id = ?1)
           AND resolved_interface_id IS NULL",
        [wit_interface_id],
    )?;
    Ok(updated)
}

/// Resolve `wit_world_export.resolved_interface_id` for exports belonging to
/// the given `wit_interface_id`.
fn resolve_export_foreign_keys(conn: &Connection, wit_interface_id: i64) -> anyhow::Result<usize> {
    let updated = conn.execute(
        "UPDATE wit_world_export
         SET resolved_interface_id = (
             SELECT wi.id FROM wit_interface wi
             WHERE wi.package_name = wit_world_export.declared_package
               AND COALESCE(wi.version, '') = COALESCE(wit_world_export.declared_version, '')
             LIMIT 1
         )
         WHERE wit_world_id IN (SELECT id FROM wit_world WHERE wit_interface_id = ?1)
           AND resolved_interface_id IS NULL",
        [wit_interface_id],
    )?;
    Ok(updated)
}

/// Resolve `wit_interface_dependency.resolved_interface_id` for deps of the
/// given `wit_interface_id`.
fn resolve_dependency_foreign_keys(
    conn: &Connection,
    wit_interface_id: i64,
) -> anyhow::Result<usize> {
    let updated = conn.execute(
        "UPDATE wit_interface_dependency
         SET resolved_interface_id = (
             SELECT wi.id FROM wit_interface wi
             WHERE wi.package_name = wit_interface_dependency.declared_package
               AND COALESCE(wi.version, '') = COALESCE(wit_interface_dependency.declared_version, '')
             LIMIT 1
         )
         WHERE dependent_id = ?1
           AND resolved_interface_id IS NULL",
        [wit_interface_id],
    )?;
    Ok(updated)
}

/// Resolve `component_target.wit_world_id` for targets of components under
/// the given `manifest_id` by matching against `wit_world` + `wit_interface`.
fn resolve_component_target_foreign_keys(
    conn: &Connection,
    manifest_id: i64,
) -> anyhow::Result<usize> {
    let updated = conn.execute(
        "UPDATE component_target
         SET wit_world_id = (
             SELECT ww.id FROM wit_world ww
             JOIN wit_interface wi ON ww.wit_interface_id = wi.id
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

    #[test]
    fn wit_world_insert_and_query() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        let iface_id = WitInterface::insert(
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
        assert_eq!(found.wit_interface_id, iface_id);
    }

    #[test]
    fn wit_world_import_export_insert() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        let iface_id = WitInterface::insert(
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

    #[test]
    fn wit_interface_dependency_insert() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        let iface_id = WitInterface::insert(
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
            WitInterfaceDependency::insert(&conn, iface_id, "wasi:io", Some("0.2.0"), None)
                .unwrap();
        assert!(dep_id > 0);
    }

    #[test]
    fn wasm_component_and_target_insert() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);
        let layer_id =
            OciLayer::insert(&conn, manifest_id, "sha256:layer1", None, Some(100), 0).unwrap();

        // Create the WIT interface and world first
        let iface_id = WitInterface::insert(
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
            WasmComponent::insert(&conn, manifest_id, Some(layer_id), None, None).unwrap();
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

    #[test]
    fn no_component_rows_for_wit_only_package() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        // Insert interface and world (as we would for a WIT-only package)
        let iface_id = WitInterface::insert(
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

    #[test]
    fn wit_world_import_export_idempotent() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        let iface_id = WitInterface::insert(
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

    #[test]
    fn resolve_import_resolved_interface_id_when_dep_exists() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        // Create the dependency interface (wasi:io@0.2.0) first
        let dep_iface_id = WitInterface::insert(
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
        let main_iface_id = WitInterface::insert(
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

        // Insert an import with no resolved_interface_id
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

        // Verify the resolved_interface_id was set
        let resolved: Option<i64> = conn
            .query_row(
                "SELECT resolved_interface_id FROM wit_world_import WHERE wit_world_id = ?1",
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

    #[test]
    fn resolve_import_stays_null_when_dep_missing() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        let iface_id = WitInterface::insert(
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

        // Verify the resolved_interface_id is still NULL
        let resolved: Option<i64> = conn
            .query_row(
                "SELECT resolved_interface_id FROM wit_world_import WHERE wit_world_id = ?1",
                [world_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            resolved, None,
            "import should remain unresolved when dependency is not in DB"
        );
    }

    #[test]
    fn resolve_dependency_resolved_interface_id() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        // Create the dependency interface
        let dep_iface_id = WitInterface::insert(
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
        let main_iface_id = WitInterface::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        // Insert a dependency with no resolved_interface_id
        WitInterfaceDependency::insert(&conn, main_iface_id, "wasi:io", Some("0.2.0"), None)
            .unwrap();

        // Run the resolution pass
        resolve_dependency_foreign_keys(&conn, main_iface_id).unwrap();

        // Verify the resolved_interface_id was set
        let resolved: Option<i64> = conn
            .query_row(
                "SELECT resolved_interface_id FROM wit_interface_dependency WHERE dependent_id = ?1",
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

    #[test]
    fn resolve_export_resolved_interface_id() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn);

        // Create the target interface for the export
        let handler_iface_id = WitInterface::insert(
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

        // Insert an export with no resolved_interface_id
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

        // Verify the resolved_interface_id was set
        let resolved: Option<i64> = conn
            .query_row(
                "SELECT resolved_interface_id FROM wit_world_export WHERE wit_world_id = ?1",
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
        let iface_id = WitInterface::insert(
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
        let comp_id = WasmComponent::insert(&conn, comp_manifest_id, None, None, None).unwrap();
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
}
