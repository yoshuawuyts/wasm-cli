#![allow(unreachable_pub)]

mod app;
/// TUI components
pub mod components;
/// TUI views
pub mod views;

use app::App;
use tokio::sync::mpsc;
use wasm_package_manager::{
    ImageEntry, KnownPackage, Manager, ProgressEvent, PullResult, Reference, StateInfo,
    WitInterface,
};

/// Events sent from the TUI to the Manager
#[derive(Debug)]
pub enum AppEvent {
    /// Request to quit the application
    Quit,
    /// Request the list of packages
    RequestPackages,
    /// Request state info
    RequestStateInfo,
    /// Pull a package from a registry
    Pull(String),
    /// Delete a package by its reference
    Delete(String),
    /// Search for known packages
    SearchPackages(String),
    /// Request all known packages
    RequestKnownPackages,
    /// Refresh tags for a package (registry, repository)
    RefreshTags(String, String),
    /// Request all WIT interfaces
    RequestWitInterfaces,
    /// Request to detect local WASM files
    DetectLocalWasm,
}

/// Events sent from the Manager to the TUI
#[derive(Debug)]
pub enum ManagerEvent {
    /// Manager has finished initializing
    Ready,
    /// List of packages
    PackagesList(Vec<ImageEntry>),
    /// State information
    StateInfo(StateInfo),
    /// Result of a pull operation (includes InsertResult to indicate if package was new or already existed)
    PullResult(Result<Box<PullResult>, String>),
    /// Result of a delete operation
    DeleteResult(Result<(), String>),
    /// Search results for known packages
    SearchResults(Vec<KnownPackage>),
    /// All known packages
    KnownPackagesList(Vec<KnownPackage>),
    /// Result of refreshing tags for a package
    RefreshTagsResult(Result<usize, String>),
    /// List of WIT interfaces with their component references
    WitInterfacesList(Vec<(WitInterface, String)>),
    /// List of local WASM files
    LocalWasmList(Vec<wasm_detector::WasmEntry>),
    /// Progress event during a pull operation
    PullProgress(ProgressEvent),
}

/// Run the TUI application
pub async fn run(offline: bool) -> anyhow::Result<()> {
    // Create channels for bidirectional communication
    let (app_sender, app_receiver) = mpsc::channel::<AppEvent>(32);
    let (manager_sender, manager_receiver) = mpsc::channel::<ManagerEvent>(32);

    // Run the TUI in a blocking task (separate thread) since it has a synchronous event loop
    let tui_handle = tokio::task::spawn_blocking(move || {
        let terminal = ratatui::init();
        let res = App::new(app_sender, manager_receiver, offline).run(terminal);
        ratatui::restore();
        res
    });

    // Run the manager on the current task using LocalSet (Manager is not Send)
    let local = tokio::task::LocalSet::new();
    local
        .run_until(run_manager(app_receiver, manager_sender, offline))
        .await?;

    // Wait for TUI to finish
    tui_handle.await??;

    Ok(())
}

