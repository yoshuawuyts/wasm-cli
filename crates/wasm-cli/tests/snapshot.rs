//! Snapshot tests for TUI views using the `insta` crate.
//!
//! These tests render each view to a buffer and snapshot the result to ensure
//! consistent rendering across changes.
//!
//! # Running Snapshot Tests
//!
//! Run tests with: `cargo test --package wasm`
//!
//! # Updating Snapshots
//!
//! When views change intentionally, update snapshots with:
//! `cargo insta review` or `cargo insta accept`
//!
//! Install the insta CLI with: `cargo install cargo-insta`
//!
//! # Test Coverage Guidelines
//!
//! Every TUI view and component should have at least one snapshot test covering:
//! - Empty/loading state (when applicable)
//! - Populated state with sample data
//! - Interactive states (filter active, search active, etc.)
//!
//! When adding new views or components, add corresponding snapshot tests.

use std::path::PathBuf;

use insta::assert_snapshot;
use oci_client::manifest::{IMAGE_MANIFEST_MEDIA_TYPE, OciDescriptor, OciImageManifest};
use ratatui::prelude::*;

use wasm::tui::components::{TabBar, TabItem};
use wasm::tui::views::packages::PackagesViewState;
use wasm::tui::views::{
    InterfacesView, InterfacesViewState, KnownPackageDetailView, LocalView, LogView,
    PackageDetailView, PackagesView, SearchView, SearchViewState, SettingsView,
};
use wasm_detector::WasmEntry;
use wasm_package_manager::{ImageView, KnownPackageView, StateInfo};

/// Helper function to render a widget to a string buffer.
fn render_to_string<W: Widget>(widget: W, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    widget.render(area, &mut buffer);
    buffer_to_string(&buffer)
}

/// Helper function to render a stateful widget to a string buffer.
fn render_stateful_to_string<W, S>(widget: W, state: &mut S, width: u16, height: u16) -> String
where
    W: StatefulWidget<State = S>,
{
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    widget.render(area, &mut buffer, state);
    buffer_to_string(&buffer)
}

/// Convert a buffer to a string representation for snapshot testing.
fn buffer_to_string(buffer: &Buffer) -> String {
    let mut output = String::new();
    for y in 0..buffer.area.height {
        let line_start = output.len();
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            output.push_str(cell.symbol());
        }
        // Trim trailing spaces using truncate to avoid allocation
        let trimmed_len = output[line_start..].trim_end().len() + line_start;
        output.truncate(trimmed_len);
        output.push('\n');
    }
    output
}

/// Creates a minimal OCI image manifest with a single WASM layer for testing.
fn test_manifest() -> OciImageManifest {
    OciImageManifest {
        schema_version: 2,
        media_type: Some(IMAGE_MANIFEST_MEDIA_TYPE.to_string()),
        config: OciDescriptor {
            media_type: "application/vnd.oci.image.config.v1+json".to_string(),
            digest: "sha256:abc123".to_string(),
            size: 100,
            urls: None,
            annotations: None,
        },
        layers: vec![OciDescriptor {
            media_type: "application/wasm".to_string(),
            digest: "sha256:def456".to_string(),
            size: 1024,
            urls: None,
            annotations: None,
        }],
        artifact_type: None,
        annotations: None,
        subject: None,
    }
}

// =============================================================================
// LocalView Snapshot Tests
// =============================================================================

#[test]
fn test_local_view_empty_snapshot() {
    let wasm_files = vec![];
    let output = render_to_string(LocalView::new(&wasm_files), 60, 10);
    assert_snapshot!(output);
}

#[test]
fn test_local_view_with_files_snapshot() {
    let wasm_files = vec![
        WasmEntry::new(PathBuf::from(
            "./target/wasm32-unknown-unknown/release/app.wasm",
        )),
        WasmEntry::new(PathBuf::from("./pkg/component.wasm")),
        WasmEntry::new(PathBuf::from("./examples/hello.wasm")),
    ];
    let output = render_to_string(LocalView::new(&wasm_files), 80, 15);
    assert_snapshot!(output);
}

// =============================================================================
// InterfacesView Snapshot Tests
// =============================================================================

#[test]
fn test_interfaces_view_snapshot() {
    let interfaces = vec![];
    let mut state = InterfacesViewState::new();
    let output = render_stateful_to_string(InterfacesView::new(&interfaces), &mut state, 60, 10);
    assert_snapshot!(output);
}

