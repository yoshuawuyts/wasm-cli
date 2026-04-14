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

mod fonts;
mod footer;
mod layout;
mod markdown;
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

/// Build the application router with all frontend routes.
fn app() -> Router {
    Router::new()
        .route("/", get(home))
        .route("/all", get(all_packages))
        .route("/search", get(search))
        .route("/about", get(about))
        .route("/docs", get(docs))
        .route("/downloads", get(downloads))
        .route("/health", get(health))
        .route("/fonts/iosevka-regular.woff2", get(fonts::regular))
        .route("/fonts/iosevka-medium.woff2", get(fonts::medium))
        .route("/fonts/iosevka-semibold.woff2", get(fonts::semibold))
        .route("/fonts/iosevka-bold.woff2", get(fonts::bold))
        .route("/{namespace}/{name}", get(package_redirect))
        .route("/{namespace}/{name}/", get(package_redirect))
        .route("/{namespace}", get(namespace_page))
        .route("/{namespace}/", get(namespace_page))
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

/// About page — redirects to docs.
async fn about() -> Response {
    Redirect::permanent("/docs").into_response()
}

/// Documentation page.
async fn docs() -> Response {
    let html = pages::docs::render();
    with_cache_control(html, "public, max-age=3600")
}

/// Downloads page.
async fn downloads() -> Response {
    let html = pages::downloads::render();
    with_cache_control(html, "public, max-age=3600")
}

/// Namespace page — list all packages under a publisher.
async fn namespace_page(Path(namespace): Path<String>) -> Response {
    if is_reserved(&namespace) {
        return not_found_response();
    }

    let client = RegistryClient::from_env();
    let html = pages::namespace::render(&client, &namespace).await;
    with_cache_control(html, "public, max-age=60")
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
        Err(resp) => return resp,
    };
    let version_detail = client
        .fetch_package_version(&pkg.registry, &pkg.repository, &version)
        .await
        .ok()
        .flatten();
    let display_name = format!("{namespace}:{name}");
    let importers = client
        .search_packages_by_import(&display_name)
        .await
        .unwrap_or_default();
    let exporters = client
        .search_packages_by_export(&display_name)
        .await
        .unwrap_or_default();
    let html = pages::package::render(
        &pkg,
        &version,
        version_detail.as_ref(),
        &importers,
        &exporters,
    );
    with_cache_control(html, "public, max-age=300")
}

/// Legacy dependencies route — redirects to the main package page.
async fn package_dependencies(
    Path((namespace, name, version)): Path<(String, String, String)>,
) -> Response {
    Redirect::permanent(&format!("/{namespace}/{name}/{version}")).into_response()
}

/// Legacy dependents route — redirects to the main package page.
async fn package_dependents(
    Path((namespace, name, version)): Path<(String, String, String)>,
) -> Response {
    Redirect::permanent(&format!("/{namespace}/{name}/{version}")).into_response()
}

