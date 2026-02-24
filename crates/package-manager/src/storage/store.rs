use anyhow::Context;
use std::collections::HashSet;
use std::path::Path;

use super::config::StateInfo;
use super::models::{ImageEntry, InsertResult, KnownPackage, Migrations, TagType, WitInterface};
use super::wit_parser::extract_wit_metadata;
use futures_concurrency::prelude::*;
use oci_client::{Reference, client::ImageData};
use sea_orm::{Database, DatabaseConnection};

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
    conn: DatabaseConnection,
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

        let a = tokio::fs::create_dir_all(&data_dir);
        let b = tokio::fs::create_dir_all(&store_dir);
        let c = tokio::fs::create_dir_all(&db_dir);
        let _ = (a, b, c)
            .try_join()
            .await
            .context("Could not create config directories on disk")?;

        let db_url = format!("sqlite://{}?mode=rwc", metadata_file.display());
        let conn = Database::connect(&db_url).await?;
        Migrations::run_all(&conn).await?;

        let migration_info = Migrations::get(&conn).await?;
        let store_size = dir_size(&store_dir).await;
        let metadata_size = tokio::fs::metadata(&metadata_file)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let state_info = StateInfo::new_at(data_dir, migration_info, store_size, metadata_size);

        let store = Self { state_info, conn };

        // Re-scan known package tags after migrations to ensure derived data is up-to-date
        // Suppress errors as they shouldn't prevent the store from opening
        if let Err(e) = store.rescan_known_package_tags().await {
            eprintln!("Warning: Failed to re-scan known package tags: {}", e);
        }

        Ok(store)
    }

    pub(crate) async fn insert(
        &self,
        reference: &Reference,
        image: ImageData,
    ) -> anyhow::Result<InsertResult> {
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
        )
        .await?;

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
                        self.try_extract_wit_interface(image_id, data).await;
                    }
                }
            }
        }
        Ok(result)
    }

    /// Attempt to extract WIT interface from wasm component bytes.
    /// This is best-effort - if extraction fails, we silently skip.
    async fn try_extract_wit_interface(&self, image_id: i64, wasm_bytes: &[u8]) {
        let Some(metadata) = extract_wit_metadata(wasm_bytes) else {
            return; // Not a valid wasm component, skip
        };

        // Insert the WIT interface
        let wit_id = match WitInterface::insert(
            &self.conn,
            &metadata.wit_text,
            metadata.package_name.as_deref(),
            Some(&metadata.world_name),
            metadata.import_count,
            metadata.export_count,
        )
        .await
        {
            Ok(id) => id,
            Err(_) => return, // Failed to insert, skip
        };

        // Link to image
        let _ = WitInterface::link_to_image(&self.conn, image_id, wit_id).await;
    }

    /// Returns all currently stored images and their metadata.
    pub(crate) async fn list_all(&self) -> anyhow::Result<Vec<ImageEntry>> {
        ImageEntry::get_all(&self.conn).await
    }

    /// Deletes an image by its reference.
    /// Only removes cached layers if no other images reference them.
    pub(crate) async fn delete(&self, reference: &Reference) -> anyhow::Result<bool> {
        // Get all images to find which layers are still needed
        let all_entries = ImageEntry::get_all(&self.conn).await?;

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
        .await
    }

    /// Search for known packages by query string.
    pub(crate) async fn search_known_packages(
        &self,
        query: &str,
    ) -> anyhow::Result<Vec<KnownPackage>> {
        KnownPackage::search(&self.conn, query).await
    }

    /// Get all known packages.
    pub(crate) async fn list_known_packages(&self) -> anyhow::Result<Vec<KnownPackage>> {
        KnownPackage::get_all(&self.conn).await
    }

    /// Add or update a known package.
    pub(crate) async fn add_known_package(
        &self,
        registry: &str,
        repository: &str,
        tag: Option<&str>,
        description: Option<&str>,
    ) -> anyhow::Result<()> {
        KnownPackage::upsert(&self.conn, registry, repository, tag, description).await
    }

    /// Re-scan known package tags to update derived data after migrations.
    /// This re-classifies tag types based on tag naming conventions:
    /// - Tags ending in ".sig" are classified as "signature"
    /// - Tags ending in ".att" are classified as "attestation"
    /// - All other tags are classified as "release"
    pub(crate) async fn rescan_known_package_tags(&self) -> anyhow::Result<usize> {
        use crate::storage::entities::known_package_tag;
        use sea_orm::{
            ActiveModelTrait,
            ActiveValue::{Set, Unchanged},
            EntityTrait,
        };

        let tags = known_package_tag::Entity::find().all(&self.conn).await?;

        let mut updated_count = 0;

        // Re-process each tag to ensure it has the correct tag_type
        for tag_model in tags {
            let correct_type = TagType::from_tag(&tag_model.tag).as_str().to_string();

            // Update the tag type if needed
            if tag_model.tag_type != correct_type {
                let active_model = known_package_tag::ActiveModel {
                    id: Unchanged(tag_model.id),
                    tag_type: Set(correct_type),
                    ..Default::default()
                };
                active_model.update(&self.conn).await?;
                updated_count += 1;
            }
        }

        Ok(updated_count)
    }

    /// Get all WIT interfaces.
    #[allow(dead_code)]
    pub(crate) async fn list_wit_interfaces(&self) -> anyhow::Result<Vec<WitInterface>> {
        WitInterface::get_all(&self.conn).await
    }

    /// Get all WIT interfaces with their associated component references.
    pub(crate) async fn list_wit_interfaces_with_components(
        &self,
    ) -> anyhow::Result<Vec<(WitInterface, String)>> {
        WitInterface::get_all_with_images(&self.conn).await
    }
}
