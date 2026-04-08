//! Web frontend for the WebAssembly package registry.
//!
//! A server-side rendered web application compiled as a `wasm32-wasip2`
//! component targeting `wasi:http`. Uses `wstd-axum` for routing and the
//! `html` crate for type-safe HTML generation.

// Logging errors to stderr is the appropriate way to surface API failures
// when running under wasmtime serve.
#![allow(clippy::print_stderr)]
#![recursion_limit = "512"]

// r[impl frontend.server.wasi-http]

mod api_client;
mod footer;
mod layout;
mod nav;
mod pages;
mod reserved;

use axum::extract::{Path, Query};
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Json, Router, routing::get};
use serde::Deserialize;

use crate::api_client::ApiClient;
use crate::reserved::is_reserved;

/// Build the application router with all frontend routes.
fn app() -> Router {
    Router::new()
        .route("/", get(home))
        .route("/all", get(all_packages))
        .route("/search", get(search))
        .route("/about", get(about))
        .route("/health", get(health))
        .route("/{namespace}/{name}", get(package_redirect))
        .route("/{namespace}/{name}/{version}", get(package_detail))
        .fallback(not_found)
}

// r[impl frontend.server.wasi-http]
#[wstd_axum::http_server]
fn main() -> Router {
    app()
}

// r[impl frontend.server.health]
/// Health check endpoint.
async fn health() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-cache")],
        Json(serde_json::json!({ "status": "ok" })),
    )
}

// r[impl frontend.pages.home]
/// Front page showing recently updated components and interfaces.
async fn home() -> Response {
    let client = ApiClient::from_env();
    let html = pages::home::render(&client).await;
    with_cache_control(html, "public, max-age=60")
}

/// Query parameters for the search page.
#[derive(Deserialize)]
struct SearchParams {
    /// The search query string.
    #[serde(default)]
    q: String,
}

/// Query parameters for the all-packages page.
#[derive(Deserialize)]
struct AllPackagesParams {
    /// Pagination offset.
    #[serde(default)]
    offset: u32,
    /// Pagination limit.
    #[serde(default = "default_all_packages_limit")]
    limit: u32,
}

fn default_all_packages_limit() -> u32 {
    100
}

// r[impl frontend.pages.search]
/// Search results page.
async fn search(Query(params): Query<SearchParams>) -> Response {
    let client = ApiClient::from_env();
    let html = pages::search::render(&client, &params.q).await;
    with_cache_control(html, "public, max-age=60")
}

// r[impl frontend.pages.all]
/// Paginated listing of all known packages.
async fn all_packages(Query(params): Query<AllPackagesParams>) -> Response {
    let client = ApiClient::from_env();
    let limit = params.limit.clamp(1, 200);
    let html = pages::all::render(&client, params.offset, limit).await;
    with_cache_control(html, "public, max-age=60")
}

/// About page (placeholder).
async fn about() -> Response {
    let html = pages::about::render();
    with_cache_control(html, "public, max-age=3600")
}

