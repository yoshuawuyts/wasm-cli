#![allow(clippy::print_stderr)]

//! Internal crate for executing WebAssembly components via Wasmtime.
//!
//! This crate is **not** intended for third-party consumption — it is an
//! implementation detail of `wasm-cli`. The API may change without notice.
//!
//! It provides two entry points:
//!
//! - [`validate_component`] — checks that a byte slice is a Wasm Component
//!   (not a core module or WIT-only package).
//! - [`execute_cli_component`] — builds the Wasmtime runtime, wires WASI
//!   permissions, instantiates the component, and invokes
//!   `wasi:cli/run@0.2.0#run`.

mod errors;

use miette::Context;
use wasmparser::{Encoding, Parser, Payload};
use wasmtime::component::Component;
use wasmtime::{Engine, Store};
use wasmtime_wasi::p2::bindings::sync::Command;
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtxBuilder, WasiCtxView, WasiView};

pub use errors::RunError;

/// Host state wired into `Store<WasiState>`.
struct WasiState {
    ctx: wasmtime_wasi::WasiCtx,
    table: ResourceTable,
}

impl WasiView for WasiState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

/// Confirm the bytes are a Wasm Component (not a core module or WIT-only package).
///
/// # Errors
///
/// Returns a [`RunError`] if the bytes are a core module, invalid binary, or
/// have no version header.
pub fn validate_component(bytes: &[u8]) -> miette::Result<()> {
    let parser = Parser::new(0);
    for payload in parser.parse_all(bytes) {
        match payload {
            Ok(Payload::Version { encoding, .. }) => {
                return match encoding {
                    Encoding::Component => Ok(()),
                    Encoding::Module => Err(RunError::CoreModule.into()),
                };
            }
            Err(e) => {
                return Err(RunError::InvalidBinary {
                    reason: e.to_string(),
                }
                .into());
            }
            _ => {}
        }
    }
    Err(RunError::NoVersionHeader.into())
}

/// Build the Wasmtime runtime, instantiate the component, and invoke
/// `wasi:cli/run@0.2.0#run`.
///
/// Returns `Ok(Ok(()))` on success, `Ok(Err(()))` when the guest returns a
/// non-zero exit code, or a [`miette::Report`] on runtime failures.
///
/// # Errors
///
/// Returns a [`miette::Report`] when compilation, instantiation, or WASI
/// context setup fails.
pub fn execute_cli_component(
    bytes: &[u8],
    permissions: &wasm_manifest::ResolvedPermissions,
) -> miette::Result<Result<(), ()>> {
    let engine = Engine::default();
    let component = Component::new(&engine, bytes)
        .map_err(into_miette)
        .wrap_err("failed to compile Wasm Component")?;

    // Build WASI context from resolved permissions.
    let mut builder = WasiCtxBuilder::new();

    if permissions.inherit_stdio {
        builder.inherit_stdio();
    }
    if permissions.inherit_env {
        builder.inherit_env();
    }
    // Forward explicitly allowed env vars.
    // Entries containing '=' are treated as KEY=VAL pairs (from --env flags);
    // entries without '=' are treated as variable names to look up from the host.
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
                DirPerms::all(),
                FilePerms::all(),
            )
            .map_err(into_miette)
            .wrap_err_with(|| format!("failed to pre-open directory: {}", dir.display()))?;
    }
    if permissions.inherit_network {
        builder.inherit_network();
    }

    let wasi_ctx = builder.build();
    let state = WasiState {
        ctx: wasi_ctx,
        table: ResourceTable::new(),
    };
    let mut store = Store::new(&engine, state);

    let mut linker = wasmtime::component::Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(into_miette)?;

    let command = Command::instantiate(&mut store, &component, &linker)
        .map_err(into_miette)
        .wrap_err("failed to instantiate Wasm Component")?;

    let result = command.wasi_cli_run().call_run(&mut store);
    match result {
        Ok(Ok(())) => Ok(Ok(())),
        Ok(Err(())) => {
            eprintln!("Error: guest returned a non-zero exit code");
            Ok(Err(()))
        }
        Err(e) => {
            eprintln!("Error: {e:#}");
            Ok(Err(()))
        }
    }
}

/// Convert an error into a [`miette::Report`], preserving the cause chain.
fn into_miette(err: impl std::fmt::Display) -> miette::Report {
    miette::miette!("{err:#}")
}
