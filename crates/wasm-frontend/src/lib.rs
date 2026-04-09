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

mod footer;
mod layout;
mod nav;
mod pages;
mod reserved;
mod wit_doc;

use axum::extract::{Path, Query};
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Json, Router, routing::get};
use serde::Deserialize;

use wasm_meta_registry_client::{KnownPackage, RegistryClient};

use crate::reserved::is_reserved;
use pages::package::ActiveTab;

/// Build the application router with all frontend routes.
fn app() -> Router {
    Router::new()
        .route("/", get(home))
        .route("/all", get(all_packages))
        .route("/search", get(search))
        .route("/about", get(about))
        .route("/docs", get(docs))
        .route("/health", get(health))
        .route("/{namespace}/{name}", get(package_redirect))
        .route("/{namespace}/{name}/{version}", get(package_detail))
        .route(
            "/{namespace}/{name}/{version}/dependencies",
            get(package_dependencies),
        )
        .route(
            "/{namespace}/{name}/{version}/dependents",
            get(package_dependents),
        )
        .route(
            "/{namespace}/{name}/{version}/interface/{iface}",
            get(interface_detail),
        )
        .route(
            "/{namespace}/{name}/{version}/interface/{iface}/{item}",
            get(item_detail),
        )
        .route(
            "/{namespace}/{name}/{version}/world/{world_name}",
            get(world_detail),
        )
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
    let client = RegistryClient::from_env();
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
    let client = RegistryClient::from_env();
    let html = pages::search::render(&client, &params.q).await;
    with_cache_control(html, "public, max-age=60")
}

// r[impl frontend.pages.all]
/// Paginated listing of all known packages.
async fn all_packages(Query(params): Query<AllPackagesParams>) -> Response {
    let client = RegistryClient::from_env();
    let limit = params.limit.clamp(1, 200);
    let html = pages::all::render(&client, params.offset, limit).await;
    with_cache_control(html, "public, max-age=60")
}

/// About page (placeholder).
async fn about() -> Response {
    let html = pages::about::render();
    with_cache_control(html, "public, max-age=3600")
}

/// Documentation page.
async fn docs() -> Response {
    let html = pages::docs::render();
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

    let client = RegistryClient::from_env();
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
    let client = RegistryClient::from_env();
    let pkg = match fetch_package_or_404(&client, &namespace, &name, &version).await {
        Ok(Some(pkg)) => pkg,
        Ok(None) => return not_found_response(),
        Err(response) => return response,
    };
    let version_detail = client
        .fetch_package_version(&pkg.registry, &pkg.repository, &version)
        .await
        .ok()
        .flatten();
    let tab = ActiveTab::Docs {
        version_detail: version_detail.as_ref(),
    };
    let html = pages::package::render(&pkg, &version, &tab);
    with_cache_control(html, "public, max-age=300")
}

/// Dependencies tab at `/<namespace>/<name>/<version>/dependencies`.
async fn package_dependencies(
    Path((namespace, name, version)): Path<(String, String, String)>,
) -> Response {
    let client = RegistryClient::from_env();
    let pkg = match fetch_package_or_404(&client, &namespace, &name, &version).await {
        Ok(Some(pkg)) => pkg,
        Ok(None) => return not_found_response(),
        Err(response) => return response,
    };
    let html = pages::package::render(&pkg, &version, &tab);
    with_cache_control(html, "public, max-age=300")
}

/// Dependents tab at `/<namespace>/<name>/<version>/dependents`.
async fn package_dependents(
    Path((namespace, name, version)): Path<(String, String, String)>,
) -> Response {
    let client = RegistryClient::from_env();
    let pkg = match fetch_package_or_404(&client, &namespace, &name, &version).await {
        Ok(Some(pkg)) => pkg,
        Ok(None) => return not_found_response(),
        Err(response) => return response,
    };
    let display_name = format!("{namespace}:{name}");
    let importers = client
        .search_packages_by_import(&display_name)
        .await
        .unwrap_or_default();
    let exporters = client
        .search_packages_by_export(&display_name)
        .await
        .unwrap_or_default();
    let tab = ActiveTab::Dependents {
        importers: &importers,
        exporters: &exporters,
    };
    let html = pages::package::render(&pkg, &version, &tab);
    with_cache_control(html, "public, max-age=300")
}

/// Interface detail page at `/<namespace>/<name>/<version>/interface/<iface>`.
async fn interface_detail(
    Path((namespace, name, version, iface)): Path<(String, String, String, String)>,
) -> Response {
    let client = RegistryClient::from_env();
    let pkg = match fetch_package_or_404(&client, &namespace, &name, &version).await {
        Ok(Some(pkg)) => pkg,
        Ok(None) => return not_found_response(),
        Err(response) => return response,
    };
    let Some(doc) = fetch_wit_doc(&client, &pkg, &version).await else {
        return not_found_response();
    };
    let Some(iface_doc) = doc.interfaces.iter().find(|i| i.name == iface) else {
        return not_found_response();
    };
    let display_name = format!("{namespace}:{name}");
    let html = pages::interface::render(&display_name, &version, iface_doc, &doc);
    with_cache_control(html, "public, max-age=300")
}

