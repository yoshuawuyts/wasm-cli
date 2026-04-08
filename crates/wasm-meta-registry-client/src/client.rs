//! HTTP client for querying and syncing packages from a meta-registry.
//!
//! Uses `wstd::http` when compiled for WASI p2 components and `reqwest` on
//! native targets (requires the **`client`** feature).

// r[impl frontend.api.callback]
// r[impl frontend.api.base-url]

use std::fmt;

use crate::KnownPackage;
use wasm_meta_registry_types::{PackageDetail, PackageVersion};

/// Default API base URL when no environment variable is set.
const DEFAULT_API_BASE_URL: &str = "http://localhost:8081";

/// An error returned when the meta-registry API is unreachable or returns
/// an unexpected response.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_client::ApiError;
///
/// let err = ApiError::new("connection refused");
/// assert_eq!(err.to_string(), "connection refused");
/// ```
#[derive(Debug)]
pub struct ApiError {
    message: String,
}

impl ApiError {
    /// Create a new API error with the given message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Result of fetching packages with ETag-based conditional requests.
///
/// Only available with the **`client`** feature.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_client::{KnownPackage, FetchResult};
///
/// let result = FetchResult::Updated {
///     packages: vec![KnownPackage {
///         registry: "ghcr.io".into(),
///         repository: "user/repo".into(),
///         kind: None,
///         description: None,
///         tags: vec!["v1.0".into()],
///         signature_tags: vec![],
///         attestation_tags: vec![],
///         last_seen_at: String::new(),
///         created_at: String::new(),
///         wit_namespace: None,
///         wit_name: None,
///         dependencies: vec![],
///     }],
///     etag: Some("\"abc123\"".into()),
/// };
///
/// if let FetchResult::Updated { packages, etag } = result {
///     assert_eq!(packages.len(), 1);
///     assert!(etag.is_some());
/// }
/// ```
#[cfg(feature = "client")]
#[derive(Debug)]
pub enum FetchResult {
    /// The server returned 304 Not Modified; local data is still fresh.
    NotModified,
    /// The server returned new data.
    Updated {
        /// The updated list of known packages.
        packages: Vec<KnownPackage>,
        /// The ETag header from the response, if present.
        etag: Option<String>,
    },
}

/// HTTP client for the meta-registry API.
///
/// Supports fetching recent packages, searching, pagination, and looking up
/// individual packages by WIT namespace and name. On native targets with the
/// **`client`** feature, also supports ETag-based conditional fetches with
/// exponential-backoff retries via [`fetch_packages`](Self::fetch_packages).
///
/// On native targets this uses `reqwest`; on `wasm32-wasip2` it uses
/// `wstd::http`.
///
/// # Example
///
/// ```no_run
/// use wasm_meta_registry_client::RegistryClient;
///
/// # async fn example() -> Result<(), wasm_meta_registry_client::ApiError> {
/// let client = RegistryClient::new("http://localhost:8081");
/// let packages = client.fetch_recent_packages(10).await?;
/// println!("got {} packages", packages.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct RegistryClient {
    base_url: String,
    #[cfg(all(target_os = "wasi", target_env = "p2"))]
    client: wstd::http::Client,
    #[cfg(not(all(target_os = "wasi", target_env = "p2")))]
    client: reqwest::Client,
}

