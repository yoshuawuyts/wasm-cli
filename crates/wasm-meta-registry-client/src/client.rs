//! HTTP client for querying and syncing packages from a meta-registry.
//!
//! Uses `wstd::http` when compiled for WASI p2 components and `reqwest` on
//! native targets (requires the **`client`** feature).

// r[impl frontend.api.callback]
// r[impl frontend.api.base-url]

use std::fmt;

use crate::KnownPackage;

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

    /// Fetch and deserialize a list of packages from the given URL.
    async fn fetch_packages_from(&self, url: &str) -> Result<Vec<KnownPackage>, ApiError> {
        let bytes = self.get(url).await?;
        serde_json::from_slice(&bytes).map_err(|e| {
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
}
