//! API client for fetching package data from the meta-registry.

// r[impl frontend.api.callback]
// r[impl frontend.api.base-url]

use std::fmt;

use wasm_meta_registry_client::KnownPackage;
use wstd::http::{Body, Client, Request};

/// Default API base URL when no environment variable is set.
const DEFAULT_API_BASE_URL: &str = "http://localhost:8081";

/// An error returned when the meta-registry API is unreachable or returns
/// an unexpected response.
#[derive(Debug)]
pub(crate) struct ApiError {
    message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Thin wrapper around `wstd::http::Client` for meta-registry API calls.
#[derive(Debug)]
pub(crate) struct ApiClient {
    base_url: String,
    client: Client,
}

impl ApiClient {
    /// Create a new client with the given base URL.
    #[must_use]
    pub(crate) fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: Client::new(),
        }
    }

    /// Create a client using the API base URL.
    ///
    /// The URL is set at compile time via the `API_BASE_URL` environment
    /// variable. Falls back to `http://localhost:8081` when unset.
    #[must_use]
    pub(crate) fn from_env() -> Self {
        let base_url = option_env!("API_BASE_URL").unwrap_or(DEFAULT_API_BASE_URL);
        Self::new(base_url)
    }

    /// Fetch recently updated packages from the meta-registry.
    pub(crate) async fn fetch_recent_packages(
        &self,
        limit: u32,
    ) -> Result<Vec<KnownPackage>, ApiError> {
        let url = format!("{}/v1/packages/recent?limit={limit}", self.base_url);
        self.fetch_packages_from(&url).await
    }

    /// Search packages by query string.
    pub(crate) async fn search_packages(&self, query: &str) -> Result<Vec<KnownPackage>, ApiError> {
        let encoded_query = percent_encode_query_component(query);
        let url = format!("{}/v1/search?q={encoded_query}", self.base_url);
        self.fetch_packages_from(&url).await
    }

    /// Fetch all packages with pagination.
    pub(crate) async fn fetch_all_packages(
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
    pub(crate) async fn fetch_package_by_wit(
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
        let req = Request::get(url)
            .body(Body::empty())
            .map_err(|e| ApiError {
                message: format!("failed to build request for {url}: {e}"),
            })?;

        let response = self.client.send(req).await.map_err(|e| {
            eprintln!("wasm-frontend: HTTP request to {url} failed: {e}");
            ApiError {
                message: format!("could not connect to the registry API: {e}"),
            }
        })?;

        let mut body = response.into_body();
        body.json::<Vec<KnownPackage>>().await.map_err(|e| {
            eprintln!("wasm-frontend: failed to parse response from {url}: {e}");
            ApiError {
                message: format!("received an unexpected response from the registry: {e}"),
            }
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
