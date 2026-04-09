//! HTTP server with JSON API endpoints for package discovery.
//!
//! Provides search and listing endpoints backed by the `wasm-package-manager`
//! known packages database.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use serde::Deserialize;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use wasm_meta_registry_types::HostEngine;
use wasm_package_manager::manager::Manager;

/// Shared application state.
///
/// The manager is wrapped in a mutex because all manager calls in handlers
/// are synchronous (no `.await` while holding the lock).
///
/// # Example
///
/// ```no_run
/// use wasm_meta_registry::server::{AppState, StateData};
/// use wasm_package_manager::manager::Manager;
/// use std::sync::{Arc, Mutex};
///
/// # async fn example() -> anyhow::Result<()> {
/// let manager = Manager::open().await?;
/// let state: AppState = Arc::new(StateData {
///     manager: Mutex::new(manager),
///     engines: vec![],
/// });
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct StateData {
    /// Package manager used by search and package endpoints.
    pub manager: std::sync::Mutex<Manager>,
    /// Host runtimes and the interfaces they support.
    pub engines: Vec<HostEngine>,
}

/// Shared application state.
pub type AppState = Arc<StateData>;

/// Query parameters for search.
///
/// # Example
///
/// ```
/// use wasm_meta_registry::server::SearchParams;
///
/// let params = SearchParams {
///     q: "wasi".to_string(),
///     offset: 0,
///     limit: 20,
/// };
///
/// assert_eq!(params.q, "wasi");
/// ```
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// Search query string.
    pub q: String,
    /// Pagination offset (default: 0).
    #[serde(default)]
    pub offset: u32,
    /// Pagination limit (default: 20).
    #[serde(default = "default_limit")]
    pub limit: u32,
}

/// Query parameters for listing packages.
///
/// # Example
///
/// ```
/// use wasm_meta_registry::server::ListParams;
///
/// let params = ListParams {
///     offset: 0,
///     limit: 50,
/// };
///
/// assert_eq!(params.limit, 50);
/// ```
#[derive(Debug, Deserialize)]
pub struct ListParams {
    /// Pagination offset (default: 0).
    #[serde(default)]
    pub offset: u32,
    /// Pagination limit (default: 20).
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    20
}