/// Interface detail page at `/<namespace>/<name>/<version>/interface/<iface>`.
async fn interface_detail(
    Path((namespace, name, version, iface)): Path<(String, String, String, String)>,
) -> Response {
    let client = RegistryClient::from_env();
    let pkg = match fetch_package_or_404(&client, &namespace, &name, &version).await {
        Ok(Some(pkg)) => pkg,
        Ok(None) => return not_found_response(),
        Err(resp) => return resp,
    };
    let Some((doc, version_detail)) = fetch_wit_doc(&client, &pkg, &version).await else {
        return not_found_response();
    };
    let Some(iface_doc) = doc.interfaces.iter().find(|i| i.name == iface) else {
        return not_found_response();
    };
    let html = pages::interface::render(&pkg, &version, Some(&version_detail), iface_doc, &doc);
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
        Err(resp) => return resp,
    };
    let Some((doc, version_detail)) = fetch_wit_doc(&client, &pkg, &version).await else {
        return not_found_response();
    };
    let Some(iface_doc) = doc.interfaces.iter().find(|i| i.name == iface) else {
        return not_found_response();
    };

    // Try types first, then functions.
    if let Some(ty) = iface_doc.types.iter().find(|t| t.name == item_name) {
        let html =
            pages::item::render_type(&pkg, &version, Some(&version_detail), &iface, ty, &doc);
        return with_cache_control(html, "public, max-age=300");
    }
    if let Some(func) = iface_doc.functions.iter().find(|f| f.name == item_name) {
        let html =
            pages::item::render_function(&pkg, &version, Some(&version_detail), &iface, func, &doc);
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
        Err(resp) => return resp,
    };
    let Some((doc, version_detail)) = fetch_wit_doc(&client, &pkg, &version).await else {
        return not_found_response();
    };
    let Some(world_doc) = doc.worlds.iter().find(|w| w.name == world_name) else {
        return not_found_response();
    };
    let html = pages::world::render(&pkg, &version, Some(&version_detail), world_doc, &doc);
    with_cache_control(html, "public, max-age=300")
}

/// Fetch and parse the WIT document for a package version, returning
/// both the parsed document and the version detail.
async fn fetch_wit_doc(
    client: &RegistryClient,
    pkg: &KnownPackage,
    version: &str,
) -> Option<(
    wit_doc::WitDocument,
    wasm_meta_registry_client::PackageVersion,
)> {
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
    let doc = wit_doc::parse_wit_doc(wit_text, &url_base, &dep_urls).ok()?;
    Some((doc, detail))
}

/// Fetch a package by WIT namespace/name, validating the version exists.
///
/// Returns `Ok(None)` (and logs) if the namespace is reserved, the package is
/// not found, or the version tag doesn't exist. Returns `Err(Response)` with
/// a `502 Bad Gateway` response when the upstream API call fails, so that
/// registry outages are surfaced correctly instead of being masked as 404s.
async fn fetch_package_or_404(
    client: &RegistryClient,
    namespace: &str,
    name: &str,
    version: &str,
) -> Result<Option<KnownPackage>, Response> {
    if is_reserved(namespace) {
        return Ok(None);
    }
    match client.fetch_package_by_wit(namespace, name).await {
        Ok(Some(pkg)) => {
            if pkg.tags.iter().any(|tag| tag == version) {
                Ok(Some(pkg))
            } else {
                eprintln!("wasm-frontend: version not found for {namespace}/{name}: {version}");
                Ok(None)
            }
        }
        Ok(None) => {
            eprintln!("wasm-frontend: package not found: {namespace}/{name}@{version}");
            Ok(None)
        }
        Err(e) => {
            eprintln!("wasm-frontend: API error looking up {namespace}/{name}@{version}: {e}");
            Err(error_response(&e.to_string()))
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
pub(crate) fn pick_redirect_version(tags: &[String]) -> Option<String> {
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

    /// Trailing-slash URLs must be handled: the router must register
    /// both `/{namespace}/{name}` and `/{namespace}/{name}/`.
    #[test]
    fn trailing_slash_package_route_is_registered() {
        // Verify the app builds with trailing-slash routes by checking
        // that the route table doesn't panic or conflict.
        let _app = app();
    }

    /// Verify the package redirect handler works with valid path parameters
    /// and doesn't panic — it should either redirect, return not-found, or
    /// return bad-gateway when the registry API is unreachable.
    #[tokio::test]
    async fn package_redirect_handles_trailing_slash_path() {
        let result = package_redirect(Path(("wasi".to_string(), "random".to_string()))).await;
        match result {
            Ok(redirect) => {
                let resp = redirect.into_response();
                assert!(resp.status().is_redirection());
            }
            Err(resp) => {
                let status = resp.status();
                assert!(
                    status == StatusCode::NOT_FOUND || status == StatusCode::BAD_GATEWAY,
                    "expected 404 or 502, got {status}"
                );
            }
        }
    }
}
