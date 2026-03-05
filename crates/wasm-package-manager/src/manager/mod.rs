use oci_client::Reference;
use std::path::Path;
use tokio_stream::StreamExt;

mod errors;
mod logic;
mod models;

use crate::config::Config;
use crate::oci::{Client, ImageEntry, InsertResult};
use crate::progress::ProgressEvent;
use crate::storage::{KnownPackage, StateInfo, Store};
use crate::types::WitPackage;

pub use errors::ManagerError;
pub use logic::{derive_component_name, sanitize_to_wit_identifier, should_sync, vendor_filename};
pub use models::{InstallResult, PullResult, SyncPolicy, SyncResult};

/// A cache on disk
///
/// # Example
///
/// ```no_run
/// use wasm_package_manager::manager::Manager;
///
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// let manager = Manager::open().await?;
/// let images = manager.list_all()?;
/// for image in &images {
///     println!("{}", image.reference());
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Manager {
    client: Client,
    store: Store,
    config: Config,
    offline: bool,
}

impl Manager {
    /// Create a new store at a location on disk.
    ///
    /// This may return an error if it fails to create the cache location on disk.
    /// Loads configuration from the default config location.
    pub async fn open() -> anyhow::Result<Self> {
        Self::open_with_offline(false).await
    }

    /// Create a new Manager at a location on disk with offline mode.
    ///
    /// When offline is true, network operations will fail with an error.
    /// This may return an error if it fails to create the cache location on disk.
    pub async fn open_offline() -> anyhow::Result<Self> {
        Self::open_with_offline(true).await
    }

    /// Create a new Manager with the specified offline mode.
    async fn open_with_offline(offline: bool) -> anyhow::Result<Self> {
        let config = Config::load()?;
        let client = Client::new(config.clone());
        let store = Store::open().await?;

        Ok(Self {
            client,
            store,
            config,
            offline,
        })
    }

    /// Create a new store at a custom data directory on disk.
    ///
    /// This opens a separate cache at the specified path, isolated from the
    /// default location. Useful for running multiple instances (e.g. a
    /// registry server) without sharing state.
    ///
    /// This may return an error if it fails to create the cache location on disk.
    pub async fn open_at(data_dir: impl Into<std::path::PathBuf>) -> anyhow::Result<Self> {
        let config = Config::load()?;
        let client = Client::new(config.clone());
        let store = Store::open_at(data_dir).await?;

        Ok(Self {
            client,
            store,
            config,
            offline: false,
        })
    }

    /// Returns whether the manager is in offline mode.
    #[must_use]
    pub fn is_offline(&self) -> bool {
        self.offline
    }

    /// Create a new store with a specific configuration.
    ///
    /// This may return an error if it fails to create the cache location on disk.
    pub async fn with_config(config: Config) -> anyhow::Result<Self> {
        let client = Client::new(config.clone());
        let store = Store::open().await?;

        Ok(Self {
            client,
            store,
            config,
            offline: false,
        })
    }

    /// Pull a package from the registry.
    /// Returns the insert result indicating whether the package was newly inserted
    /// or already existed in the database.
    ///
    /// This method also fetches all related tags for the package and stores them
    /// as known packages for discovery purposes.
    ///
    /// # Errors
    ///
    /// Returns an error if offline mode is enabled.
    pub async fn pull(&self, reference: Reference) -> anyhow::Result<PullResult> {
        if self.offline {
            return Err(ManagerError::OfflinePull.into());
        }

        let image = self.client.pull(&reference).await?;

        // Validate the OCI bundle has exactly one WASM layer.
        if let Some(ref manifest) = image.manifest {
            crate::oci::validate_single_wasm_layer(&manifest.layers)?;
        }

        let (result, digest, manifest, manifest_id) = self.store.insert(&reference, image).await?;

        // Add to known packages when pulling (with tag if present)
        self.store.add_known_package(
            reference.registry(),
            reference.repository(),
            reference.tag(),
            None,
        )?;

        self.store_related_tags(&reference).await?;

        // Best-effort: discover and store referrers (signatures, SBOMs, etc.)
        if let Some(manifest_id) = manifest_id {
            self.try_store_referrers(&reference, manifest_id).await;
        }

        Ok(PullResult {
            insert_result: result,
            digest,
            manifest,
        })
    }