// r[impl frontend.pages.package-redirect]
// r[impl frontend.routing.reserved-namespaces]
/// Redirect `/<namespace>/<name>` to `/<namespace>/<name>/<latest-version>`.
async fn package_redirect(
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Redirect, Response> {
    if is_reserved(&namespace) {
        return Err(not_found_response());
    }

    let client = ApiClient::from_env();
    match client.fetch_package_by_wit(&namespace, &name).await {
        Ok(Some(pkg)) => {
            if let Some(version) = pick_redirect_version(&pkg.tags) {
                Ok(Redirect::temporary(&format!(
                    "/{namespace}/{name}/{version}"
                )))
            } else {
                eprintln!("wasm-frontend: package has no redirectable tags: {namespace}/{name}");
                Err(not_found_response())
            }
        }
        Ok(None) => {
            eprintln!("wasm-frontend: package not found: {namespace}/{name}");
            Err(not_found_response())
        }
        Err(e) => {
            eprintln!("wasm-frontend: API error looking up {namespace}/{name}: {e}");
            Err(error_response(&e.to_string()))
        }
    }
}

// r[impl frontend.pages.package-detail]
// r[impl frontend.routing.package-path]
/// Package detail page at `/<namespace>/<name>/<version>`.
async fn package_detail(
    Path((namespace, name, version)): Path<(String, String, String)>,
) -> Response {
    if is_reserved(&namespace) {
        return not_found_response();
    }

    let client = ApiClient::from_env();
    match client.fetch_package_by_wit(&namespace, &name).await {
        Ok(Some(pkg)) => {
            if !pkg.tags.iter().any(|tag| tag == &version) {
                eprintln!("wasm-frontend: version not found for {namespace}/{name}: {version}");
                return not_found_response();
            }
            let html = pages::package::render(&pkg, &version);
            with_cache_control(html, "public, max-age=300")
        }
        Ok(None) => {
            eprintln!("wasm-frontend: package not found: {namespace}/{name}@{version}");
            not_found_response()
        }
        Err(e) => {
            eprintln!("wasm-frontend: API error looking up {namespace}/{name}@{version}: {e}");
            error_response(&e.to_string())
        }
    }
}

// r[impl frontend.pages.not-found]
/// Fallback 404 handler — logs a warning and renders the not-found page.
async fn not_found(uri: Uri) -> Response {
    eprintln!("wasm-frontend: 404 {uri}");
    not_found_response()
}

/// Render the 404 page response.
fn not_found_response() -> Response {
    let html = pages::not_found::render();
    let mut response = axum::response::Html(html).into_response();
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// Render an error page when the registry API is unreachable.
fn error_response(message: &str) -> Response {
    let html = pages::error::render(message);
    let mut response = axum::response::Html(html).into_response();
    *response.status_mut() = StatusCode::BAD_GATEWAY;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

// r[impl frontend.caching.static-pages]
/// Wrap an HTML string response with `Cache-Control` header.
fn with_cache_control(html: String, cache_control: &'static str) -> Response {
    let mut response = axum::response::Html(html).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    response
}

#[must_use]
fn pick_redirect_version(tags: &[String]) -> Option<String> {
    tags.iter()
        .filter_map(|tag| {
            semver::Version::parse(tag)
                .ok()
                .filter(|version| version.pre.is_empty())
                .map(|version| (version, tag))
        })
        .max_by(|(acc_version, _), (candidate_version, _)| acc_version.cmp(candidate_version))
        .map(|(_, tag)| tag.clone())
        .or_else(|| tags.iter().find(|tag| tag.as_str() == "latest").cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify frontend.routing.reserved-namespaces]
    #[tokio::test]
    async fn package_redirect_reserved_namespace_returns_not_found() {
        let response = package_redirect(Path(("all".to_string(), "demo".to_string())))
            .await
            .expect_err("reserved namespace should not redirect");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .expect("cache-control header should be set"),
            "no-cache"
        );
    }

    // r[verify frontend.routing.reserved-namespaces]
    #[tokio::test]
    async fn package_detail_reserved_namespace_returns_not_found() {
        let response = package_detail(Path((
            "all".to_string(),
            "demo".to_string(),
            "1.0.0".to_string(),
        )))
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .expect("cache-control header should be set"),
            "no-cache"
        );
    }

    // r[verify frontend.pages.not-found]
    #[tokio::test]
    async fn fallback_not_found_has_expected_status_and_headers() {
        let response = not_found(Uri::from_static("/does-not-exist")).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .expect("cache-control header should be set"),
            "no-cache"
        );
    }

    #[test]
    fn pick_redirect_version_prefers_latest_stable_semver() {
        let tags = vec![
            "latest".to_string(),
            "2.0.0-rc.1".to_string(),
            "1.2.0".to_string(),
            "1.10.0".to_string(),
        ];
        assert_eq!(pick_redirect_version(&tags), Some("1.10.0".to_string()));
    }

    #[test]
    fn pick_redirect_version_falls_back_to_latest_tag() {
        let tags = vec!["latest".to_string(), "sha256-deadbeef".to_string()];
        assert_eq!(pick_redirect_version(&tags), Some("latest".to_string()));
    }

    #[test]
    fn pick_redirect_version_returns_none_for_unusable_tags() {
        let tags = vec!["sha256-deadbeef".to_string()];
        assert_eq!(pick_redirect_version(&tags), None);
    }
}
