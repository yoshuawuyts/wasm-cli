//! `cargo xtask serve` — build and serve the frontend with a local meta-registry.
//!
//! Orchestrates three steps:
//! 1. Build `wasm-frontend` for `wasm32-wasip2`.
//! 2. Start `wasm-meta-registry` in the background.
//! 3. Start `wasmtime serve` for the frontend component.
//!
//! On Ctrl-C both child processes are killed so no ports are left open.

use std::process::{Child, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};

use crate::workspace_root;

/// Run the full frontend development stack.
pub(crate) fn run_serve() -> Result<()> {
    let root = workspace_root()?;

    // 1. Build the frontend component.
    eprintln!(":: Building wasm-frontend for wasm32-wasip2…");
    let build_status = Command::new("cargo")
        .env("API_BASE_URL", "http://127.0.0.1:8081")
        .args([
            "build",
            "--package",
            "wasm-frontend",
            "--target",
            "wasm32-wasip2",
        ])
        .status()
        .context("failed to build wasm-frontend")?;
    if !build_status.success() {
        anyhow::bail!(
            "cargo build failed with exit code: {:?}",
            build_status.code()
        );
    }

    let wasm_path = root
        .join("target/wasm32-wasip2/debug/wasm_frontend.wasm")
        .to_str()
        .expect("workspace root path is valid UTF-8")
        .to_owned();

    let registry_dir = root
        .join("registry")
        .to_str()
        .expect("workspace root path is valid UTF-8")
        .to_owned();

    // 2. Start the meta-registry.
    eprintln!(":: Starting meta-registry on 127.0.0.1:8081…");
    let mut registry_child = Command::new("cargo")
        .args([
            "run",
            "--package",
            "wasm-meta-registry",
            "--",
            &registry_dir,
            "--bind",
            "127.0.0.1:8081",
        ])
        .spawn()
        .context("failed to start wasm-meta-registry")?;

    // 3. Start wasmtime serve.
    eprintln!(":: Starting wasmtime serve on 127.0.0.1:8080…");
    let mut wasmtime_child = Command::new("wasmtime")
        .args([
            "serve",
            "--listen",
            "127.0.0.1:8080",
            "-Scli",
            "-Sinherit-network",
            "-Sallow-ip-name-lookup",
            &wasm_path,
        ])
        .spawn()
        .context("failed to start wasmtime serve")?;

    // Install a Ctrl-C handler that flags shutdown.
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_flag = Arc::clone(&shutdown);
    ctrlc::set_handler(move || {
        shutdown_flag.store(true, Ordering::SeqCst);
    })
    .context("failed to install Ctrl-C handler")?;

    // Wait for either process to exit or Ctrl-C.
    loop {
        if shutdown.load(Ordering::SeqCst) {
            eprintln!("\n:: Shutting down…");
            break;
        }

        // Check if wasmtime exited on its own.
        if let Some(status) = wasmtime_child
            .try_wait()
            .context("failed to poll wasmtime")?
        {
            eprintln!(":: wasmtime serve exited with {status}");
            break;
        }

        // Check if registry exited on its own.
        if let Some(status) = registry_child
            .try_wait()
            .context("failed to poll meta-registry")?
        {
            eprintln!(":: meta-registry exited with {status}");
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    kill_child(&mut wasmtime_child, "wasmtime serve");
    kill_child(&mut registry_child, "meta-registry");

    Ok(())
}

/// Kill a child process, ignoring errors if it already exited.
fn kill_child(child: &mut Child, name: &str) {
    if let Err(e) = child.kill() {
        // "InvalidInput" means the process already exited — that's fine.
        if e.kind() != std::io::ErrorKind::InvalidInput {
            eprintln!("warning: failed to kill {name}: {e}");
        }
    } else {
        let _ = child.wait();
    }
}