/// Build the axum router with all API routes.
///
/// # Example
///
/// ```no_run
/// use wasm_meta_registry::router;
/// use wasm_meta_registry::server::StateData;
/// use wasm_package_manager::manager::Manager;
/// use std::sync::{Arc, Mutex};
///
/// # async fn example() -> anyhow::Result<()> {
/// let manager = Manager::open().await?;
/// let state = Arc::new(StateData {
///     manager: Mutex::new(manager),
///     engines: vec![],
/// });
/// let app = router(state);
///
/// let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
/// axum::serve(listener, app).await?;
/// # Ok(())
/// # }
/// ```
pub fn router(state: AppState) -> Router {
    // Routes with explicit suffixes must be registered before the catch-all
    // wildcard `{*repository}` to avoid conflicts.  We achieve this by
    // nesting the version/detail routes under a separate "prefix" router
    // that axum matches first.
    let package_detail_routes =
        Router::new().route("/{registry}/{*repository}", get(get_package_detail_nested));

    let package_versions_routes = Router::new().route(
        "/{registry}/{*repository}",
        get(get_package_versions_nested),
    );

    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/search", get(search))
        .route("/v1/search/by-import", get(search_by_import))
        .route("/v1/search/by-export", get(search_by_export))
        .route("/v1/packages", get(list_packages))
        .route("/v1/packages/recent", get(list_recent_packages))
        .route("/v1/engines", get(list_engines))
        .nest("/v1/packages/detail", package_detail_routes)
        .nest("/v1/packages/versions", package_versions_routes)
        .route(
            "/v1/packages/version/{registry}/{version}/{*repository}",
            get(get_package_version_reordered),
        )
        .route("/v1/packages/{registry}/{*repository}", get(get_package))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Health check endpoint.
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Search packages by query string.
async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<impl IntoResponse, AppError> {
    let manager = state
        .manager
        .lock()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
    let packages = manager.search_packages(&params.q, params.offset, params.limit)?;
    Ok(Json(packages))
}

/// List all known packages.
async fn list_packages(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, AppError> {
    let manager = state
        .manager
        .lock()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
    let packages = manager.list_known_packages(params.offset, params.limit)?;
    Ok(Json(packages))
}

/// List recently updated known packages.
async fn list_recent_packages(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, AppError> {
    let manager = state
        .manager
        .lock()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
    let packages = manager.list_recent_known_packages(params.offset, params.limit)?;
    Ok(Json(packages))
}

/// List host runtimes and their declared interface support.
async fn list_engines(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.engines.clone())
}

/// Get a specific package by registry and repository.
async fn get_package(
    State(state): State<AppState>,
    Path((registry, repository)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    // Wildcard captures include a leading `/`; strip it.
    let repository = repository.trim_start_matches('/');
    let manager = state
        .manager
        .lock()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
    match manager.get_known_package(&registry, repository)? {
        Some(package) => Ok(Json(package).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

/// Query parameters for interface-based search.
#[derive(Debug, Deserialize)]
pub struct InterfaceSearchParams {
    /// The interface to search for (e.g. `"wasi:io/streams"`).
    pub interface: String,
    /// Pagination offset (default: 0).
    #[serde(default)]
    pub offset: u32,
    /// Pagination limit (default: 20).
    #[serde(default = "default_limit")]
    pub limit: u32,
}

/// Search packages by imported interface.
// r[verify server.search.by-import]
async fn search_by_import(
    State(state): State<AppState>,
    Query(params): Query<InterfaceSearchParams>,
) -> Result<impl IntoResponse, AppError> {
    let manager = state
        .manager
        .lock()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
    let packages =
        manager.search_packages_by_import(&params.interface, params.offset, params.limit)?;
    Ok(Json(packages))
}

/// Search packages by exported interface.
// r[verify server.search.by-export]
async fn search_by_export(
    State(state): State<AppState>,
    Query(params): Query<InterfaceSearchParams>,
) -> Result<impl IntoResponse, AppError> {
    let manager = state
        .manager
        .lock()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
    let packages =
        manager.search_packages_by_export(&params.interface, params.offset, params.limit)?;
    Ok(Json(packages))
}

/// Get full package detail including all versions and metadata.
// r[verify server.detail]
async fn get_package_detail_nested(
    State(state): State<AppState>,
    Path((registry, repository)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let repository = repository.trim_start_matches('/');
    let manager = state
        .manager
        .lock()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
    match manager.get_package_detail(&registry, repository)? {
        Some(detail) => Ok(Json(detail).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

/// List all versions of a package.
// r[verify server.versions.list]
async fn get_package_versions_nested(
    State(state): State<AppState>,
    Path((registry, repository)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let repository = repository.trim_start_matches('/');
    let manager = state
        .manager
        .lock()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
    match manager.get_package_detail(&registry, repository)? {
        Some(detail) => Ok(Json(detail.versions).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

/// Get a specific version of a package by tag.
// r[verify server.versions.get]
async fn get_package_version_reordered(
    State(state): State<AppState>,
    Path((registry, version, repository)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let repository = repository.trim_start_matches('/');
    let manager = state
        .manager
        .lock()
        .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
    match manager.get_package_version(&registry, repository, &version)? {
        Some(ver) => Ok(Json(ver).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

/// Application error type that converts to HTTP responses.
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wasm_meta_registry_types::HostInterfaceSupport;

    // r[verify server.health]
    /// Verify the server starts, binds to a port, and responds to `/v1/health`.
    #[tokio::test]
    async fn server_starts_and_listens() {
        let tempdir = tempfile::tempdir().expect("failed to create tempdir");
        let manager = Manager::open_at(tempdir.path())
            .await
            .expect("failed to open manager");
        let state = Arc::new(StateData {
            manager: std::sync::Mutex::new(manager),
            engines: vec![],
        });
        let app = router(state);

        // Bind to port 0 so the OS assigns a random available port.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind listener");
        let addr = listener.local_addr().expect("failed to get local addr");

        // Spawn the server in a background task.
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server error");
        });

        // Hit the health endpoint.
        let url = format!("http://{addr}/v1/health");
        let resp = reqwest::get(&url).await.expect("request failed");
        assert_eq!(resp.status(), 200);

        let body: serde_json::Value = resp.json().await.expect("invalid json");
        assert_eq!(body, serde_json::json!({ "status": "ok" }));

        // Clean up.
        server.abort();
    }

    // r[verify server.engines]
    #[tokio::test]
    async fn engines_endpoint_returns_configured_engines() {
        let tempdir = tempfile::tempdir().expect("failed to create tempdir");
        let manager = Manager::open_at(tempdir.path())
            .await
            .expect("failed to open manager");
        let state = Arc::new(StateData {
            manager: std::sync::Mutex::new(manager),
            engines: vec![HostEngine {
                name: "wasmtime".to_string(),
                homepage: Some("https://wasmtime.dev".to_string()),
                interfaces: vec![HostInterfaceSupport {
                    interface: "wasi:http".to_string(),
                    versions: vec!["0.2.0".to_string()],
                }],
            }],
        });
        let app = router(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind listener");
        let addr = listener.local_addr().expect("failed to get local addr");

        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server error");
        });

        let url = format!("http://{addr}/v1/engines");
        let resp = reqwest::get(&url).await.expect("request failed");
        assert_eq!(resp.status(), 200);
        let body: Vec<HostEngine> = resp.json().await.expect("invalid json");
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].name, "wasmtime");

        server.abort();
    }
}