impl RegistryClient {
    /// Create a new client with the given base URL.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            #[cfg(all(target_os = "wasi", target_env = "p2"))]
            client: wstd::http::Client::new(),
            #[cfg(not(all(target_os = "wasi", target_env = "p2")))]
            client: reqwest::Client::new(),
        }
    }

    /// Create a client using the API base URL.
    ///
    /// The URL is set at compile time via the `API_BASE_URL` environment
    /// variable. Falls back to `http://localhost:8081` when unset.
    #[must_use]
    pub fn from_env() -> Self {
        let base_url = option_env!("API_BASE_URL").unwrap_or(DEFAULT_API_BASE_URL);
        Self::new(base_url)
    }

    /// Fetch recently updated packages from the meta-registry.
    pub async fn fetch_recent_packages(&self, limit: u32) -> Result<Vec<KnownPackage>, ApiError> {
        let url = format!("{}/v1/packages/recent?limit={limit}", self.base_url);
        self.fetch_packages_from(&url).await
    }

    /// Search packages by query string.
    pub async fn search_packages(&self, query: &str) -> Result<Vec<KnownPackage>, ApiError> {
        let encoded_query = percent_encode_query_component(query);
        let url = format!("{}/v1/search?q={encoded_query}", self.base_url);
        self.fetch_packages_from(&url).await
    }

    /// Fetch all packages with pagination.
    pub async fn fetch_all_packages(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<KnownPackage>, ApiError> {
        let url = format!(
            "{}/v1/packages?offset={offset}&limit={limit}",
            self.base_url
        );
        self.fetch_packages_from(&url).await
    }

    /// Look up a package by its WIT namespace and name.
    ///
    /// Searches by WIT name and filters client-side for an exact match.
    /// Returns `Ok(None)` when the API is reachable but no match is found,
    /// and `Err` when the API itself fails.
    pub async fn fetch_package_by_wit(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<Option<KnownPackage>, ApiError> {
        let packages = self.search_packages(name).await?;
        Ok(packages.into_iter().find(|pkg| {
            pkg.wit_namespace.as_deref() == Some(namespace) && pkg.wit_name.as_deref() == Some(name)
        }))
    }

    // ================================================================
    // Rich API methods
    // ================================================================

    /// Fetch full detail for a package, including all versions and metadata.
    // r[verify client.detail]
    pub async fn fetch_package_detail(
        &self,
        registry: &str,
        repository: &str,
    ) -> Result<Option<PackageDetail>, ApiError> {
        let encoded_reg = percent_encode_query_component(registry);
        let encoded_repo = percent_encode_query_component(repository);
        let url = format!(
            "{}/v1/packages/detail/{encoded_reg}/{encoded_repo}",
            self.base_url
        );
        self.fetch_optional(&url).await
    }

    /// Fetch all versions of a package.
    // r[verify client.versions.list]
    pub async fn fetch_package_versions(
        &self,
        registry: &str,
        repository: &str,
    ) -> Result<Vec<PackageVersion>, ApiError> {
        let encoded_reg = percent_encode_query_component(registry);
        let encoded_repo = percent_encode_query_component(repository);
        let url = format!(
            "{}/v1/packages/versions/{encoded_reg}/{encoded_repo}",
            self.base_url
        );
        self.fetch_list(&url).await
    }

    /// Fetch a specific version of a package by tag.
    // r[verify client.versions.get]
    pub async fn fetch_package_version(
        &self,
        registry: &str,
        repository: &str,
        version: &str,
    ) -> Result<Option<PackageVersion>, ApiError> {
        let encoded_reg = percent_encode_query_component(registry);
        let encoded_repo = percent_encode_query_component(repository);
        let encoded_ver = percent_encode_query_component(version);
        let url = format!(
            "{}/v1/packages/version/{encoded_reg}/{encoded_ver}/{encoded_repo}",
            self.base_url
        );
        self.fetch_optional(&url).await
    }

    /// Search packages by imported interface.
    // r[verify client.search.by-import]
    pub async fn search_packages_by_import(
        &self,
        interface: &str,
    ) -> Result<Vec<KnownPackage>, ApiError> {
        let encoded = percent_encode_query_component(interface);
        let url = format!("{}/v1/search/by-import?interface={encoded}", self.base_url);
        self.fetch_packages_from(&url).await
    }

    /// Search packages by exported interface.
    // r[verify client.search.by-export]
    pub async fn search_packages_by_export(
        &self,
        interface: &str,
    ) -> Result<Vec<KnownPackage>, ApiError> {
        let encoded = percent_encode_query_component(interface);
        let url = format!("{}/v1/search/by-export?interface={encoded}", self.base_url);
        self.fetch_packages_from(&url).await
    }

    /// Fetch and deserialize a list of packages from the given URL.
    async fn fetch_packages_from(&self, url: &str) -> Result<Vec<KnownPackage>, ApiError> {
        let bytes = self.get(url).await?;
        serde_json::from_slice(&bytes).map_err(|e| {
            ApiError::new(format!(
                "received an unexpected response from the registry: {e}"
            ))
        })
    }

    /// Fetch and deserialize a list of items from the given URL.
    async fn fetch_list<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<Vec<T>, ApiError> {
        let bytes = self.get(url).await?;
        serde_json::from_slice(&bytes).map_err(|e| {
            ApiError::new(format!(
                "received an unexpected response from the registry: {e}"
            ))
        })
    }

    /// Fetch and deserialize a single item, returning `None` on 404.
    async fn fetch_optional<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<Option<T>, ApiError> {
        let Some(bytes) = self.get_with_status(url).await? else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes).map(Some).map_err(|e| {
            ApiError::new(format!(
                "received an unexpected response from the registry: {e}"
            ))
        })
    }

    /// Perform an HTTP GET request and return the raw response body.
    #[cfg(all(target_os = "wasi", target_env = "p2"))]
    async fn get(&self, url: &str) -> Result<Vec<u8>, ApiError> {
        use wstd::http::{Body, Request};

        let req = Request::get(url)
            .body(Body::empty())
            .map_err(|e| ApiError::new(format!("failed to build request for {url}: {e}")))?;

        let response =
            self.client.send(req).await.map_err(|e| {
                ApiError::new(format!("could not connect to the registry API: {e}"))
            })?;

        let mut body = response.into_body();
        let bytes = body
            .contents()
            .await
            .map_err(|e| ApiError::new(format!("failed to read response body: {e}")))?;
        Ok(bytes.to_vec())
    }

    /// Perform an HTTP GET request and return the raw response body.
    #[cfg(not(all(target_os = "wasi", target_env = "p2")))]
    async fn get(&self, url: &str) -> Result<Vec<u8>, ApiError> {
        let resp =
            self.client.get(url).send().await.map_err(|e| {
                ApiError::new(format!("could not connect to the registry API: {e}"))
            })?;

        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| ApiError::new(format!("failed to read response body: {e}")))
    }

    /// Perform an HTTP GET request, returning `None` for 404 responses.
    #[cfg(all(target_os = "wasi", target_env = "p2"))]
    async fn get_with_status(&self, url: &str) -> Result<Option<Vec<u8>>, ApiError> {
        use wstd::http::{Body, Request};

        let req = Request::get(url)
            .body(Body::empty())
            .map_err(|e| ApiError::new(format!("failed to build request for {url}: {e}")))?;

        let response =
            self.client.send(req).await.map_err(|e| {
                ApiError::new(format!("could not connect to the registry API: {e}"))
            })?;

        let status = response.status();
        if status == wstd::http::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let mut body = response.into_body();
        let bytes = body
            .contents()
            .await
            .map_err(|e| ApiError::new(format!("failed to read response body: {e}")))?;
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            return Err(ApiError::new(format!(
                "registry API returned unexpected status {status} for {url}: {body}"
            )));
        }
        Ok(Some(bytes.to_vec()))
    }

    /// Perform an HTTP GET request, returning `None` for 404 responses.
    #[cfg(not(all(target_os = "wasi", target_env = "p2")))]
    async fn get_with_status(&self, url: &str) -> Result<Option<Vec<u8>>, ApiError> {
        let resp =
            self.client.get(url).send().await.map_err(|e| {
                ApiError::new(format!("could not connect to the registry API: {e}"))
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ApiError::new(format!("failed to read response body: {e}")))?;
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            return Err(ApiError::new(format!(
                "registry API returned unexpected status {status} for {url}: {body}"
            )));
        }

        Ok(Some(bytes.to_vec()))
    }
}

