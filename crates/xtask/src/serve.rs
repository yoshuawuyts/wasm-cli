//! `cargo xtask serve` — build and serve the frontend with a local meta-registry.
//!
//! Orchestrates three steps:
//! 1. Build `wasm-frontend` for `wasm32-wasip2`.
//! 2. Start `wasm-meta-registry` in the background.
//! 3. Start `wasmtime serve` for the frontend component.
//!
//! Watches `crates/wasm-frontend/src/` for changes and automatically rebuilds
//! and restarts only the frontend (wasmtime) — the registry stays running.
//! On Ctrl-C both child processes are killed so no ports are left open.

use std::io::BufRead;
use std::process::{Child, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{RecursiveMode, Watcher};

use crate::workspace_root;

/// Run the full frontend development stack.
pub(crate) fn run_serve() -> Result<()> {
    let root = workspace_root()?;

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

    // Initial build.
    build_frontend(&root)?;

    // Start servers.
    let mut registry_child = start_registry(&registry_dir)?;
    let mut wasmtime_child = start_wasmtime(&wasm_path)?;

    // Install a Ctrl-C handler that flags shutdown.
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_flag = Arc::clone(&shutdown);
    ctrlc::set_handler(move || {
        shutdown_flag.store(true, Ordering::SeqCst);
    })
    .context("failed to install Ctrl-C handler")?;

    // Spawn a thread to read stdin for Enter presses.
    let reload = Arc::new(AtomicBool::new(false));
    let reload_flag = Arc::clone(&reload);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for _ in stdin.lock().lines() {
            reload_flag.store(true, Ordering::SeqCst);
        }
    });

    // Watch frontend source for changes.
    let fs_reload = Arc::clone(&reload);
    let watch_path = root.join("crates/wasm-frontend/src");
    let mut watcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res
                && (event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove())
            {
                fs_reload.store(true, Ordering::SeqCst);
            }
        })
        .context("failed to create file watcher")?;
    watcher
        .watch(&watch_path, RecursiveMode::Recursive)
        .context("failed to watch frontend source directory")?;

    eprintln!(":: Watching crates/wasm-frontend/src/ for changes.");
    eprintln!(":: Press Enter to rebuild, Ctrl-C to quit.");

    // Debounce: wait a short period after a change before rebuilding.
    let mut last_rebuild = Instant::now();

    // Wait for either process to exit, Ctrl-C, or file change.
    loop {
        if shutdown.load(Ordering::SeqCst) {
            eprintln!("\n:: Shutting down…");
            break;
        }

        // Reload on Enter or file change (with debounce).
        if reload.load(Ordering::SeqCst) && last_rebuild.elapsed() > Duration::from_millis(500) {
            reload.store(false, Ordering::SeqCst);
            eprintln!("\n:: Rebuilding frontend…");
            if build_frontend(&root).is_ok() {
                kill_child(&mut wasmtime_child, "wasmtime serve");
                wasmtime_child = start_wasmtime(&wasm_path)?;
                last_rebuild = Instant::now();
                eprintln!(":: Watching for changes. Press Enter to rebuild, Ctrl-C to quit.");
            } else {
                eprintln!(":: Build failed, keeping current server running.");
            }
            continue;
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

        std::thread::sleep(Duration::from_millis(200));
    }

    kill_child(&mut wasmtime_child, "wasmtime serve");
    kill_child(&mut registry_child, "meta-registry");

    Ok(())
}

/// Build the frontend component for wasm32-wasip2.
fn build_frontend(root: &std::path::Path) -> Result<()> {
    eprintln!(":: Building wasm-frontend for wasm32-wasip2…");
    let status = Command::new("cargo")
        .env("API_BASE_URL", "http://127.0.0.1:8081")
        .current_dir(root)
        .args([
            "build",
            "--package",
            "wasm-frontend",
            "--target",
            "wasm32-wasip2",
        ])
        .status()
        .context("failed to build wasm-frontend")?;
    if !status.success() {
        anyhow::bail!("cargo build failed with exit code: {:?}", status.code());
    }
    Ok(())
}

/// Start the meta-registry server.
fn start_registry(registry_dir: &str) -> Result<Child> {
    eprintln!(":: Starting meta-registry on 127.0.0.1:8081…");
    Command::new("cargo")
        .args([
            "run",
            "--package",
            "wasm-meta-registry",
            "--",
            registry_dir,
            "--bind",
            "127.0.0.1:8081",
        ])
        .spawn()
        .context("failed to start wasm-meta-registry")
}

/// Start wasmtime serve for the frontend.
fn start_wasmtime(wasm_path: &str) -> Result<Child> {
    eprintln!(":: Starting wasmtime serve on 127.0.0.1:8080…");
    Command::new("wasmtime")
        .args([
            "serve",
            "--addr",
            "127.0.0.1:8080",
            "-Scli",
            "-Sinherit-network",
            "-Sallow-ip-name-lookup",
            wasm_path,
        ])
        .spawn()
        .context("failed to start wasmtime serve")
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
