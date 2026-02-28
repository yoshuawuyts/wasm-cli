use anyhow::Context;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use super::config::StateInfo;
use super::models::{
    ImageEntry, InsertResult, KnownPackage, Migrations, OciLayer, OciManifest, OciRepository,
    OciTag, WitInterface,
};
use super::wit_parser::extract_wit_metadata;
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
            migration_info,
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
        let digest = reference.digest().map(|s| s.to_owned()).or(image.digest);
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
                let fallback_key = reference.whole().to_string();
                let layer_digest = manifest
                    .layers
                    .get(idx)
                    .map(|l| l.digest.as_str())
                    .unwrap_or(&fallback_key);
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
        position: i32,
    ) -> anyhow::Result<()> {
        let cache = self.state_info.store_dir();
        let _integrity = cacache::write(&cache, layer_digest, data).await?;

        if let Some(manifest_id) = manifest_id {
            let layer_id = OciLayer::insert(
                &self.conn,
                manifest_id,
                layer_digest,
                None,
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

        if let Err(e) = WitInterface::insert(
            &self.conn,
            package_name,
            version,
            None,
            Some(&metadata.wit_text),
            Some(manifest_id),
            layer_id,
        ) {
            tracing::warn!(
                "Failed to insert WIT interface for manifest {}: {}",
                manifest_id,
                e
            );
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
        let mut layers_still_needed: HashSet<String> = HashSet::new();
        for other in &all_manifests {
            if manifest_ids.contains(&other.id()) {
                continue;
            }
            if let Ok(other_layers) = OciLayer::list_by_manifest(&self.conn, other.id()) {
                for l in other_layers {
                    layers_still_needed.insert(l.digest);
                }
            }
        }

        // Remove cached layers that are no longer needed
        for layer_digest in &layer_digests {
            if !layers_still_needed.contains(layer_digest) {
                let _ = cacache::remove(self.state_info.store_dir(), layer_digest).await;
            }
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
    pub(crate) fn set_sync_meta(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO _sync_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (key, value),
        )?;
        Ok(())
    }
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
}
