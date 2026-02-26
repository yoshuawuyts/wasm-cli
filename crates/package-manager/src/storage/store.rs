use anyhow::Context;
use std::collections::HashSet;
use std::path::Path;

use super::config::StateInfo;
use super::models::{ImageEntry, InsertResult, KnownPackage, Migrations, TagType, WitInterface};
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
        let store_dir = data_dir.join("store");
        let db_dir = data_dir.join("db");
        let metadata_file = db_dir.join("metadata.db3");

        // TODO: remove me once we're done testing
        // tokio::fs::remove_dir_all(&data_dir).await?;

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
        let state_info = StateInfo::new_at(data_dir, migration_info, store_size, metadata_size);

        let store = Self { state_info, conn };

        // Re-scan known package tags after migrations to ensure derived data is up-to-date
        // Suppress errors as they shouldn't prevent the store from opening
        if let Err(e) = store.rescan_known_package_tags() {
            eprintln!("Warning: Failed to re-scan known package tags: {}", e);
        }

        Ok(store)
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

        let (result, image_id) = ImageEntry::insert(
            &self.conn,
            reference.registry(),
            reference.repository(),
            reference.tag(),
            digest.as_deref(),
            &manifest_str,
            size_on_disk,
            "component",
        )?;

        let manifest = image.manifest.clone();

        // Only store layers if this is a new entry
        if result == InsertResult::Inserted {
            // Store layers by their content digest (content-addressable storage)
            // The manifest.layers and image.layers should be in the same order
            if let Some(ref manifest) = image.manifest {
                for (idx, layer) in image.layers.iter().enumerate() {
                    let cache = self.state_info.store_dir();
                    // Use the layer's content digest from the manifest as the key
                    let fallback_key = reference.whole().to_string();
                    let key = manifest
                        .layers
                        .get(idx)
                        .map(|l| l.digest.as_str())
                        .unwrap_or(&fallback_key);
                    let data = &layer.data;
                    let _integrity = cacache::write(&cache, key, data).await?;

                    // Try to extract WIT interface from this layer
                    if let Some(image_id) = image_id {
                        self.try_extract_wit_interface(image_id, data);
                    }
                }
            }
        }
        Ok((result, digest, manifest))
    }

    /// Insert only the metadata (SQLite entry) for an image, without storing layers.
    ///
    /// Returns the insert result and the optional image ID.
    pub(crate) fn insert_metadata(
        &self,
        reference: &Reference,
        digest: Option<&str>,
        manifest: &OciImageManifest,
        size_on_disk: u64,
    ) -> anyhow::Result<(InsertResult, Option<i64>)> {
        let manifest_str = serde_json::to_string(manifest)?;
        ImageEntry::insert(
            &self.conn,
            reference.registry(),
            reference.repository(),
            reference.tag(),
            digest,
            &manifest_str,
            size_on_disk,
            "component",
        )
    }

    /// Insert a single layer into the content-addressable store.
    ///
    /// Optionally extracts WIT interface metadata if an `image_id` is provided.
    pub(crate) async fn insert_layer(
        &self,
        layer_digest: &str,
        data: &[u8],
        image_id: Option<i64>,
    ) -> anyhow::Result<()> {
        let cache = self.state_info.store_dir();
        let _integrity = cacache::write(&cache, layer_digest, data).await?;

        if let Some(image_id) = image_id {
            self.try_extract_wit_interface(image_id, data);
        }

        Ok(())
    }

    /// Attempt to extract WIT interface from wasm component bytes.
    /// This is best-effort - if extraction fails, we log a warning and skip.
    fn try_extract_wit_interface(&self, image_id: i64, wasm_bytes: &[u8]) {
        let Some(metadata) = extract_wit_metadata(wasm_bytes) else {
            return; // Not a valid wasm component, skip
        };

        // Update the image's package_type based on the detected type
        let package_type = if crate::utils::is_wit_package(wasm_bytes) {
            "interface"
        } else {
            "component"
        };
        if let Err(e) = self.conn.execute(
            "UPDATE image SET package_type = ?1 WHERE id = ?2",
            (package_type, image_id),
        ) {
            eprintln!(
                "Warning: Failed to update package_type for image {}: {}",
                image_id, e
            );
        }

        // Insert the WIT interface
        let wit_id = match WitInterface::insert(
            &self.conn,
            &metadata.wit_text,
            metadata.package_name.as_deref(),
            Some(&metadata.world_name),
            metadata.import_count,
            metadata.export_count,
        ) {
            Ok(id) => id,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to insert WIT interface for image {}: {}",
                    image_id, e
                );
                return;
            }
        };

        // Link to image
        if let Err(e) = WitInterface::link_to_image(&self.conn, image_id, wit_id) {
            eprintln!(
                "Warning: Failed to link WIT interface {} to image {}: {}",
                wit_id, image_id, e
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
        // Get all images to find which layers are still needed
        let all_entries = ImageEntry::get_all(&self.conn)?;

        // Find the entry we're deleting to get its layer digests
        let entry_to_delete = all_entries.iter().find(|e| {
            e.ref_registry == reference.registry()
                && e.ref_repository == reference.repository()
                && e.ref_tag.as_deref() == reference.tag()
                && e.ref_digest.as_deref() == reference.digest()
        });

        if let Some(entry) = entry_to_delete {
            // Collect all layer digests from the entry we're deleting
            let layers_to_delete: HashSet<&str> = entry
                .manifest
                .layers
                .iter()
                .map(|l| l.digest.as_str())
                .collect();

            // Collect all layer digests from OTHER entries (excluding the one we're deleting)
            let layers_still_needed: HashSet<&str> = all_entries
                .iter()
                .filter(|e| {
                    !(e.ref_registry == reference.registry()
                        && e.ref_repository == reference.repository()
                        && e.ref_tag.as_deref() == reference.tag()
                        && e.ref_digest.as_deref() == reference.digest())
                })
                .flat_map(|e| e.manifest.layers.iter().map(|l| l.digest.as_str()))
                .collect();

            // Only delete layers that are not needed by other entries
            for layer_digest in layers_to_delete {
                if !layers_still_needed.contains(layer_digest) {
                    let _ = cacache::remove(self.state_info.store_dir(), layer_digest).await;
                }
            }
        }

        // Delete from database
        ImageEntry::delete_by_reference(
            &self.conn,
            reference.registry(),
            reference.repository(),
            reference.tag(),
            reference.digest(),
        )
    }

    /// Search for known packages by query string.
    /// Uses pagination with `offset` and `limit` parameters.
    pub(crate) fn search_known_packages(
        &self,
        query: &str,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<KnownPackage>> {
        KnownPackage::search(&self.conn, query, offset, limit)
    }

    /// Get all known packages.
    /// Uses pagination with `offset` and `limit` parameters.
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

    /// Re-scan known package tags to update derived data after migrations.
    /// This re-classifies tag types based on tag naming conventions:
    /// - Tags ending in ".sig" are classified as "signature"
    /// - Tags ending in ".att" are classified as "attestation"
    /// - All other tags are classified as "release"
    pub(crate) fn rescan_known_package_tags(&self) -> anyhow::Result<usize> {
        // Get all unique package IDs and their tags
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT kpt.known_package_id, kpt.tag 
             FROM known_package_tag kpt",
        )?;

        let tags: Vec<(i64, String)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut updated_count = 0;

        // Re-process each tag to ensure it has the correct tag_type
        for (package_id, tag) in tags {
            // Determine the correct tag type using existing logic
            let tag_type = TagType::from_tag(&tag).as_str();

            // Update the tag type if needed
            let rows_affected = self.conn.execute(
                "UPDATE known_package_tag 
                 SET tag_type = ?1 
                 WHERE known_package_id = ?2 AND tag = ?3 AND tag_type != ?1",
                (tag_type, package_id, &tag),
            )?;

            if rows_affected > 0 {
                updated_count += 1;
            }
        }

        Ok(updated_count)
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