    /// Pull a package from the registry with per-layer progress reporting.
    ///
    /// This method streams layers individually and sends `ProgressEvent`s
    /// via the provided channel to enable progress bar rendering.
    ///
    /// # Errors
    ///
    /// Returns an error if offline mode is enabled or if any network/storage
    /// operation fails.
    pub async fn pull_with_progress(
        &self,
        reference: Reference,
        progress_tx: &tokio::sync::mpsc::Sender<ProgressEvent>,
    ) -> anyhow::Result<PullResult> {
        if self.offline {
            return Err(ManagerError::OfflinePull.into());
        }

        // Fetch manifest and config
        let (manifest, digest) = self.client.pull_manifest(&reference).await?;

        // Validate the OCI bundle has exactly one WASM layer.
        crate::oci::validate_single_wasm_layer(&manifest.layers)?;

        let layer_count = manifest.layers.len();
        let _ = progress_tx
            .send(ProgressEvent::ManifestFetched {
                layer_count,
                image_digest: digest.clone(),
            })
            .await;

        // Calculate total size from manifest layer descriptors
        let size_on_disk: u64 = manifest
            .layers
            .iter()
            .map(|l| u64::try_from(l.size.max(0)).unwrap_or(0))
            .sum();

        // Insert metadata into the database
        let (result, image_id) =
            self.store
                .insert_metadata(&reference, Some(&digest), &manifest, size_on_disk)?;

        if result == InsertResult::Inserted {
            // Stream and store each layer individually with progress
            for (index, layer_descriptor) in manifest.layers.iter().enumerate() {
                let total_bytes = if layer_descriptor.size > 0 {
                    Some(u64::try_from(layer_descriptor.size).unwrap_or(0))
                } else {
                    None
                };

                let _ = progress_tx
                    .send(ProgressEvent::LayerStarted {
                        index,
                        digest: layer_descriptor.digest.clone(),
                        total_bytes,
                        title: layer_descriptor
                            .annotations
                            .as_ref()
                            .and_then(|a| a.get("org.opencontainers.image.title").cloned()),
                        media_type: layer_descriptor.media_type.clone(),
                    })
                    .await;

                // Stream the layer data
                let mut stream = self
                    .client
                    .pull_layer_stream(&reference, layer_descriptor)
                    .await?;

                let mut layer_data = Vec::new();
                let mut bytes_downloaded: u64 = 0;

                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;
                    bytes_downloaded += u64::try_from(chunk.len()).unwrap_or(0);
                    layer_data.extend_from_slice(&chunk);

                    let _ = progress_tx
                        .send(ProgressEvent::LayerProgress {
                            index,
                            bytes_downloaded,
                        })
                        .await;
                }

                let _ = progress_tx
                    .send(ProgressEvent::LayerDownloaded { index })
                    .await;

                // Store the layer (with annotations from the descriptor)
                self.store
                    .insert_layer(
                        &layer_descriptor.digest,
                        &layer_data,
                        image_id,
                        Some(layer_descriptor.media_type.as_str()),
                        i32::try_from(index).unwrap_or(i32::MAX),
                        layer_descriptor.annotations.as_ref(),
                    )
                    .await?;

                let _ = progress_tx.send(ProgressEvent::LayerStored { index }).await;
            }
        } else {
            // Package already cached — show layers as completed
            for (index, layer_descriptor) in manifest.layers.iter().enumerate() {
                let total_bytes = if layer_descriptor.size > 0 {
                    Some(u64::try_from(layer_descriptor.size).unwrap_or(0))
                } else {
                    None
                };

                let _ = progress_tx
                    .send(ProgressEvent::LayerStarted {
                        index,
                        digest: layer_descriptor.digest.clone(),
                        total_bytes,
                        title: layer_descriptor
                            .annotations
                            .as_ref()
                            .and_then(|a| a.get("org.opencontainers.image.title").cloned()),
                        media_type: layer_descriptor.media_type.clone(),
                    })
                    .await;

                let _ = progress_tx.send(ProgressEvent::LayerStored { index }).await;
            }
        }

        // Add to known packages when pulling (with tag if present)
        self.store.add_known_package(
            reference.registry(),
            reference.repository(),
            reference.tag(),
            None,
        )?;

        self.store_related_tags(&reference).await?;

        // Best-effort: discover and store referrers (signatures, SBOMs, etc.)
        if let Some(manifest_id) = image_id {
            self.try_store_referrers(&reference, manifest_id).await;
        }