#[test]
fn test_interfaces_view_populated_snapshot() {
    use wasm_package_manager::WitInterfaceView;

    let interfaces = vec![
        (
            WitInterfaceView {
                package_name: "wasi:http".to_string(),
                version: Some("0.2.0".to_string()),
                description: None,
                wit_text: Some("package wasi:http@0.2.0;\n\nworld proxy {\n  import wasi:http/types;\n  export wasi:http/handler;\n}".to_string()),
                created_at: "2024-01-15T10:30:00Z".to_string(),
            },
            "ghcr.io/example/http-proxy:v1.0.0".to_string(),
        ),
        (
            WitInterfaceView {
                package_name: "wasi:cli".to_string(),
                version: Some("0.2.0".to_string()),
                description: None,
                wit_text: Some("package wasi:cli@0.2.0;\n\nworld command {\n  import wasi:cli/stdin;\n  import wasi:cli/stdout;\n  export run;\n}".to_string()),
                created_at: "2024-01-16T11:20:00Z".to_string(),
            },
            "ghcr.io/example/cli-tool:latest".to_string(),
        ),
    ];
    let mut state = InterfacesViewState::new();
    let output = render_stateful_to_string(InterfacesView::new(&interfaces), &mut state, 100, 15);
    assert_snapshot!(output);
}

// =============================================================================
// PackagesView Snapshot Tests
// =============================================================================

#[test]
fn test_packages_view_empty_snapshot() {
    let packages = vec![];
    let output = render_to_string(PackagesView::new(&packages), 80, 15);
    assert_snapshot!(output);
}

#[test]
fn test_packages_view_with_packages_snapshot() {
    let packages = vec![
        ImageView {
            ref_registry: "ghcr.io".to_string(),
            ref_repository: "bytecode-alliance/wasmtime".to_string(),
            ref_mirror_registry: None,
            ref_tag: Some("v1.0.0".to_string()),
            ref_digest: Some("sha256:abc123def456".to_string()),
            manifest: test_manifest(),
            size_on_disk: 1024 * 1024 * 5, // 5 MB
        },
        ImageView {
            ref_registry: "docker.io".to_string(),
            ref_repository: "example/hello-wasm".to_string(),
            ref_mirror_registry: None,
            ref_tag: Some("latest".to_string()),
            ref_digest: None,
            manifest: test_manifest(),
            size_on_disk: 1024 * 512, // 512 KB
        },
        ImageView {
            ref_registry: "ghcr.io".to_string(),
            ref_repository: "user/my-component".to_string(),
            ref_mirror_registry: None,
            ref_tag: Some("v2.1.0".to_string()),
            ref_digest: Some("sha256:789xyz".to_string()),
            manifest: test_manifest(),
            size_on_disk: 1024 * 1024 * 2, // 2 MB
        },
    ];
    let output = render_to_string(PackagesView::new(&packages), 100, 15);
    assert_snapshot!(output);
}

#[test]
fn test_packages_view_with_filter_active_snapshot() {
    let packages = vec![];
    let mut state = PackagesViewState::new();
    state.filter_active = true;
    state.filter_query = "wasi".to_string();
    let output = render_stateful_to_string(PackagesView::new(&packages), &mut state, 100, 15);
    assert_snapshot!(output);
}

#[test]
fn test_packages_view_filter_with_results_snapshot() {
    let packages = vec![ImageView {
        ref_registry: "ghcr.io".to_string(),
        ref_repository: "bytecode-alliance/wasi-http".to_string(),
        ref_mirror_registry: None,
        ref_tag: Some("v0.2.0".to_string()),
        ref_digest: Some("sha256:wasi123".to_string()),
        manifest: test_manifest(),
        size_on_disk: 1024 * 256, // 256 KB
    }];
    let mut state = PackagesViewState::new();
    state.filter_query = "wasi".to_string();
    let output = render_stateful_to_string(PackagesView::new(&packages), &mut state, 100, 12);
    assert_snapshot!(output);
}

// =============================================================================
// PackageDetailView Snapshot Tests
// =============================================================================

