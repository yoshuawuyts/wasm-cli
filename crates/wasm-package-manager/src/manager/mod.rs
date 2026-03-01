use oci_client::Reference;
use oci_client::manifest::OciImageManifest;
use std::path::Path;
use tokio_stream::StreamExt;

mod logic;

use crate::config::Config;
use crate::interfaces::WitInterfaceView;
use crate::oci::{Client, ImageView, InsertResult};
use crate::progress::ProgressEvent;
use crate::storage::{KnownPackageView, StateInfo, Store};

pub use logic::{derive_component_name, sanitize_to_wit_identifier, should_sync, vendor_filename};

/// Result of syncing the package index from a meta-registry.
#[derive(Debug)]
pub enum SyncResult {
    /// Sync was skipped because the minimum interval has not elapsed.
    Skipped,
    /// The server indicated the local data is still current (304 Not Modified).
    NotModified,
    /// New package data was fetched and stored locally.
    Updated {
        /// Number of packages that were synced.
        count: usize,
    },
    /// The sync failed but local cached data is available.
    Degraded {
        /// A human-readable description of the error.
        error: String,
    },
}

/// Controls whether `sync_from_meta_registry` respects the minimum sync
/// interval or forces an immediate fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPolicy {
    /// Only sync if the minimum interval has elapsed since the last sync.
    IfStale,
    /// Ignore the minimum interval and always contact the registry.
    Force,
}

/// Result of a pull operation.
///
/// Contains the insert result along with the content digest and manifest
/// from the pulled image.
#[derive(Debug, Clone)]
pub struct PullResult {
    /// Whether the image was newly inserted or already existed.
    pub insert_result: InsertResult,
    /// The content digest of the pulled image (e.g., "sha256:abc123...").
    pub digest: Option<String>,
    /// The OCI image manifest.
    pub manifest: Option<OciImageManifest>,
}

/// Result of an install operation.
///
/// Contains metadata about the installed package for updating
/// manifest and lockfile entries.
#[derive(Debug, Clone)]
pub struct InstallResult {
    /// The registry hostname (e.g., "ghcr.io").
    pub registry: String,
    /// The repository path (e.g., "webassembly/wasi-logging").
    pub repository: String,
    /// The tag, if present (e.g., "1.0.0").
    pub tag: Option<String>,
    /// The content digest of the image.
    pub digest: Option<String>,
    /// The WIT package name if available (e.g., "wasi:logging@0.1.0").
    pub package_name: Option<String>,
    /// The `org.opencontainers.image.title` manifest annotation, if present.
    pub oci_title: Option<String>,
    /// The list of vendored file paths.
    pub vendored_files: Vec<std::path::PathBuf>,
    /// Whether this package is a compiled component (`true`) or a WIT interface (`false`).
    pub is_component: bool,
}

/// Result of an add operation.
///
/// Contains metadata about the package for updating the manifest
/// without having pulled the actual layers.
#[derive(Debug, Clone)]
pub struct AddResult {
    /// The registry hostname (e.g., "ghcr.io").
    pub registry: String,
    /// The repository path (e.g., "webassembly/wasi-logging").
    pub repository: String,
    /// The tag, if present (e.g., "1.0.0").
    pub tag: Option<String>,
    /// The resolved dependency name for the manifest key.
    pub dep_name: String,
}

