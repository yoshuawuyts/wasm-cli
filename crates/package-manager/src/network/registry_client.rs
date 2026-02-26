//! HTTP client for syncing packages from a wasm-meta-registry instance.
//!
//! Uses ETags for conditional fetches and exponential backoff for retries.

use std::time::Duration;

use exponential_backoff::Backoff;

use crate::storage::KnownPackage;

/// Result of fetching packages from the meta-registry.
#[derive(Debug)]
pub(crate) enum FetchResult {
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

/// HTTP client for the meta-registry's `/v1/packages` endpoint.
#[derive(Debug)]
pub(crate) struct RegistryClient {
    base_url: String,
    http: reqwest::Client,
}

impl RegistryClient {
    /// Create a new registry client pointing at `base_url`.
    pub(crate) fn new(base_url: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        }
    }

    /// Fetch all packages from the meta-registry.
    ///
    /// Sends `If-None-Match` when an ETag is available. Retries up to 3 times
    /// with exponential backoff on transient errors.
    ///
    /// # Errors
    ///
    /// Returns an error if all retry attempts fail.
    pub(crate) async fn fetch_packages(&self, etag: Option<&str>) -> anyhow::Result<FetchResult> {
        let url = format!("{}/v1/packages?limit=1000", self.base_url);
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

    /// Single attempt to fetch packages.
    async fn try_fetch(&self, url: &str, etag: Option<&str>) -> anyhow::Result<FetchResult> {
        let mut req = self.http.get(url);
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