#[test]
fn test_package_detail_view_snapshot() {
    let package = ImageView {
        ref_registry: "ghcr.io".to_string(),
        ref_repository: "bytecode-alliance/wasmtime".to_string(),
        ref_mirror_registry: None,
        ref_tag: Some("v1.0.0".to_string()),
        ref_digest: Some("sha256:abc123def456789".to_string()),
        manifest: test_manifest(),
        size_on_disk: 1024 * 1024 * 5, // 5 MB
    };
    let output = render_to_string(PackageDetailView::new(&package), 80, 25);
    assert_snapshot!(output);
}

#[test]
fn test_package_detail_view_without_tag_snapshot() {
    let package = ImageView {
        ref_registry: "docker.io".to_string(),
        ref_repository: "library/hello-world".to_string(),
        ref_mirror_registry: None,
        ref_tag: None,
        ref_digest: Some("sha256:digest123".to_string()),
        manifest: test_manifest(),
        size_on_disk: 1024 * 128, // 128 KB
    };
    let output = render_to_string(PackageDetailView::new(&package), 80, 25);
    assert_snapshot!(output);
}

// =============================================================================
// SearchView Snapshot Tests
// =============================================================================

#[test]
fn test_search_view_empty_snapshot() {
    let packages = vec![];
    let output = render_to_string(SearchView::new(&packages), 80, 15);
    assert_snapshot!(output);
}

#[test]
fn test_search_view_with_packages_snapshot() {
    let packages = vec![
        KnownPackageView {
            registry: "ghcr.io".to_string(),
            repository: "bytecode-alliance/wasi-http".to_string(),
            description: Some("WASI HTTP interface".to_string()),
            tags: vec!["v0.2.0".to_string(), "v0.1.0".to_string()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: "2024-01-15T10:30:00Z".to_string(),
            created_at: "2024-01-01T08:00:00Z".to_string(),
        },
        KnownPackageView {
            registry: "ghcr.io".to_string(),
            repository: "user/my-component".to_string(),
            description: None,
            tags: vec!["latest".to_string()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: "2024-02-01T12:00:00Z".to_string(),
            created_at: "2024-01-20T09:00:00Z".to_string(),
        },
    ];
    let output = render_to_string(SearchView::new(&packages), 100, 15);
    assert_snapshot!(output);
}

#[test]
fn test_search_view_with_search_active_snapshot() {
    let packages = vec![];
    let mut state = SearchViewState::new();
    state.search_active = true;
    state.search_query = "wasi".to_string();
    let output = render_stateful_to_string(SearchView::new(&packages), &mut state, 100, 15);
    assert_snapshot!(output);
}

#[test]
fn test_search_view_with_many_tags_snapshot() {
    let packages = vec![KnownPackageView {
        registry: "ghcr.io".to_string(),
        repository: "project/component".to_string(),
        description: Some("A component with many tags".to_string()),
        tags: vec![
            "v3.0.0".to_string(),
            "v2.0.0".to_string(),
            "v1.0.0".to_string(),
            "beta".to_string(),
            "alpha".to_string(),
        ],
        signature_tags: vec!["v3.0.0.sig".to_string()],
        attestation_tags: vec!["v3.0.0.att".to_string()],
        last_seen_at: "2024-03-01T10:00:00Z".to_string(),
        created_at: "2023-06-01T08:00:00Z".to_string(),
    }];
    let output = render_to_string(SearchView::new(&packages), 100, 12);
    assert_snapshot!(output);
}

// =============================================================================
// KnownPackageDetailView Snapshot Tests
// =============================================================================

#[test]
fn test_known_package_detail_view_snapshot() {
    let package = KnownPackageView {
        registry: "ghcr.io".to_string(),
        repository: "user/example-package".to_string(),
        description: Some("An example WASM component package".to_string()),
        tags: vec![
            "v1.0.0".to_string(),
            "v0.9.0".to_string(),
            "latest".to_string(),
        ],
        signature_tags: vec!["v1.0.0.sig".to_string()],
        attestation_tags: vec!["v1.0.0.att".to_string()],
        last_seen_at: "2024-01-15T10:30:00Z".to_string(),
        created_at: "2024-01-01T08:00:00Z".to_string(),
    };
    let output = render_to_string(KnownPackageDetailView::new(&package), 80, 20);
    assert_snapshot!(output);
}

#[test]
fn test_known_package_detail_view_minimal_snapshot() {
    let package = KnownPackageView {
        registry: "docker.io".to_string(),
        repository: "library/minimal".to_string(),
        description: None,
        tags: vec!["latest".to_string()],
        signature_tags: vec![],
        attestation_tags: vec![],
        last_seen_at: "2024-02-01T12:00:00Z".to_string(),
        created_at: "2024-02-01T12:00:00Z".to_string(),
    };
    let output = render_to_string(KnownPackageDetailView::new(&package), 80, 15);
    assert_snapshot!(output);
}

// =============================================================================
// SettingsView Snapshot Tests
// =============================================================================

#[test]
fn test_settings_view_loading_snapshot() {
    let output = render_to_string(SettingsView::new(None), 80, 15);
    assert_snapshot!(output);
}

#[test]
fn test_settings_view_with_state_info_snapshot() {
    let state_info = StateInfo::new_at(
        PathBuf::from("/home/user/.local/share/wasm"),
        PathBuf::from("/home/user/.config/wasm/config.toml"),
        wasm_package_manager::Migrations {
            current: 3,
            total: 3,
        },
        1024 * 1024 * 10, // 10 MB
        1024 * 64,        // 64 KB
    )
    .with_executable(PathBuf::from("/usr/local/bin/wasm"));
    let output = render_to_string(SettingsView::new(Some(&state_info)), 100, 15);
    assert_snapshot!(output);
}

// =============================================================================
// TabBar Component Snapshot Tests
// =============================================================================

/// Tab enum for testing the TabBar component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestTab {
    First,
    Second,
    Third,
}

impl TestTab {
    const ALL: [TestTab; 3] = [TestTab::First, TestTab::Second, TestTab::Third];
}

impl TabItem for TestTab {
    fn all() -> &'static [Self] {
        &Self::ALL
    }

    fn title(&self) -> &'static str {
        match self {
            TestTab::First => "First [1]",
            TestTab::Second => "Second [2]",
            TestTab::Third => "Third [3]",
        }
    }
}