// --- ETag-based sync (native only) -------------------------------------------

#[cfg(feature = "client")]
impl RegistryClient {
    /// Fetch all packages from the meta-registry with ETag support.
    ///
    /// Sends `If-None-Match` when an ETag is available. Retries up to 3 times
    /// with exponential backoff on transient errors.
    ///
    /// The `limit` controls the maximum number of packages to fetch per request.
    ///
    /// # Errors
    ///
    /// Returns an error if all retry attempts fail.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use wasm_meta_registry_client::{RegistryClient, FetchResult};
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let client = RegistryClient::new("http://localhost:8081");
    ///
    ///     // First fetch without an ETag.
    ///     let result = client.fetch_packages(None, 50).await?;
    ///     let etag = match result {
    ///         FetchResult::Updated { packages, etag } => {
    ///             println!("got {} packages", packages.len());
    ///             etag
    ///         }
    ///         FetchResult::NotModified => None,
    ///     };
    ///
    ///     // Subsequent fetch with the ETag for conditional update.
    ///     let _result = client.fetch_packages(etag.as_deref(), 50).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn fetch_packages(
        &self,
        etag: Option<&str>,
        limit: u32,
    ) -> anyhow::Result<FetchResult> {
        use std::time::Duration;

        use exponential_backoff::Backoff;

        let url = format!("{}/v1/packages?limit={limit}", self.base_url);
        let backoff = Backoff::new(3, Duration::from_millis(250), Duration::from_secs(5));

        let mut last_err: Option<anyhow::Error> = None;

        for duration in &backoff {
            match self.try_fetch(&url, etag).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_err = Some(e);
                    if let Some(d) = duration {
                        tokio::time::sleep(d).await;
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("failed to fetch packages from {url} after retries")
        }))
    }

    /// Single attempt to fetch packages with ETag support.
    async fn try_fetch(&self, url: &str, etag: Option<&str>) -> anyhow::Result<FetchResult> {
        let mut req = self.client.get(url);
        if let Some(etag_val) = etag {
            req = req.header(reqwest::header::IF_NONE_MATCH, etag_val);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("could not reach registry at {}: {e}", self.base_url))?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(FetchResult::NotModified);
        }

        if status.is_server_error() {
            anyhow::bail!(
                "registry at {} returned server error: {status}",
                self.base_url
            );
        }

        if !status.is_success() {
            anyhow::bail!(
                "registry at {} returned unexpected status: {status}",
                self.base_url
            );
        }

        let new_etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let packages: Vec<KnownPackage> = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("failed to parse response from {}: {e}", self.base_url))?;

        Ok(FetchResult::Updated {
            packages,
            etag: new_etag,
        })
    }
}

