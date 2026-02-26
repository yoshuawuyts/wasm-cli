use oci_client::Reference;

use crate::config::Config;
use crate::network::Client;
use crate::storage::{ImageEntry, InsertResult, KnownPackage, StateInfo, Store, WitInterface};

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
    pub async fn pull(&self, reference: Reference) -> anyhow::Result<InsertResult> {
        if self.offline {
            anyhow::bail!("cannot pull packages in offline mode");
        }

        let image = self.client.pull(&reference).await?;
        let result = self.store.insert(&reference, image).await?;

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

        Ok(result)
    }

    /// List all stored images and their metadata.
    pub fn list_all(&self) -> anyhow::Result<Vec<ImageEntry>> {
        self.store.list_all()
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
    pub fn search_packages(&self, query: &str) -> anyhow::Result<Vec<KnownPackage>> {
        self.store.search_known_packages(query)
    }

    /// Get all known packages.
    pub fn list_known_packages(&self) -> anyhow::Result<Vec<KnownPackage>> {
        self.store.list_known_packages()
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
        let known_packages = self.store.list_known_packages()?;
        let tags: Vec<String> = known_packages
            .into_iter()
            .filter(|pkg| {
                pkg.registry == reference.registry() && pkg.repository == reference.repository()
            })
            .flat_map(|pkg| {
                // Combine all tag types: release, signature, and attestation
                pkg.tags
                    .into_iter()
                    .chain(pkg.signature_tags)
                    .chain(pkg.attestation_tags)
            })
            .collect();
        Ok(tags)
    }

    /// Re-scan known package tags to update derived data (e.g., tag types).
    /// This should be called after migrations that affect tag classification logic
    /// (e.g., when tag type rules change from .sig/.att suffixes).
    /// Returns the number of tags that were updated.
    pub fn rescan_known_package_tags(&self) -> anyhow::Result<usize> {
        self.store.rescan_known_package_tags()
    }

    /// Get all WIT interfaces with their associated component references.
    pub fn list_wit_interfaces_with_components(
        &self,
    ) -> anyhow::Result<Vec<(WitInterface, String)>> {
        self.store.list_wit_interfaces_with_components()
    }
}