#[test]
fn test_tab_bar_first_selected_snapshot() {
    let tab_bar = TabBar::new("Test App - ready", TestTab::First);
    let output = render_to_string(tab_bar, 60, 3);
    assert_snapshot!(output);
}

#[test]
fn test_tab_bar_second_selected_snapshot() {
    let tab_bar = TabBar::new("Test App - ready", TestTab::Second);
    let output = render_to_string(tab_bar, 60, 3);
    assert_snapshot!(output);
}

#[test]
fn test_tab_bar_third_selected_snapshot() {
    let tab_bar = TabBar::new("Test App - ready", TestTab::Third);
    let output = render_to_string(tab_bar, 60, 3);
    assert_snapshot!(output);
}

#[test]
fn test_tab_bar_loading_state_snapshot() {
    let tab_bar = TabBar::new("Test App - loading...", TestTab::First);
    let output = render_to_string(tab_bar, 60, 3);
    assert_snapshot!(output);
}

#[test]
fn test_tab_bar_error_state_snapshot() {
    let tab_bar = TabBar::new("Test App - error occurred!", TestTab::First);
    let output = render_to_string(tab_bar, 60, 3);
    assert_snapshot!(output);
}

// =============================================================================
// LogView Snapshot Tests
// =============================================================================

#[test]
fn test_log_view_empty_snapshot() {
    let lines = vec![];
    let output = render_to_string(LogView::new(&lines, 0), 80, 10);
    assert_snapshot!(output);
}

#[test]
fn test_log_view_with_lines_snapshot() {
    let lines = vec![
        "2024-01-15T10:30:00Z WARN wasm: connection timeout".to_string(),
        "2024-01-15T10:30:01Z WARN wasm: retrying request".to_string(),
        "2024-01-15T10:30:05Z WARN wasm: registry unreachable".to_string(),
    ];
    let output = render_to_string(LogView::new(&lines, 0), 80, 10);
    assert_snapshot!(output);
}

#[test]
fn test_log_view_scrolled_snapshot() {
    let lines: Vec<String> = (1..=20)
        .map(|i| format!("2024-01-15T10:30:{:02}Z WARN wasm: log line {}", i, i))
        .collect();
    let output = render_to_string(LogView::new(&lines, 10), 80, 10);
    assert_snapshot!(output);
}