/// A cache on disk
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
            anyhow::bail!("cannot pull packages in offline mode");
        }

        let image = self.client.pull(&reference).await?;
        let (result, digest, manifest, manifest_id) = self.store.insert(&reference, image).await?;

        // Add to known packages when pulling (with tag if present)
        self.store.add_known_package(
            reference.registry(),
            reference.repository(),
            reference.tag(),
            None,
        )?;

        // Fetch all related tags and store them as known packages
        if let Ok(tags) = self.client.list_tags(&reference).await {
            for tag in tags {
                self.store.add_known_package(
                    reference.registry(),
                    reference.repository(),
                    Some(&tag),
                    None,
                )?;
            }
        }

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
            anyhow::bail!("cannot pull packages in offline mode");
        }

        // Fetch manifest and config
        let (manifest, digest) = self.client.pull_manifest(&reference).await?;

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

        // Fetch all related tags and store them as known packages
        if let Ok(tags) = self.client.list_tags(&reference).await {
            for tag in tags {
                self.store.add_known_package(
                    reference.registry(),
                    reference.repository(),
                    Some(&tag),
                    None,
                )?;
            }
        }

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
        use crate::interfaces::{extract_wit_metadata, is_wit_package};
        use crate::oci::filter_wasm_layers;

        let pull_result = self.pull(reference.clone()).await?;

        let mut vendored_files = Vec::new();
        let mut package_name = None;
        let mut is_component = true; // Default to component

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

                // Try to extract WIT package name and detect type from the layer data
                if package_name.is_none()
                    && let Ok(data) = self.get(&layer.digest).await
                {
                    is_component = !is_wit_package(&data);
                    if let Some(metadata) = extract_wit_metadata(&data) {
                        package_name = metadata.package_name;
                    }
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
        use crate::interfaces::{extract_wit_metadata, is_wit_package};
        use crate::oci::filter_wasm_layers;

        let pull_result = self
            .pull_with_progress(reference.clone(), progress_tx)
            .await?;

        let mut vendored_files = Vec::new();
        let mut package_name = None;
        let mut is_component = true; // Default to component

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

                // Try to extract WIT package name and detect type from the layer data
                if package_name.is_none()
                    && let Ok(data) = self.get(&layer.digest).await
                {
                    is_component = !is_wit_package(&data);
                    if let Some(metadata) = extract_wit_metadata(&data) {
                        package_name = metadata.package_name;
                    }
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
        })
    }
    /// Add a package reference to the manifest without pulling layers.
    ///
    /// Resolves the dependency name using the following priority:
    /// 1. Explicit `name_override` (the `--name` CLI flag).
    /// 2. Local search index (known packages in the database).
    /// 3. OCI `org.opencontainers.image.title` annotation (manifest-only fetch).
    /// 4. Last segment of the repository path (fallback).
    ///
    /// # Errors
    ///
    /// Returns an error if the manifest fetch fails when online, or if
    /// offline mode is enabled and no local metadata is available to derive
    /// a name.
    pub async fn add(
        &self,
        reference: &Reference,
        name_override: Option<&str>,
        existing_names: &std::collections::HashSet<String>,
    ) -> anyhow::Result<AddResult> {
        let registry = reference.registry().to_string();
        let repository = reference.repository().to_string();
        let tag = reference.tag().map(str::to_string);

        // 1. Explicit name override.
        if let Some(name) = name_override {
            return Ok(AddResult {
                registry,
                repository,
                tag,
                dep_name: name.to_string(),
            });
        }

        // 2. Check the local search index for a known package name.
        if let Some(known) = self
            .store
            .get_known_package(reference.registry(), reference.repository())?
            && let Some(desc) = &known.description
            && let Some(sanitized) = sanitize_to_wit_identifier(desc)
        {
            return Ok(AddResult {
                registry,
                repository,
                tag,
                dep_name: sanitized,
            });
        }

        // 3. OCI annotation metadata (manifest-only, no layer fetch).
        if !self.offline
            && let Ok((manifest, _digest)) = self.client.pull_manifest(reference).await
            && let Some(title) = manifest
                .annotations
                .as_ref()
                .and_then(|a| a.get("org.opencontainers.image.title").cloned())
            && let Some(sanitized) = sanitize_to_wit_identifier(&title)
        {
            return Ok(AddResult {
                registry,
                repository,
                tag,
                dep_name: sanitized,
            });
        }

        // 4. Fall back to the repository name.
        let dep_name = derive_component_name(None, None, &repository, existing_names);

        Ok(AddResult {
            registry,
            repository,
            tag,
            dep_name,
        })
    }

    /// List all stored images and their metadata.
    pub fn list_all(&self) -> anyhow::Result<Vec<ImageView>> {
        Ok(self
            .store
            .list_all()?
            .into_iter()
            .map(ImageView::from)
            .collect())
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
    ) -> anyhow::Result<Vec<KnownPackageView>> {
        Ok(self
            .store
            .search_known_packages(query, offset, limit)?
            .into_iter()
            .map(KnownPackageView::from)
            .collect())
    }

    /// Get all known packages.
    /// Uses pagination with `offset` and `limit` parameters.
    pub fn list_known_packages(
        &self,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<KnownPackageView>> {
        Ok(self
            .store
            .list_known_packages(offset, limit)?
            .into_iter()
            .map(KnownPackageView::from)
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
        if let Some(pkg) = self
            .store
            .get_known_package(reference.registry(), reference.repository())?
        {
            // Combine all tag types: release, signature, and attestation
            let tags: Vec<String> = pkg
                .tags
                .into_iter()
                .chain(pkg.signature_tags)
                .chain(pkg.attestation_tags)
                .collect();
            Ok(tags)
        } else {
            Ok(Vec::new())
        }
    }

    /// Get a known package by registry and repository.
    pub fn get_known_package(
        &self,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<Option<KnownPackageView>> {
        Ok(self
            .store
            .get_known_package(registry, repository)?
            .map(KnownPackageView::from))
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
    pub async fn index_package(&self, reference: &Reference) -> anyhow::Result<KnownPackageView> {
        if self.offline {
            anyhow::bail!("cannot index packages in offline mode");
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
        self.store
            .get_known_package(reference.registry(), reference.repository())?
            .map(KnownPackageView::from)
            .ok_or_else(|| anyhow::anyhow!("failed to retrieve indexed package"))
    }

    /// Get all WIT interfaces with their associated component references.
    pub fn list_wit_interfaces_with_components(
        &self,
    ) -> anyhow::Result<Vec<(WitInterfaceView, String)>> {
        Ok(self
            .store
            .list_wit_interfaces_with_components()?
            .into_iter()
            .map(|(iface, s)| (WitInterfaceView::from(iface), s))
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
        use crate::network::registry_client::{FetchResult, RegistryClient};

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
            Err(e) => Err(anyhow::anyhow!(
                "{e}. No local data available. Please check your network connection and run 'wasm package sync' to fetch the package index."
            )),
        }
    }

    #[cfg(feature = "http-sync")]
    fn handle_update(
        &self,
        packages: &[crate::storage::KnownPackage],
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
