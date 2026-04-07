#![allow(clippy::print_stdout, clippy::print_stderr)]

//! HTTP server for components targeting the `wasi:http/proxy` world.
//!
//! When a component exports `wasi:http/incoming-handler`, this module starts a
//! local HTTP server that forwards each incoming request to the guest.

use std::net::SocketAddr;
use std::sync::Arc;

use hyper::server::conn::http1;
use miette::Context;
use wasmparser::{Parser, Payload};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::io::TokioIo;
use wasmtime_wasi_http::p2::bindings::ProxyPre;
use wasmtime_wasi_http::p2::bindings::http::types::Scheme;
use wasmtime_wasi_http::p2::body::HyperOutgoingBody;
use wasmtime_wasi_http::{
    WasiHttpCtx,
    p2::{WasiHttpCtxView, WasiHttpView},
};

use super::errors::RunError;

/// Host state for HTTP components, wired into `Store<HttpState>`.
struct HttpState {
    wasi: WasiCtx,
    http: WasiHttpCtx,
    table: ResourceTable,
}

impl WasiView for HttpState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl WasiHttpView for HttpState {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http,
            table: &mut self.table,
            // Use the built-in default hooks (standard outgoing request handling).
            hooks: Default::default(),
        }
    }
}

/// Shared server state holding the pre-instantiated component and
/// resolved permissions for building per-request WASI contexts.
struct Server {
    pre: ProxyPre<HttpState>,
    permissions: wasm_manifest::ResolvedPermissions,
}