async fn run_manager(
    mut receiver: mpsc::Receiver<AppEvent>,
    sender: mpsc::Sender<ManagerEvent>,
    offline: bool,
) -> Result<(), anyhow::Error> {
    let manager = if offline {
        Manager::open_offline().await?
    } else {
        Manager::open().await?
    };
    sender.send(ManagerEvent::Ready).await.ok();

    while let Some(event) = receiver.recv().await {
        match event {
            AppEvent::Quit => break,
            AppEvent::RequestPackages => {
                if let Ok(packages) = manager.list_all() {
                    sender.send(ManagerEvent::PackagesList(packages)).await.ok();
                }
            }
            AppEvent::RequestStateInfo => {
                let state_info = manager.state_info();
                sender.send(ManagerEvent::StateInfo(state_info)).await.ok();
            }
            AppEvent::Pull(reference_str) => {
                let result = match reference_str.parse::<Reference>() {
                    Ok(reference) => {
                        let (progress_tx, mut progress_rx) =
                            tokio::sync::mpsc::channel::<ProgressEvent>(64);
                        let sender_clone = sender.clone();
                        // Forward progress events to the TUI
                        let forwarder = tokio::task::spawn_local(async move {
                            while let Some(event) = progress_rx.recv().await {
                                sender_clone
                                    .send(ManagerEvent::PullProgress(event))
                                    .await
                                    .ok();
                            }
                        });
                        let pull_result = manager
                            .pull_with_progress(reference, &progress_tx)
                            .await
                            .map(Box::new)
                            .map_err(|e| e.to_string());
                        drop(progress_tx);
                        let _ = forwarder.await;
                        pull_result
                    }
                    Err(e) => Err(format!("Invalid reference: {}", e)),
                };
                sender.send(ManagerEvent::PullResult(result)).await.ok();
                // Refresh the packages list after pull (only if it was newly inserted)
                if let Ok(packages) = manager.list_all() {
                    sender.send(ManagerEvent::PackagesList(packages)).await.ok();
                }
            }
            AppEvent::Delete(reference_str) => {
                let result = match reference_str.parse::<Reference>() {
                    Ok(reference) => manager
                        .delete(reference)
                        .await
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                    Err(e) => Err(format!("Invalid reference: {}", e)),
                };
                sender.send(ManagerEvent::DeleteResult(result)).await.ok();
                // Refresh the packages list after delete
                if let Ok(packages) = manager.list_all() {
                    sender.send(ManagerEvent::PackagesList(packages)).await.ok();
                }
            }
            AppEvent::SearchPackages(query) => {
                // Use default pagination: offset 0, limit 100
                if let Ok(packages) = manager.search_packages(&query, 0, 100) {
                    sender
                        .send(ManagerEvent::SearchResults(packages))
                        .await
                        .ok();
                }
            }
            AppEvent::RequestKnownPackages => {
                // Use default pagination: offset 0, limit 100
                if let Ok(packages) = manager.list_known_packages(0, 100) {
                    sender
                        .send(ManagerEvent::KnownPackagesList(packages))
                        .await
                        .ok();
                }
            }
            AppEvent::RefreshTags(registry, repository) => {
                // Create a reference to fetch tags
                let reference_str = format!("{}/{}:latest", registry, repository);
                let result = match reference_str.parse::<Reference>() {
                    Ok(reference) => match manager.list_tags(&reference).await {
                        Ok(tags) => {
                            let tag_count = tags.len();
                            // Store all fetched tags as known packages
                            for tag in tags {
                                let _ = manager.add_known_package(
                                    &registry,
                                    &repository,
                                    Some(&tag),
                                    None,
                                );
                            }
                            Ok(tag_count)
                        }
                        Err(e) => Err(e.to_string()),
                    },
                    Err(e) => Err(format!("Invalid reference: {}", e)),
                };
                sender
                    .send(ManagerEvent::RefreshTagsResult(result))
                    .await
                    .ok();
                // Refresh known packages list after updating tags
                // Use default pagination: offset 0, limit 100
                if let Ok(packages) = manager.list_known_packages(0, 100) {
                    sender
                        .send(ManagerEvent::KnownPackagesList(packages))
                        .await
                        .ok();
                }
            }
            AppEvent::RequestWitInterfaces => {
                if let Ok(interfaces) = manager.list_wit_interfaces_with_components() {
                    sender
                        .send(ManagerEvent::WitInterfacesList(interfaces))
                        .await
                        .ok();
                }
            }
            AppEvent::DetectLocalWasm => {
                // Detect local WASM files in the current directory
                let detector = wasm_detector::WasmDetector::new(std::path::Path::new("."));
                let wasm_files: Vec<_> = detector.into_iter().filter_map(Result::ok).collect();
                sender
                    .send(ManagerEvent::LocalWasmList(wasm_files))
                    .await
                    .ok();
            }
        }
    }

    Ok(())
}