/// Item detail page at `/<namespace>/<name>/<version>/interface/<iface>/<item>`.
async fn item_detail(
    Path((namespace, name, version, iface, item_name)): Path<(
        String,
        String,
        String,
        String,
        String,
    )>,
) -> Response {
    let client = RegistryClient::from_env();
    let pkg = match fetch_package_or_404(&client, &namespace, &name, &version).await {
        Ok(Some(pkg)) => pkg,
        Ok(None) => return not_found_response(),
        Err(response) => return response,
    };
    let Some(doc) = fetch_wit_doc(&client, &pkg, &version).await else {
        return not_found_response();
    };
    let Some(iface_doc) = doc.interfaces.iter().find(|i| i.name == iface) else {
        return not_found_response();
    };
    let display_name = format!("{namespace}:{name}");

    // Try types first, then functions.
    if let Some(ty) = iface_doc.types.iter().find(|t| t.name == item_name) {
        let html = pages::item::render_type(&display_name, &version, &iface, ty, &doc);
        return with_cache_control(html, "public, max-age=300");
    }
    if let Some(func) = iface_doc.functions.iter().find(|f| f.name == item_name) {
        let html = pages::item::render_function(&display_name, &version, &iface, func, &doc);
        return with_cache_control(html, "public, max-age=300");
    }

    not_found_response()
}

/// World detail page at `/<namespace>/<name>/<version>/world/<world_name>`.
async fn world_detail(
    Path((namespace, name, version, world_name)): Path<(String, String, String, String)>,
) -> Response {
    let client = RegistryClient::from_env();
    let pkg = match fetch_package_or_404(&client, &namespace, &name, &version).await {
        Ok(Some(pkg)) => pkg,
        Ok(None) => return not_found_response(),
        Err(response) => return response,
    };
    let Some(doc) = fetch_wit_doc(&client, &pkg, &version).await else {
        return not_found_response();
    };
    let Some(world_doc) = doc.worlds.iter().find(|w| w.name == world_name) else {
        return not_found_response();
    };
    let display_name = format!("{namespace}:{name}");
    let html = pages::world::render(&display_name, &version, world_doc, &doc);
    with_cache_control(html, "public, max-age=300")
}

/// Fetch and parse the WIT document for a package version.
async fn fetch_wit_doc(
    client: &RegistryClient,
    pkg: &KnownPackage,
    version: &str,
) -> Option<wit_doc::WitDocument> {
    let detail = client
        .fetch_package_version(&pkg.registry, &pkg.repository, version)
        .await
        .ok()
        .flatten()?;
    let wit_text = detail.wit_text.as_deref()?;
    let dep_urls: std::collections::HashMap<String, String> = detail
        .dependencies
        .iter()
        .filter_map(|dep| {
            let v = dep.version.as_deref()?;
            let url = format!("/{}/{v}", dep.package.replace(':', "/"));
            Some((dep.package.clone(), url))
        })
        .collect();
    let url_base = format!(
        "/{}/{}/{}",
        pkg.wit_namespace.as_deref().unwrap_or("_"),
        pkg.wit_name.as_deref().unwrap_or(&pkg.repository),
        version
    );
    wit_doc::parse_wit_doc(wit_text, &url_base, &dep_urls).ok()
}

/// Fetch a package by WIT namespace/name, validating the version exists.
///
/// Returns `None` (and logs) if the namespace is reserved, the package is
/// not found, or the version tag doesn't exist. Returns a 502 response if the
/// upstream API call fails.
async fn fetch_package_or_404(
    client: &RegistryClient,
    namespace: &str,
    name: &str,
    version: &str,
) -> Option<KnownPackage> {
    if is_reserved(namespace) {
        return None;
    }
    match client.fetch_package_by_wit(namespace, name).await {
        Ok(Some(pkg)) => {
            if pkg.tags.iter().any(|tag| tag == version) {
                Some(pkg)
            } else {
                eprintln!("wasm-frontend: version not found for {namespace}/{name}: {version}");
                None
            }
        }
        Ok(None) => {
            eprintln!("wasm-frontend: package not found: {namespace}/{name}@{version}");
            None
        }
        Err(e) => {
            eprintln!("wasm-frontend: API error looking up {namespace}/{name}@{version}: {e}");
            Err(error_response(&format!("{e:#}")))
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
    use axum::body::to_bytes;

    // r[verify frontend.pages.package-redirect]
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

    // r[verify frontend.pages.package-detail]
    // r[verify frontend.routing.package-path]
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

    // r[verify frontend.server.wasi-http]
    // r[verify frontend.server.health]
    #[tokio::test]
    async fn health_returns_ok_json_and_no_cache() {
        let response = health().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .expect("cache-control header should be set"),
            "no-cache"
        );

        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("health response body should be readable");
        assert_eq!(bytes.as_ref(), br#"{"status":"ok"}"#);
    }

    // r[verify frontend.caching.static-pages]
    #[test]
    fn with_cache_control_sets_header() {
        let response = with_cache_control("<p>Hello</p>".to_string(), "public, max-age=60");
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .expect("cache-control header should be set"),
            "public, max-age=60"
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