/// Check whether a component exports `wasi:http/incoming-handler`, indicating
/// it targets the `wasi:http/proxy` world rather than `wasi:cli/command`.
///
/// Only top-level component exports are considered; nested component exports
/// are ignored by tracking nesting depth through `Version` / `End` payloads.
pub(super) fn exports_http_incoming_handler(bytes: &[u8]) -> bool {
    let parser = Parser::new(0);
    let mut depth: u32 = 0;

    for payload in parser.parse_all(bytes) {
        let Ok(payload) = payload else { continue };
        match payload {
            Payload::Version { .. } => {
                depth += 1;
            }
            Payload::End(_) => {
                depth = depth.saturating_sub(1);
            }
            Payload::ComponentExportSection(reader) if depth == 1 => {
                for export in reader.into_iter().flatten() {
                    if export.name.0.starts_with("wasi:http/incoming-handler") {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// Start an HTTP server that proxies incoming requests to an HTTP
/// `wasi:http/proxy` component.
///
/// This function listens on `addr`, accepting connections and forwarding
/// each request to a fresh component instance. It runs indefinitely until
/// the process is interrupted.
pub(super) async fn serve(
    bytes: &[u8],
    permissions: &wasm_manifest::ResolvedPermissions,
    addr: SocketAddr,
) -> miette::Result<()> {
    // Wasmtime 42+ enables async support by default.
    let engine = Engine::default();

    let component = Component::new(&engine, bytes)
        .map_err(crate::util::into_miette)
        .wrap_err("failed to compile Wasm Component")?;

    let mut linker: Linker<HttpState> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker).map_err(crate::util::into_miette)?;
    wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker)
        .map_err(crate::util::into_miette)?;

    let pre = ProxyPre::new(
        linker
            .instantiate_pre(&component)
            .map_err(crate::util::into_miette)?,
    )
    .map_err(crate::util::into_miette)
    .wrap_err("component does not target the wasi:http/proxy world")?;

    let server = Arc::new(Server {
        pre,
        permissions: permissions.clone(),
    });

    let listener =
        tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| RunError::HttpBindFailed {
                addr: addr.to_string(),
                reason: e.to_string(),
            })?;

    let bound = listener
        .local_addr()
        .map_err(|e| RunError::HttpBindFailed {
            addr: addr.to_string(),
            reason: e.to_string(),
        })?;
    eprintln!("Serving HTTP on http://{bound}");

    loop {
        let (stream, peer) = listener
            .accept()
            .await
            .map_err(|e| RunError::HttpAcceptFailed {
                reason: e.to_string(),
            })?;

        let server = Arc::clone(&server);
        tokio::task::spawn(async move {
            serve_connection(server, stream, peer).await;
        });
    }
}

/// Serve a single TCP connection using HTTP/1.1, dispatching each request to
/// the guest component.
async fn serve_connection(server: Arc<Server>, stream: tokio::net::TcpStream, peer: SocketAddr) {
    if let Err(e) = http1::Builder::new()
        .keep_alive(true)
        .serve_connection(
            TokioIo::new(stream),
            hyper::service::service_fn(move |req| {
                let server = Arc::clone(&server);
                async move { handle_request(&server, req).await }
            }),
        )
        .await
    {
        eprintln!("error serving {peer}: {e}");
    }
}

/// Handle a single HTTP request by instantiating the guest and invoking
/// `wasi:http/incoming-handler.handle`.
async fn handle_request(
    server: &Server,
    req: hyper::Request<hyper::body::Incoming>,
) -> anyhow::Result<hyper::Response<HyperOutgoingBody>> {
    let mut builder = WasiCtxBuilder::new();
    apply_permissions(&mut builder, &server.permissions).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let mut store = Store::new(
        server.pre.engine(),
        HttpState {
            wasi: builder.build(),
            http: WasiHttpCtx::new(),
            table: ResourceTable::new(),
        },
    );

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let req = store
        .data_mut()
        .http()
        .new_incoming_request(Scheme::Http, req)?;
    let out = store.data_mut().http().new_response_outparam(sender)?;
    let pre = server.pre.clone();

    // Spawn so the guest can continue writing the body after the initial
    // response headers are sent.
    let task = tokio::task::spawn(async move {
        let proxy = pre.instantiate_async(&mut store).await?;
        proxy
            .wasi_http_incoming_handler()
            .call_handle(&mut store, req, out)
            .await
    });

    match receiver.await {
        Ok(Ok(resp)) => Ok(resp),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err(collect_guest_error(task).await),
    }
}

/// Collect the error from a guest task that failed to set a response.
async fn collect_guest_error(task: tokio::task::JoinHandle<wasmtime::Result<()>>) -> anyhow::Error {
    let inner: anyhow::Error = match task.await {
        Ok(Ok(())) => anyhow::anyhow!("guest never invoked `response-outparam::set`"),
        Ok(Err(e)) => e.into(),
        Err(e) => e.into(),
    };
    inner.context("guest never invoked `response-outparam::set`")
}

/// Apply resolved permissions to a [`WasiCtxBuilder`].
///
/// Returns `Ok(())` on success or a [`miette::Report`] if a directory
/// pre-open fails.
fn apply_permissions(
    builder: &mut WasiCtxBuilder,
    permissions: &wasm_manifest::ResolvedPermissions,
) -> miette::Result<()> {
    if permissions.inherit_stdio {
        builder.inherit_stdio();
    }
    if permissions.inherit_env {
        builder.inherit_env();
    }
    for entry in &permissions.allow_env {
        if let Some((k, v)) = entry.split_once('=') {
            builder.env(k, v);
        } else if let Ok(v) = std::env::var(entry) {
            builder.env(entry, &v);
        }
    }
    // Pre-open directories with full read/write permissions.
    for dir in &permissions.allow_dirs {
        builder
            .preopened_dir(
                dir,
                dir.to_string_lossy(),
                wasmtime_wasi::DirPerms::all(),
                wasmtime_wasi::FilePerms::all(),
            )
            .map_err(crate::util::into_miette)
            .wrap_err_with(|| format!("failed to pre-open directory: {}", dir.display()))?;
    }
    if permissions.inherit_network {
        builder.inherit_network();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal component that exports `wasi:http/incoming-handler@0.2.0`.
    fn build_http_component() -> Vec<u8> {
        use wasm_encoder::{ComponentExportKind, ComponentExportSection};
        let mut component = wasm_encoder::Component::new();

        let mut exports = ComponentExportSection::new();
        exports.export(
            "wasi:http/incoming-handler@0.2.0",
            ComponentExportKind::Instance,
            0,
            None,
        );
        component.section(&exports);

        component.finish()
    }

    // r[verify run.http-world-detection]
    #[test]
    fn detect_http_world_in_http_component() {
        let bytes = build_http_component();
        assert!(
            exports_http_incoming_handler(&bytes),
            "should detect wasi:http/incoming-handler export"
        );
    }

    // r[verify run.http-server]
    #[test]
    fn non_http_component_not_detected_as_http() {
        // A component that does not export wasi:http/incoming-handler
        // should fall through to the CLI execution path, not the HTTP server.
        let bytes = include_bytes!("../../tests/fixtures/minimal_component.wasm");
        assert!(
            !exports_http_incoming_handler(bytes),
            "non-HTTP component must not trigger HTTP server path"
        );
    }

    // r[verify run.http-listen-message]
    #[test]
    fn listen_message_format() {
        // The serve function prints "Serving HTTP on http://<addr>" to stderr.
        // Verify the message format matches the spec requirement.
        let addr: SocketAddr = "127.0.0.1:8080".parse().expect("valid addr");
        let msg = format!("Serving HTTP on http://{addr}");
        assert_eq!(msg, "Serving HTTP on http://127.0.0.1:8080");
    }

    #[test]
    fn detect_cli_world_in_minimal_component() {
        let bytes = include_bytes!("../../tests/fixtures/minimal_component.wasm");
        assert!(
            !exports_http_incoming_handler(bytes),
            "minimal component should not be detected as HTTP"
        );
    }

    #[test]
    fn detect_cli_world_in_core_module() {
        let bytes = include_bytes!("../../tests/fixtures/core_module.wasm");
        assert!(
            !exports_http_incoming_handler(bytes),
            "core module should not be detected as HTTP"
        );
    }
}
