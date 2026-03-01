use docker_credential::DockerCredential;
use oci_client::Reference;
use oci_client::client::{ClientConfig, ClientProtocol, ImageData, SizedStream};
use oci_client::manifest::{OciDescriptor, OciImageIndex, OciImageManifest};
use oci_client::secrets::RegistryAuth;
use oci_wasm::WasmClient;

use crate::config::Config;

pub(crate) struct Client {
    inner: WasmClient,
    config: Config,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

impl Client {
    pub(crate) fn new(config: Config) -> Self {
        let client_config = ClientConfig {
            protocol: ClientProtocol::Https,
            ..Default::default()
        };
        let client = WasmClient::new(oci_client::Client::new(client_config));
        Self {
            inner: client,
            config,
        }
    }

    pub(crate) async fn pull(&self, reference: &Reference) -> anyhow::Result<ImageData> {
        let auth = resolve_auth(reference, &self.config)?;
        let image = self.inner.pull(reference, &auth).await?;
        Ok(image)
    }

    /// Fetches the manifest and config digest for a given reference.
    ///
    /// Returns the OCI image manifest and the content digest.
    pub(crate) async fn pull_manifest(
        &self,
        reference: &Reference,
    ) -> anyhow::Result<(OciImageManifest, String)> {
        let auth = resolve_auth(reference, &self.config)?;
        let (manifest, _config, digest) = self
            .inner
            .pull_manifest_and_config(reference, &auth)
            .await?;
        Ok((manifest, digest))
    }

    /// Streams a single layer from the registry.
    ///
    /// Returns a `SizedStream` that yields chunks of bytes and optionally
    /// provides the content length.
    pub(crate) async fn pull_layer_stream(
        &self,
        reference: &Reference,
        layer: &OciDescriptor,
    ) -> anyhow::Result<SizedStream> {
        let auth = resolve_auth(reference, &self.config)?;
        // Ensure auth is stored before calling pull_blob_stream
        self.inner
            .store_auth_if_needed(reference.resolve_registry(), &auth)
            .await;
        let stream = self.inner.pull_blob_stream(reference, layer).await?;
        Ok(stream)
    }

    /// Fetches all tags for a given reference from the registry.
    ///
    /// This method handles pagination automatically, fetching all available tags
    /// by making multiple requests if necessary.
    pub(crate) async fn list_tags(&self, reference: &Reference) -> anyhow::Result<Vec<String>> {
        let auth = resolve_auth(reference, &self.config)?;
        let mut all_tags = Vec::new();
        let mut last: Option<String> = None;

        loop {
            // Some registries return null for tags instead of an empty array,
            // which causes deserialization to fail. We handle this gracefully.
            let response = match self
                .inner
                .list_tags(reference, &auth, None, last.as_deref())
                .await
            {
                Ok(resp) => resp,
                Err(_) if all_tags.is_empty() => {
                    // First request failed, likely due to null tags - return empty
                    return Ok(Vec::new());
                }
                Err(_) => {
                    // Subsequent request failed, return what we have
                    break;
                }
            };

            if response.tags.is_empty() {
                break;
            }

            last = response.tags.last().cloned();
            all_tags.extend(response.tags);

            // If we got fewer tags than a typical page size, we're done
            // The API doesn't provide a "next" link, so we detect the end
            // by checking if the last tag changed
            if last.is_none() {
                break;
            }

            // Make another request to check if there are more tags
            let next_response = match self
                .inner
                .list_tags(reference, &auth, Some(1), last.as_deref())
                .await
            {
                Ok(resp) => resp,
                Err(_) => break,
            };

            if next_response.tags.is_empty() {
                break;
            }
        }

        Ok(all_tags)
    }

    /// Fetches referrers (signatures, SBOMs, attestations) for a given reference.
    ///
    /// Returns the OCI image index listing all referrer manifests. If the
    /// registry does not support the Referrers API, returns `Ok(None)`.
    pub(crate) async fn pull_referrers(
        &self,
        reference: &Reference,
    ) -> anyhow::Result<Option<OciImageIndex>> {
        let auth = resolve_auth(reference, &self.config)?;
        self.inner
            .store_auth_if_needed(reference.resolve_registry(), &auth)
            .await;

        match self.inner.pull_referrers(reference, None).await {
            Ok(index) => Ok(Some(index)),
            // Registry may not support the Referrers API — log and skip.
            Err(e) => {
                tracing::debug!("Referrers API unavailable for {}: {}", reference, e);
                Ok(None)
            }
        }
    }
}

/// Resolve authentication for a registry reference.
///
/// The authentication is resolved in the following order:
/// 1. Check if a credential helper is configured in the config file for this registry
/// 2. Fall back to Docker credential store
/// 3. Use anonymous access if no credentials are found
fn resolve_auth(reference: &Reference, config: &Config) -> anyhow::Result<RegistryAuth> {
    let registry = reference.resolve_registry();

    // First, check if a credential helper is configured in the config file
    if let Some((username, password)) = config.get_credentials(registry)? {
        return Ok(RegistryAuth::Basic(username, password));
    }

    // Fall back to Docker credential store
    // NOTE: copied approach from https://github.com/bytecodealliance/wasm-pkg-tools/blob/48c28825a7dfb585b3fe1d42be65fe73a17d84fe/crates/wkg/src/oci.rs#L59-L66
    let server_url = match registry {
        "index.docker.io" => "https://index.docker.io/v1/",
        other => other,
    };

    match docker_credential::get_credential(server_url) {
        Ok(DockerCredential::UsernamePassword(username, password)) => {
            Ok(RegistryAuth::Basic(username, password))
        }
        Ok(DockerCredential::IdentityToken(_)) => {
            Err(anyhow::anyhow!("identity tokens not supported"))
        }
        Err(_) => Ok(RegistryAuth::Anonymous),
    }
}