/// Percent-encode a query parameter component according to RFC 3986.
#[must_use]
fn percent_encode_query_component(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            write!(&mut encoded, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(all(target_os = "wasi", target_env = "p2")))]
    use std::io::{Read, Write};
    #[cfg(not(all(target_os = "wasi", target_env = "p2")))]
    use std::net::TcpListener;

    // r[verify frontend.api.base-url]
    #[test]
    fn from_env_uses_compile_time_or_default_base_url() {
        let client = RegistryClient::from_env();
        let expected = option_env!("API_BASE_URL").unwrap_or(DEFAULT_API_BASE_URL);
        assert_eq!(client.base_url, expected);
    }

    // r[verify frontend.api.callback]
    #[test]
    fn percent_encoding_escapes_query_parameter_delimiters() {
        let query = "name with spaces & ? /";
        assert_eq!(
            percent_encode_query_component(query),
            "name%20with%20spaces%20%26%20%3F%20%2F"
        );
    }

    #[cfg(not(all(target_os = "wasi", target_env = "p2")))]
    fn spawn_single_response_server(status_line: &str, body: &str, content_type: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let addr = listener.local_addr().expect("get listener addr");
        let status = status_line.to_string();
        let body = body.to_string();
        let content_type = content_type.to_string();

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept client connection");
            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        });

        format!("http://{addr}")
    }

    #[cfg(not(all(target_os = "wasi", target_env = "p2")))]
    #[tokio::test]
    async fn fetch_optional_returns_none_on_404() {
        let base = spawn_single_response_server(
            "404 Not Found",
            "{\"error\":\"not found\"}",
            "application/json",
        );
        let client = RegistryClient::new(base);

        let result = client
            .fetch_package_detail("ghcr.io", "user/repo")
            .await
            .expect("404 should be treated as not found");
        assert!(result.is_none());
    }

    #[cfg(not(all(target_os = "wasi", target_env = "p2")))]
    #[tokio::test]
    async fn fetch_optional_errors_on_non_404_non_success_status() {
        let base = spawn_single_response_server("500 Internal Server Error", "boom", "text/plain");
        let client = RegistryClient::new(base);

        let err = client
            .fetch_package_detail("ghcr.io", "user/repo")
            .await
            .expect_err("non-404 non-success should return an API error");
        let msg = err.to_string();
        assert!(msg.contains("500"), "error should include status code");
        assert!(
            msg.contains("boom"),
            "error should include response body for debugging"
        );
    }
}
