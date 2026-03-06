#![allow(clippy::print_stdout, clippy::print_stderr)]

//! HTTP server for components targeting the `wasi:http/proxy` world.
//!
//! When a component exports `wasi:http/incoming-handler`, this module starts a
//! local HTTP server that forwards each incoming request to the guest.

use std::net::SocketAddr;
use std::sync::Arc;

use hyper::server::conn::http1;
use miette::Context;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::bindings::http::types::Scheme;
use wasmtime_wasi_http::bindings::ProxyPre;
use wasmtime_wasi_http::body::HyperOutgoingBody;
use wasmtime_wasi_http::io::TokioIo;
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

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
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

/// Shared server state holding the pre-instantiated component and
/// resolved permissions for building per-request WASI contexts.
struct Server {
    pre: ProxyPre<HttpState>,
    permissions: wasm_manifest::ResolvedPermissions,
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
    let engine =
        Engine::default();

    let component = Component::new(&engine, bytes)
        .map_err(crate::util::into_miette)
        .wrap_err("failed to compile Wasm Component")?;

    let mut linker: Linker<HttpState> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)
        .map_err(crate::util::into_miette)?;
    wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)
        .map_err(crate::util::into_miette)?;

    let pre = ProxyPre::new(linker.instantiate_pre(&component).map_err(crate::util::into_miette)?)
        .map_err(crate::util::into_miette)
        .wrap_err("component does not target the wasi:http/proxy world")?;

    let server = Arc::new(Server {
        pre,
        permissions: permissions.clone(),
    });

    let listener = tokio::net::TcpListener::bind(addr)
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
        let (stream, peer) = listener.accept().await.map_err(|e| RunError::HttpAcceptFailed {
            reason: e.to_string(),
        })?;

        let server = Arc::clone(&server);
        tokio::task::spawn(async move {
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
        });
    }
}

/// Handle a single HTTP request by instantiating the guest and invoking
/// `wasi:http/incoming-handler.handle`.
async fn handle_request(
    server: &Server,
    req: hyper::Request<hyper::body::Incoming>,
) -> anyhow::Result<hyper::Response<HyperOutgoingBody>> {
    let mut builder = WasiCtxBuilder::new();
    apply_permissions(&mut builder, &server.permissions);

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
        .new_incoming_request(Scheme::Http, req)?;
    let out = store.data_mut().new_response_outparam(sender)?;
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
        Err(_) => {
            let e = match task.await {
                Ok(Ok(())) => {
                    anyhow::anyhow!("guest never invoked `response-outparam::set`")
                }
                Ok(Err(e)) => e.into(),
                Err(e) => e.into(),
            };
            Err(e.context("guest never invoked `response-outparam::set`"))
        }
    }
}

/// Apply resolved permissions to a [`WasiCtxBuilder`].
fn apply_permissions(
    builder: &mut WasiCtxBuilder,
    permissions: &wasm_manifest::ResolvedPermissions,
) {
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
    if permissions.inherit_network {
        builder.inherit_network();
    }
}
