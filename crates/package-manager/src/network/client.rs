use docker_credential::DockerCredential;
use oci_client::Reference;
use oci_client::client::{ClientConfig, ClientProtocol, ImageData, SizedStream};
use oci_client::manifest::{OciDescriptor, OciImageManifest};
use oci_client::secrets::RegistryAuth;
use oci_wasm::WasmClient;

pub(crate) struct Client {
    inner: WasmClient,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

impl Client {
    pub(crate) fn new() -> Self {
        let config = ClientConfig {
            protocol: ClientProtocol::Https,
            ..Default::default()
        };
        let client = WasmClient::new(oci_client::Client::new(config));
        Self { inner: client }
    }

    pub(crate) async fn pull(&self, reference: &Reference) -> anyhow::Result<ImageData> {
        let auth = resolve_auth(reference)?;
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
        let auth = resolve_auth(reference)?;
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
        let auth = resolve_auth(reference)?;
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
        let auth = resolve_auth(reference)?;
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
}

fn resolve_auth(reference: &Reference) -> anyhow::Result<RegistryAuth> {
    // NOTE: copied approach from https://github.com/bytecodealliance/wasm-pkg-tools/blob/48c28825a7dfb585b3fe1d42be65fe73a17d84fe/crates/wkg/src/oci.rs#L59-L66
    let server_url = match reference.resolve_registry() {
        "index.docker.io" => "https://index.docker.io/v1/", // Default registry uses this key.
        other => other, // All other registries are keyed by their domain name without the `https://` prefix or any path suffix.
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
