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

use axum::extract::Path;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Json, Router, routing::get};

use crate::api_client::ApiClient;
use crate::reserved::is_reserved;

/// Build the application router with all frontend routes.
fn app() -> Router {
    Router::new()
        .route("/", get(home))
        .route("/all", get(all_packages))
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

// r[impl frontend.pages.all]
/// Paginated listing of all known packages.
async fn all_packages() -> Response {
    let client = ApiClient::from_env();
    let html = pages::all::render(&client).await;
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
        return Err(not_found().await);
    }

    let client = ApiClient::from_env();
    match client.fetch_package_by_wit(&namespace, &name).await {
        Some(pkg) => {
            let version = pkg
                .tags
                .first()
                .cloned()
                .unwrap_or_else(|| "latest".to_string());
            Ok(Redirect::temporary(&format!(
                "/{namespace}/{name}/{version}"
            )))
        }
        None => Err(not_found().await),
    }
}

// r[impl frontend.pages.package-detail]
// r[impl frontend.routing.package-path]
/// Package detail page at `/<namespace>/<name>/<version>`.
async fn package_detail(
    Path((namespace, name, version)): Path<(String, String, String)>,
) -> Response {
    if is_reserved(&namespace) {
        return not_found().await;
    }

    let client = ApiClient::from_env();
    match client.fetch_package_by_wit(&namespace, &name).await {
        Some(pkg) => {
            let html = pages::package::render(&pkg, &version);
            with_cache_control(html, "public, max-age=300")
        }
        None => not_found().await,
    }
}

// r[impl frontend.pages.not-found]
/// Fallback 404 handler.
async fn not_found() -> Response {
    let html = pages::not_found::render();
    let mut response = axum::response::Html(html).into_response();
    *response.status_mut() = StatusCode::NOT_FOUND;
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