        Ok(PullResult {
            insert_result: result,
            digest: Some(digest),
            manifest: Some(manifest),
        })
    }

    /// Hard-link a cached layer to a destination path.
    ///
    /// Uses `cacache::hard_link` to create a hard-link from the global cache
    /// to the specified destination, saving disk space.
    ///
    /// # Errors
    ///
    /// Returns an error if the hard-link operation fails (e.g., layer not
    /// found in cache, or destination path is invalid).
    pub async fn vendor(&self, layer_digest: &str, dest: &Path) -> anyhow::Result<()> {
        cacache::hard_link(self.store.state_info.store_dir(), layer_digest, dest).await?;
        Ok(())
    }

    /// Install a package from the registry.
    ///
    /// This high-level method:
    /// 1. Pulls the package from the registry (or uses the cache)
    /// 2. Filters the manifest's layers for `application/wasm` media type
    /// 3. Hard-links each wasm layer to the vendor directory
    /// 4. Returns an `InstallResult` with metadata for updating manifest/lockfile
    ///
    /// # Errors
    ///
    /// Returns an error if pulling, vendoring, or filesystem operations fail.
    pub async fn install(
        &self,
        reference: Reference,
        vendor_dir: &Path,
    ) -> anyhow::Result<InstallResult> {
        use crate::oci::filter_wasm_layers;

        let pull_result = self.pull(reference.clone()).await?;

        let mut vendored_files = Vec::new();
        let mut package_name = None;
        let mut is_component = true; // Default to component
        let mut dependencies = Vec::new();

        // Extract the OCI image.title annotation from the manifest.
        let oci_title = pull_result
            .manifest
            .as_ref()
            .and_then(|m| m.annotations.as_ref())
            .and_then(|a| a.get("org.opencontainers.image.title").cloned());

        // Pre-compute vendor filename from the OCI reference and image digest.
        let digest_for_name = pull_result.digest.as_deref().unwrap_or("unknown");
        let filename = vendor_filename(
            reference.registry(),
            reference.repository(),
            reference.tag(),
            digest_for_name,
        );

        if let Some(ref manifest) = pull_result.manifest {
            for layer in filter_wasm_layers(&manifest.layers) {
                let dest = vendor_dir.join(&filename);

                // Ensure vendor directory exists
                tokio::fs::create_dir_all(vendor_dir).await?;

                // Remove existing file if present (hard-link requires non-existent target)
                let _ = tokio::fs::remove_file(&dest).await;

                self.vendor(&layer.digest, &dest).await?;
                vendored_files.push(dest);

                if package_name.is_none() {
                    self.try_extract_layer_metadata(
                        &layer.digest,
                        &mut package_name,
                        &mut is_component,
                        &mut dependencies,
                    )
                    .await;
                }
            }
        }

        Ok(InstallResult {
            registry: reference.registry().to_string(),
            repository: reference.repository().to_string(),
            tag: reference.tag().map(str::to_string),
            digest: pull_result.digest,
            package_name,
            oci_title,
            vendored_files,
            is_component,
            dependencies,
        })
    }

    /// Install a package from the registry with per-layer progress reporting.
    ///
    /// Like [`install`](Self::install), but sends `ProgressEvent`s via the provided
    /// channel to enable progress bar rendering in the CLI or TUI.
    ///
    /// # Errors
    ///
    /// Returns an error if pulling, vendoring, or filesystem operations fail.
    pub async fn install_with_progress(
        &self,
        reference: Reference,
        vendor_dir: &Path,
        progress_tx: &tokio::sync::mpsc::Sender<ProgressEvent>,
    ) -> anyhow::Result<InstallResult> {
        use crate::oci::filter_wasm_layers;

        let pull_result = self
            .pull_with_progress(reference.clone(), progress_tx)
            .await?;

        let mut vendored_files = Vec::new();
        let mut package_name = None;
        let mut is_component = true; // Default to component
        let mut dependencies = Vec::new();

        // Extract the OCI image.title annotation from the manifest.
        let oci_title = pull_result
            .manifest
            .as_ref()
            .and_then(|m| m.annotations.as_ref())
            .and_then(|a| a.get("org.opencontainers.image.title").cloned());

        // Pre-compute vendor filename from the OCI reference and image digest.
        let digest_for_name = pull_result.digest.as_deref().unwrap_or("unknown");
        let filename = vendor_filename(
            reference.registry(),
            reference.repository(),
            reference.tag(),
            digest_for_name,
        );

        if let Some(ref manifest) = pull_result.manifest {
            for layer in filter_wasm_layers(&manifest.layers) {
                let dest = vendor_dir.join(&filename);

                // Ensure vendor directory exists
                tokio::fs::create_dir_all(vendor_dir).await?;

                // Remove existing file if present (hard-link requires non-existent target)
                let _ = tokio::fs::remove_file(&dest).await;

                self.vendor(&layer.digest, &dest).await?;
                vendored_files.push(dest);

                if package_name.is_none() {
                    self.try_extract_layer_metadata(
                        &layer.digest,
                        &mut package_name,
                        &mut is_component,
                        &mut dependencies,
                    )
                    .await;
                }
            }
        }

        let _ = progress_tx.send(ProgressEvent::InstallComplete).await;

        Ok(InstallResult {
            registry: reference.registry().to_string(),
            repository: reference.repository().to_string(),
            tag: reference.tag().map(str::to_string),
            digest: pull_result.digest,
            package_name,
            oci_title,
            vendored_files,
            is_component,
            dependencies,
        })
    }

    /// List all stored images and their metadata.
    pub fn list_all(&self) -> anyhow::Result<Vec<ImageEntry>> {
        Ok(self
            .store
            .list_all()?
            .into_iter()
            .map(ImageEntry::from)
            .collect())
    }

    /// Resolve a WIT dependency to an OCI [`Reference`].
    ///
    /// Resolution order:
    /// 1. Exact match via `RawWitPackage::find_oci_reference()` (DB JOIN lookup).
    /// 2. Fuzzy match via `RawKnownPackage::search_by_wit_name()` (repository pattern).
    /// 3. Error with an actionable message.
    pub fn resolve_wit_dependency(
        &self,
        dep: &crate::types::DependencyItem,
    ) -> anyhow::Result<Option<Reference>> {
        // 1. Exact DB lookup: WIT package → OCI reference
        if let Some((registry, repository)) = self
            .store
            .find_oci_reference_by_wit_name(&dep.package, dep.version.as_deref())?
        {
            let tag = dep.version.as_deref().unwrap_or("latest");
            let ref_str = format!("{registry}/{repository}:{tag}");
            return Ok(Some(ref_str.parse()?));
        }

        // 2. Fallback: search known packages by WIT name
        if let Some(known) = self.store.search_known_package_by_wit_name(&dep.package)? {
            let tag = dep
                .version
                .as_deref()
                .or(known.tags.first().map(String::as_str))
                .unwrap_or("latest");
            let ref_str = format!("{}/{}:{}", known.registry, known.repository, tag);
            return Ok(Some(ref_str.parse()?));
        }

        // 3. Not resolvable
        Ok(None)
    }

    /// Get data from the store
    pub async fn get(&self, key: &str) -> cacache::Result<Vec<u8>> {
        cacache::read(self.store.state_info.store_dir(), key).await
    }

    /// Get information about the current state of the package manager.
    pub fn state_info(&self) -> StateInfo {
        self.store.state_info.clone()
    }

    /// Get the current configuration.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Delete an image from the store by its reference.
    pub async fn delete(&self, reference: Reference) -> anyhow::Result<bool> {
        self.store.delete(&reference).await
    }

    /// Search for known packages by query string.
    /// Searches in both registry and repository fields.
    /// Uses pagination with `offset` and `limit` parameters.
    pub fn search_packages(
        &self,
        query: &str,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<KnownPackage>> {
        Ok(self
            .store
            .search_known_packages(query, offset, limit)?
            .into_iter()
            .map(KnownPackage::from)
            .collect())
    }

    /// Get all known packages.
    /// Uses pagination with `offset` and `limit` parameters.
    pub fn list_known_packages(
        &self,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<KnownPackage>> {
        Ok(self
            .store
            .list_known_packages(offset, limit)?
            .into_iter()
            .map(KnownPackage::from)
            .collect())
    }

    /// Add or update a known package entry.
    pub fn add_known_package(
        &self,
        registry: &str,
        repository: &str,
        tag: Option<&str>,
        description: Option<&str>,
    ) -> anyhow::Result<()> {
        self.store
            .add_known_package(registry, repository, tag, description)
    }

    /// List all tags for a given reference from the registry.
    ///
    /// In offline mode, returns cached tags from the local database instead of
    /// fetching from the registry.
    pub async fn list_tags(&self, reference: &Reference) -> anyhow::Result<Vec<String>> {
        if self.offline {
            // Return cached tags from known packages
            return self.list_cached_tags(reference);
        }
        self.client.list_tags(reference).await
    }

    /// List tags from the local cache for a given reference.
    ///
    /// This is a private helper method used by `list_tags` when in offline mode.
    /// Returns all cached tags (release, signature, and attestation) for the given
    /// reference from the local known packages database.
    fn list_cached_tags(&self, reference: &Reference) -> anyhow::Result<Vec<String>> {
        // Use efficient lookup by registry and repository
        match self
            .store
            .get_known_package(reference.registry(), reference.repository())?
        {
            Some(pkg) => {
                // Combine all tag types: release, signature, and attestation
                let tags: Vec<String> = pkg
                    .tags
                    .into_iter()
                    .chain(pkg.signature_tags)
                    .chain(pkg.attestation_tags)
                    .collect();
                Ok(tags)
            }
            None => Ok(Vec::new()),
        }
    }

    /// Get a known package by registry and repository.
    pub fn get_known_package(
        &self,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<Option<KnownPackage>> {
        Ok(self
            .store
            .get_known_package(registry, repository)?
            .map(KnownPackage::from))
    }

    /// Index a package from the registry without downloading layers.
    ///
    /// Fetches the manifest and config to extract metadata (description from
    /// OCI annotations), lists all tags, and upserts into the known packages
    /// table. This is useful for building a search index without storing
    /// actual wasm content.
    ///
    /// # Errors
    ///
    /// Returns an error if offline mode is enabled or if network operations fail.
    pub async fn index_package(&self, reference: &Reference) -> anyhow::Result<KnownPackage> {
        if self.offline {
            return Err(ManagerError::OfflineIndex.into());
        }

        // Discover available tags first — the reference may not carry a valid
        // tag (e.g. the default "latest" might not exist).
        let tags = self.client.list_tags(reference).await?;
        anyhow::ensure!(
            !tags.is_empty(),
            "no tags found for {}/{}",
            reference.registry(),
            reference.repository()
        );

        // Pick the tag to use for pulling metadata: prefer the tag on the
        // reference if it exists in the remote, otherwise fall back to the
        // first available tag.
        let meta_tag = reference
            .tag()
            .filter(|t| tags.iter().any(|remote| remote == *t))
            .unwrap_or_else(|| tags.first().expect("tags verified non-empty"));

        // Build a reference with the chosen tag so we can pull its manifest.
        let meta_ref: Reference = format!(
            "{}/{}:{}",
            reference.registry(),
            reference.repository(),
            meta_tag
        )
        .parse()?;

        // Fetch manifest to extract metadata (e.g. description).
        let (manifest, _digest) = self.client.pull_manifest(&meta_ref).await?;
        let description = manifest
            .annotations
            .as_ref()
            .and_then(|a| a.get("org.opencontainers.image.description").cloned());

        // Store every discovered tag.
        for tag in &tags {
            self.store.add_known_package(
                reference.registry(),
                reference.repository(),
                Some(tag),
                description.as_deref(),
            )?;
        }

        // Return the indexed package.
        Ok(self
            .store
            .get_known_package(reference.registry(), reference.repository())?
            .map(KnownPackage::from)
            .ok_or(ManagerError::IndexRetrievalFailed)?)
    }

    /// Get all WIT interfaces with their associated component references.
    pub fn list_wit_packages_with_components(&self) -> anyhow::Result<Vec<(WitPackage, String)>> {
        Ok(self
            .store
            .list_wit_packages_with_components()?
            .into_iter()
            .map(|(wt, s)| (WitPackage::from(wt), s))
            .collect())
    }

    /// Sync the local package index from a meta-registry over HTTP.
    ///
    /// Checks the `_sync_meta` table for `last_synced_at` and skips the sync
    /// if less than `sync_interval` seconds have elapsed. Passes the cached
    /// ETag to the registry for conditional fetches.
    ///
    /// When `policy` is [`SyncPolicy::Force`], the minimum-interval check is
    /// skipped.
    ///
    /// # Errors
    ///
    /// Returns an error only when the sync fails **and** no cached data exists.
    /// When cached data exists but the sync fails, returns `SyncResult::Degraded`.
    #[cfg(feature = "http-sync")]
    pub async fn sync_from_meta_registry(
        &self,
        url: &str,
        sync_interval: u64,
        policy: SyncPolicy,
    ) -> anyhow::Result<SyncResult> {
        use wasm_meta_registry_client::{FetchResult, RegistryClient};

        // Check the minimum interval unless forced.
        if policy == SyncPolicy::IfStale {
            let last_synced_epoch = self
                .store
                .get_sync_meta("last_synced_at")?
                .and_then(|s| s.parse::<i64>().ok());
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .try_into()
                .unwrap_or(i64::MAX);
            if !should_sync(last_synced_epoch, sync_interval, now) {
                return Ok(SyncResult::Skipped);
            }
        }

        let etag = self.store.get_sync_meta("packages_etag")?;
        let client = RegistryClient::new(url);

        let has_cached_data = {
            let existing = self.store.list_known_packages(0, 1)?;
            !existing.is_empty()
        };

        match client.fetch_packages(etag.as_deref(), 1000).await {
            Ok(FetchResult::NotModified) => {
                self.update_last_synced_at()?;
                Ok(SyncResult::NotModified)
            }
            Ok(FetchResult::Updated { packages, etag }) => self.handle_update(&packages, etag),
            Err(e) if has_cached_data => Ok(SyncResult::Degraded {
                error: e.to_string(),
            }),
            Err(e) => Err(ManagerError::SyncNoLocalData {
                reason: e.to_string(),
            }
            .into()),
        }
    }

    #[cfg(feature = "http-sync")]
    fn handle_update(
        &self,
        packages: &[KnownPackage],
        etag: Option<String>,
    ) -> anyhow::Result<SyncResult> {
        let count = packages.len();
        // Bulk upsert all packages.
        for pkg in packages {
            let first_tag = pkg.tags.first().map(String::as_str);
            self.store.add_known_package(
                &pkg.registry,
                &pkg.repository,
                first_tag,
                pkg.description.as_deref(),
            )?;
            // Also add remaining tags.
            for tag in pkg.tags.iter().skip(1) {
                self.store.add_known_package(
                    &pkg.registry,
                    &pkg.repository,
                    Some(tag),
                    pkg.description.as_deref(),
                )?;
            }
        }
        if let Some(etag_val) = etag {
            self.store.set_sync_meta("packages_etag", &etag_val)?;
        }
        self.update_last_synced_at()?;
        Ok(SyncResult::Updated { count })
    }

    /// Update the `last_synced_at` timestamp in `_sync_meta`.
    #[cfg(feature = "http-sync")]
    fn update_last_synced_at(&self) -> anyhow::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.store.set_sync_meta("last_synced_at", &now.to_string())
    }

    /// Fetch all related tags for a reference and store them as known packages.
    ///
    /// Errors from the registry are silently ignored (best-effort).
    async fn store_related_tags(&self, reference: &Reference) -> anyhow::Result<()> {
        let Ok(tags) = self.client.list_tags(reference).await else {
            return Ok(());
        };
        for tag in tags {
            self.store.add_known_package(
                reference.registry(),
                reference.repository(),
                Some(&tag),
                None,
            )?;
        }
        Ok(())
    }

    /// Try to extract WIT metadata from a cached layer.
    ///
    /// On success, updates `package_name`, `is_component`, and `dependencies`
    /// in place. Silently skips if the layer data cannot be read or parsed.
    async fn try_extract_layer_metadata(
        &self,
        layer_digest: &str,
        package_name: &mut Option<String>,
        is_component: &mut bool,
        dependencies: &mut Vec<crate::types::DependencyItem>,
    ) {
        use crate::types::{extract_wit_metadata, is_wit_package};

        let Ok(data) = self.get(layer_digest).await else {
            return;
        };
        *is_component = !is_wit_package(&data);
        if let Some(metadata) = extract_wit_metadata(&data) {
            *package_name = metadata.package_name;
            *dependencies = metadata.dependencies;
        }
    }

    /// Best-effort: fetch and store referrers (signatures, SBOMs, attestations)
    /// for a manifest. Silently skips if the registry doesn't support the
    /// Referrers API or if any error occurs, but logs unexpected errors.
    async fn try_store_referrers(&self, reference: &Reference, manifest_id: i64) {
        let index = match self.client.pull_referrers(reference).await {
            Ok(Some(index)) => index,
            Ok(None) => return,
            Err(e) => {
                tracing::debug!(
                    "Failed to pull referrers for {}/{}: {}",
                    reference.registry(),
                    reference.repository(),
                    e
                );
                return;
            }
        };

        for entry in &index.manifests {
            // Use media_type as artifact_type — the oci-client ImageIndexEntry
            // does not expose a separate artifact_type field.
            if let Err(e) = self.store.store_referrer(
                manifest_id,
                reference.registry(),
                reference.repository(),
                &entry.digest,
                &entry.media_type,
            ) {
                tracing::warn!("Failed to store referrer {}: {}", entry.digest, e);
            }
        }
    }
}
