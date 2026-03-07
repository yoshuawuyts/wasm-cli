//! Progress bar rendering for the `wasm install` command.
//!
//! This module handles the display of download progress for packages being
//! installed. Each package gets a single aggregated progress bar that combines
//! all layer downloads. Packages are displayed in a tree structure with
//! `├──` and `└──` glyphs.
//!
//! The [`ProgressTree`] type manages the dynamic `└──` glyph: every newly
//! added bar becomes the "last" entry (`└──`), while the previously-last bar
//! is demoted to `├──`.

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use wasm_package_manager::ProgressEvent;

/// Tree glyph for non-last items in the tree.
const TREE_GLYPH_MID: &str = "├──";

/// Tree glyph for the last item in the tree.
const TREE_GLYPH_END: &str = "└──";

/// Return the appropriate tree glyph for a position in a list.
///
/// # Examples
///
/// ```rust,ignore
/// assert_eq!(tree_glyph(false), "├──");
/// assert_eq!(tree_glyph(true), "└──");
/// ```
// r[impl cli.progress-bar.tree-glyph]
fn tree_glyph(is_last: bool) -> &'static str {
    if is_last {
        TREE_GLYPH_END
    } else {
        TREE_GLYPH_MID
    }
}

/// Manages a list of progress bars rendered as a flat tree.
///
/// The last bar always shows `└──`; when a new bar is added, the
/// previously-last bar is demoted to `├──`.
pub(crate) struct ProgressTree {
    multi: MultiProgress,
    /// All bars and their display metadata, in insertion order.
    entries: Vec<TreeEntry>,
    /// Monotonically increasing counter for unique bar IDs.
    next_id: usize,
}

/// Opaque handle returned by [`ProgressTree::add_bar`] to identify a specific
/// progress bar entry when finishing it.  Using an ID instead of the
/// package name avoids misidentification when two installs share the same
/// display name and version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BarId(usize);

/// Metadata kept for each bar so we can rebuild its prefix when the glyph
/// changes (e.g. from `└──` to `├──`).
struct TreeEntry {
    id: BarId,
    bar: ProgressBar,
    name: String,
    version: Option<String>,
    is_complete: bool,
}

impl ProgressTree {
    /// Create a new progress tree backed by the given [`MultiProgress`].
    pub(crate) fn new(multi: MultiProgress) -> Self {
        Self {
            multi,
            entries: Vec::new(),
            next_id: 0,
        }
    }

    /// Add a new in-progress package bar. The bar becomes the new "last"
    /// entry (`└──`), and the previously-last bar (if any) is demoted to
    /// `├──`.
    ///
    /// Returns the [`ProgressBar`] handle and a [`BarId`] that must be
    /// passed to [`finish_bar`](Self::finish_bar) to identify this entry.
    // r[impl cli.progress-bar.bar-yellow]
    // r[impl cli.progress-bar.size-grey]
    // r[impl cli.progress-bar.eta-grey]
    pub(crate) fn add_bar(&mut self, name: &str, version: Option<&str>) -> (ProgressBar, BarId) {
        // Demote the old last entry to ├──
        self.demote_last();

        let prefix = build_prefix(TREE_GLYPH_END, name, version, false);
        let pb = self.multi.add(ProgressBar::new(0));
        // Start with the initial (bytes-only) style; run_progress_bars()
        // will switch to the full bar style once a total is known.
        pb.set_style(initial_style());
        pb.set_prefix(prefix);

        let id = BarId(self.next_id);
        self.next_id += 1;

        self.entries.push(TreeEntry {
            id,
            bar: pb.clone(),
            name: name.to_string(),
            version: version.map(String::from),
            is_complete: false,
        });

        (pb, id)
    }

    /// Mark a progress bar as complete: green name, bar hidden, size only.
    // r[impl cli.progress-bar.bar-hidden-on-complete]
    pub(crate) fn finish_bar(&mut self, pb: &ProgressBar, bar_id: BarId) {
        // Find this bar's index in the entries list by unique ID.
        let idx = self.entries.iter().position(|e| e.id == bar_id);

        // Build the prefix from the stored entry (immutable borrow), then
        // apply it to the progress bar before we take a mutable borrow to
        // mark the entry complete.
        let prefix = if let Some(entry) = idx.and_then(|i| self.entries.get(i)) {
            let is_last = idx == Some(self.entries.len() - 1);
            build_prefix(
                tree_glyph(is_last),
                &entry.name,
                entry.version.as_deref(),
                true,
            )
        } else {
            tracing::debug!("finish_bar called with unknown BarId({bar_id:?})");
            build_prefix(tree_glyph(true), "", None, true)
        };

        pb.set_style(done_style());
        pb.set_prefix(prefix);
        pb.finish();

        // Update the stored entry's completion state.
        if let Some(entry) = idx.and_then(|i| self.entries.get_mut(i)) {
            entry.is_complete = true;
        }
    }

    /// Demote the current "last" entry from `└──` to `├──`.
    fn demote_last(&mut self) {
        if let Some(entry) = self.entries.last() {
            let prefix = build_prefix(
                TREE_GLYPH_MID,
                &entry.name,
                entry.version.as_deref(),
                entry.is_complete,
            );
            entry.bar.set_prefix(prefix);
        }
    }
}

/// Extract the display name and version from a package reference.
///
/// For WIT-style names like `wasi:http@0.2.0`, the name is `wasi:http` and
/// version is `0.2.0`. For WIT-style names without version like `wasi:http`,
/// the version is taken from the OCI reference tag (stripping a leading `v`).
///
/// When `explicit_name` is `None`, the returned name is empty and the caller
/// must provide a fallback (e.g. from `reference.repository()`).
// r[impl cli.progress-bar.namespace-name]
pub(crate) fn package_display_parts(
    explicit_name: Option<&str>,
    tag: Option<&str>,
) -> (String, Option<String>) {
    if let Some(name) = explicit_name {
        if let Some((n, v)) = name.split_once('@') {
            (n.to_string(), Some(v.to_string()))
        } else {
            let version = tag.map(|t| t.strip_prefix('v').unwrap_or(t).to_string());
            (name.to_string(), version)
        }
    } else {
        // For OCI references without an explicit name, fall back to tag only
        let version = tag.map(|t| t.strip_prefix('v').unwrap_or(t).to_string());
        (String::new(), version)
    }
}

/// Derive a `namespace:name` display string from an OCI repository path.
///
/// OCI repository paths typically look like `ghcr.io/webassembly/wasi-logging`.
/// This function takes the last two `/`-separated segments and joins them with
/// `:` so that the display matches the `namespace:name` format used elsewhere
/// in the tree output.  When fewer than two segments are available, the full
/// repository path is returned as-is.
// r[impl cli.progress-bar.namespace-name]
pub(crate) fn oci_repo_display_name(repo: &str) -> String {
    let mut parts = repo.rsplitn(3, '/');
    let last = parts.next();
    let second_last = parts.next();
    match (second_last, last) {
        (Some(namespace), Some(package)) if !namespace.is_empty() && !package.is_empty() => {
            format!("{namespace}:{package}")
        }
        _ => repo.to_string(),
    }
}

/// Build the ANSI-colored prefix string for a progress bar line.
///
/// During download the name is yellow; when complete it is green.
/// The `@version` suffix is always white.
// r[impl cli.progress-bar.package-name-downloading]
// r[impl cli.progress-bar.package-name-complete]
// r[impl cli.progress-bar.version-white]
// r[impl cli.progress-bar.no-indent]
fn build_prefix(glyph: &str, name: &str, version: Option<&str>, is_complete: bool) -> String {
    let styled_name = if is_complete {
        console::style(name).green().to_string()
    } else {
        console::style(name).yellow().to_string()
    };

    match version {
        Some(v) => format!(
            "{glyph} {}{}",
            styled_name,
            console::style(format!("@{v}")).white()
        ),
        None => format!("{glyph} {styled_name}"),
    }
}

/// Template for the initial state before layer sizes are known: just bytes
/// downloaded, no bar or total.  Once the first `LayerStarted` with a known
/// `total_bytes` arrives, `run_progress_bars` switches to [`PROGRESS_TEMPLATE`].
const INITIAL_TEMPLATE: &str = "{prefix} {binary_bytes:.dim}";

/// Template for in-progress downloads: yellow bar, dim size and ETA.
const PROGRESS_TEMPLATE: &str =
    "{prefix} {bar:12.yellow} {binary_bytes:.dim}/{binary_total_bytes:.dim} {eta:.dim}";

/// Template for completed downloads: no bar, just dim total size.
const DONE_TEMPLATE: &str = "{prefix} {binary_total_bytes:.dim}";

/// Style for the initial state before any total is known: bytes only.
fn initial_style() -> ProgressStyle {
    ProgressStyle::with_template(INITIAL_TEMPLATE).expect("valid progress bar template")
}

/// Style for in-progress downloads: yellow bar, dim size and ETA.
fn progress_style() -> ProgressStyle {
    ProgressStyle::with_template(PROGRESS_TEMPLATE)
        .expect("valid progress bar template")
        .progress_chars("━━┄")
}

/// Style for completed downloads: no bar, just dim total size.
fn done_style() -> ProgressStyle {
    ProgressStyle::with_template(DONE_TEMPLATE).expect("valid progress bar template")
}

/// Consume progress events and update a single aggregated progress bar.
///
/// All layer downloads are aggregated into a single progress bar for the
/// package. The total bytes is the sum of all layer sizes, and progress
/// is the sum of all per-layer bytes downloaded.
///
/// The bar starts with a bytes-only style ([`INITIAL_TEMPLATE`]).  Once the
/// first `LayerStarted` event with a known `total_bytes` arrives, the style
/// is upgraded to the full bar ([`PROGRESS_TEMPLATE`]) so that misleading
/// `0 B` totals are never shown.
// r[impl cli.progress-bar.aggregate-layers]
pub(crate) async fn run_progress_bars(
    pb: ProgressBar,
    mut rx: tokio::sync::mpsc::Receiver<ProgressEvent>,
) {
    let mut layer_progress: Vec<u64> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut style_upgraded = false;

    while let Some(event) = rx.recv().await {
        match event {
            ProgressEvent::ManifestFetched { layer_count, .. } => {
                layer_progress.resize(layer_count, 0);
            }
            ProgressEvent::LayerStarted {
                total_bytes: size, ..
            } => {
                if let Some(size) = size {
                    total_bytes += size;
                    pb.set_length(total_bytes);
                    // Switch from the initial bytes-only style to the full
                    // bar style now that we can display a meaningful total.
                    if !style_upgraded {
                        style_upgraded = true;
                        pb.set_style(progress_style());
                    }
                }
            }
            ProgressEvent::LayerProgress {
                index,
                bytes_downloaded,
            } => {
                if let Some(slot) = layer_progress.get_mut(index) {
                    *slot = bytes_downloaded;
                }
                let downloaded: u64 = layer_progress.iter().sum();
                pb.set_position(downloaded);
            }
            ProgressEvent::LayerDownloaded { .. }
            | ProgressEvent::LayerStored { .. }
            | ProgressEvent::InstallComplete => {
                // No action needed: the bar is finished by the caller
                // (ProgressTree::finish_bar) after this task completes.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify cli.progress-bar.tree-glyph]
    #[test]
    fn tree_glyph_mid_for_non_last() {
        assert_eq!(tree_glyph(false), "├──");
    }

    // r[verify cli.progress-bar.tree-glyph]
    #[test]
    fn tree_glyph_end_for_last() {
        assert_eq!(tree_glyph(true), "└──");
    }

    // r[verify cli.progress-bar.namespace-name]
    #[test]
    fn display_parts_wit_name_with_version() {
        let (name, version) = package_display_parts(Some("wasi:http@0.2.0"), Some("v0.2.0"));
        assert_eq!(name, "wasi:http");
        assert_eq!(version.as_deref(), Some("0.2.0"));
    }

    // r[verify cli.progress-bar.namespace-name]
    #[test]
    fn display_parts_wit_name_without_version() {
        let (name, version) = package_display_parts(Some("wasi:http"), Some("v0.2.10"));
        assert_eq!(name, "wasi:http");
        assert_eq!(version.as_deref(), Some("0.2.10"));
    }

    // r[verify cli.progress-bar.namespace-name]
    #[test]
    fn display_parts_wit_name_strips_v_prefix() {
        let (name, version) = package_display_parts(Some("wasi:http"), Some("v1.0.0"));
        assert_eq!(name, "wasi:http");
        assert_eq!(version.as_deref(), Some("1.0.0"));
    }

    // r[verify cli.progress-bar.namespace-name]
    #[test]
    fn display_parts_no_tag() {
        let (name, version) = package_display_parts(Some("wasi:http"), None);
        assert_eq!(name, "wasi:http");
        assert_eq!(version, None);
    }

    // r[verify cli.progress-bar.namespace-name]
    #[test]
    fn display_parts_tag_without_v_prefix() {
        let (name, version) = package_display_parts(Some("ba:sample"), Some("0.12.2"));
        assert_eq!(name, "ba:sample");
        assert_eq!(version.as_deref(), Some("0.12.2"));
    }

    // r[verify cli.progress-bar.namespace-name]
    #[test]
    fn oci_repo_three_segments() {
        assert_eq!(
            oci_repo_display_name("ghcr.io/webassembly/wasi-logging"),
            "webassembly:wasi-logging"
        );
    }

    // r[verify cli.progress-bar.namespace-name]
    #[test]
    fn oci_repo_two_segments() {
        assert_eq!(
            oci_repo_display_name("webassembly/wasi-logging"),
            "webassembly:wasi-logging"
        );
    }

    // r[verify cli.progress-bar.namespace-name]
    #[test]
    fn oci_repo_single_segment() {
        assert_eq!(oci_repo_display_name("wasi-logging"), "wasi-logging");
    }

    // r[verify cli.progress-bar.namespace-name]
    #[test]
    fn oci_repo_deep_path() {
        assert_eq!(
            oci_repo_display_name("ghcr.io/org/sub/package"),
            "sub:package"
        );
    }

    // r[verify cli.progress-bar.no-indent]
    #[test]
    fn prefix_not_indented() {
        let prefix = build_prefix("├──", "wasi:http", Some("0.2.0"), false);
        let plain = console::strip_ansi_codes(&prefix);
        // The prefix must start with the tree glyph, not spaces
        assert!(
            plain.starts_with("├──"),
            "prefix must start with tree glyph, got: {plain}"
        );
    }

    // r[verify cli.progress-bar.package-name-downloading]
    #[test]
    fn prefix_contains_name_while_downloading() {
        let prefix = build_prefix("├──", "wasi:http", Some("0.2.0"), false);
        let plain = console::strip_ansi_codes(&prefix);
        assert!(
            plain.contains("wasi:http"),
            "prefix must contain package name: {plain}"
        );
    }

    // r[verify cli.progress-bar.package-name-complete]
    #[test]
    fn prefix_contains_name_when_complete() {
        let prefix = build_prefix("├──", "wasi:http", Some("0.2.0"), true);
        let plain = console::strip_ansi_codes(&prefix);
        assert!(
            plain.contains("wasi:http"),
            "prefix must contain package name: {plain}"
        );
    }

    // r[verify cli.progress-bar.version-white]
    #[test]
    fn prefix_contains_version() {
        let prefix = build_prefix("├──", "wasi:http", Some("0.2.0"), false);
        let plain = console::strip_ansi_codes(&prefix);
        assert!(
            plain.contains("@0.2.0"),
            "prefix must contain @version: {plain}"
        );
    }

    // r[verify cli.progress-bar.version-white]
    #[test]
    fn prefix_no_version_when_none() {
        let prefix = build_prefix("├──", "wasi:http", None, false);
        let plain = console::strip_ansi_codes(&prefix);
        assert!(
            !plain.contains('@'),
            "prefix must not contain @ when no version: {plain}"
        );
    }

    // r[verify cli.progress-bar.tree-glyph]
    #[test]
    fn progress_tree_demotes_last_bar() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut tree = ProgressTree::new(multi);

        let (pb1, _id1) = tree.add_bar("wasi:http", Some("0.2.0"));
        // First bar should be └── (it's the last)
        assert!(
            console::strip_ansi_codes(&pb1.prefix()).starts_with(TREE_GLYPH_END),
            "first bar should start as └──"
        );

        let (pb2, _id2) = tree.add_bar("wasi:io", Some("0.2.0"));
        // pb1 should now be ├── (demoted)
        assert!(
            console::strip_ansi_codes(&pb1.prefix()).starts_with(TREE_GLYPH_MID),
            "first bar should be demoted to ├──"
        );
        // pb2 should be └── (new last)
        assert!(
            console::strip_ansi_codes(&pb2.prefix()).starts_with(TREE_GLYPH_END),
            "second bar should be └──"
        );
    }

    // r[verify cli.progress-bar.tree-glyph]
    #[test]
    fn progress_tree_finish_preserves_glyph() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut tree = ProgressTree::new(multi);

        let (pb1, id1) = tree.add_bar("wasi:http", Some("0.2.0"));
        let (pb2, id2) = tree.add_bar("wasi:io", Some("0.2.0"));

        // Finish pb1 — it should remain ├── since pb2 is last
        tree.finish_bar(&pb1, id1);
        assert!(
            console::strip_ansi_codes(&pb1.prefix()).starts_with(TREE_GLYPH_MID),
            "finished non-last bar should be ├──"
        );

        // Finish pb2 — it's the last, should stay └──
        tree.finish_bar(&pb2, id2);
        assert!(
            console::strip_ansi_codes(&pb2.prefix()).starts_with(TREE_GLYPH_END),
            "finished last bar should be └──"
        );
    }

    // r[verify cli.progress-bar.tree-glyph]
    #[test]
    fn progress_tree_new_bar_after_finished_demotes() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut tree = ProgressTree::new(multi);

        let (pb1, id1) = tree.add_bar("wasi:http", Some("0.2.0"));
        tree.finish_bar(&pb1, id1);

        // pb1 was └── and finished. Now add pb2 — pb1 should be demoted to ├──
        let (pb2, _id2) = tree.add_bar("wasi:io", Some("0.2.0"));

        assert!(
            console::strip_ansi_codes(&pb1.prefix()).starts_with(TREE_GLYPH_MID),
            "finished bar should be demoted to ├── when new bar is added"
        );
        assert!(
            console::strip_ansi_codes(&pb2.prefix()).starts_with(TREE_GLYPH_END),
            "new bar should be └──"
        );
    }

    // r[verify cli.progress-bar.aggregate-layers]
    #[tokio::test]
    async fn aggregate_layers_sums_progress() {
        use indicatif::ProgressDrawTarget;

        let pb = ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::hidden());

        let (tx, rx) = tokio::sync::mpsc::channel(64);

        let handle = tokio::spawn(run_progress_bars(pb.clone(), rx));

        // Simulate 2 layers
        tx.send(ProgressEvent::ManifestFetched {
            layer_count: 2,
            image_digest: "sha256:abc".into(),
        })
        .await
        .unwrap();

        tx.send(ProgressEvent::LayerStarted {
            index: 0,
            digest: "sha256:layer0".into(),
            total_bytes: Some(1000),
            title: None,
            media_type: "application/wasm".into(),
        })
        .await
        .unwrap();

        tx.send(ProgressEvent::LayerStarted {
            index: 1,
            digest: "sha256:layer1".into(),
            total_bytes: Some(500),
            title: None,
            media_type: "application/wasm".into(),
        })
        .await
        .unwrap();

        // Progress on both layers
        tx.send(ProgressEvent::LayerProgress {
            index: 0,
            bytes_downloaded: 600,
        })
        .await
        .unwrap();

        tx.send(ProgressEvent::LayerProgress {
            index: 1,
            bytes_downloaded: 300,
        })
        .await
        .unwrap();

        // Allow processing
        tokio::task::yield_now().await;

        // Verify aggregate state
        assert_eq!(
            pb.length(),
            Some(1500),
            "total should be sum of layer sizes"
        );
        assert_eq!(
            pb.position(),
            900,
            "position should be sum of layer progress"
        );

        tx.send(ProgressEvent::InstallComplete).await.unwrap();
        drop(tx);
        let _ = handle.await;
    }

    // r[verify cli.progress-bar.aggregate-layers]
    #[tokio::test]
    async fn aggregate_layers_handles_unknown_sizes() {
        use indicatif::ProgressDrawTarget;

        let pb = ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::hidden());

        let (tx, rx) = tokio::sync::mpsc::channel(64);

        let handle = tokio::spawn(run_progress_bars(pb.clone(), rx));

        tx.send(ProgressEvent::ManifestFetched {
            layer_count: 1,
            image_digest: "sha256:abc".into(),
        })
        .await
        .unwrap();

        // Layer with unknown size
        tx.send(ProgressEvent::LayerStarted {
            index: 0,
            digest: "sha256:layer0".into(),
            total_bytes: None,
            title: None,
            media_type: "application/wasm".into(),
        })
        .await
        .unwrap();

        tx.send(ProgressEvent::LayerProgress {
            index: 0,
            bytes_downloaded: 500,
        })
        .await
        .unwrap();

        tokio::task::yield_now().await;

        // Total stays at initial 0 since we never got a total_bytes
        assert_eq!(pb.length(), Some(0));
        assert_eq!(pb.position(), 500);

        drop(tx);
        let _ = handle.await;
    }

    // r[verify cli.progress-bar.bar-hidden-on-complete]
    #[test]
    fn done_style_template_has_no_bar() {
        assert!(
            !DONE_TEMPLATE.contains("{bar"),
            "done style must not contain a bar: {DONE_TEMPLATE}"
        );
    }

    // r[verify cli.progress-bar.bar-yellow]
    #[test]
    fn progress_style_template_uses_yellow() {
        assert!(
            PROGRESS_TEMPLATE.contains(".yellow"),
            "progress style must use yellow for the bar: {PROGRESS_TEMPLATE}"
        );
    }

    // r[verify cli.progress-bar.size-grey]
    #[test]
    fn progress_style_template_uses_dim_for_size() {
        assert!(
            PROGRESS_TEMPLATE.contains("binary_bytes:.dim"),
            "progress style must use dim for size: {PROGRESS_TEMPLATE}"
        );
    }

    // r[verify cli.progress-bar.eta-grey]
    #[test]
    fn progress_style_template_uses_dim_for_eta() {
        assert!(
            PROGRESS_TEMPLATE.contains("eta:.dim"),
            "progress style must use dim for ETA: {PROGRESS_TEMPLATE}"
        );
    }

    // Verify all style factory functions produce usable ProgressStyle instances.
    // This guards against template syntax errors and ensures the constants used
    // by the tests above are the same ones that reach production code.

    // r[verify cli.progress-bar.bar-yellow]
    #[test]
    fn progress_style_produces_valid_style() {
        let style = progress_style();
        let pb = ProgressBar::hidden();
        pb.set_style(style);
        pb.set_length(100);
        pb.set_position(50);
        // No panic ⇒ template is valid and can be applied.
    }

    // r[verify cli.progress-bar.bar-hidden-on-complete]
    #[test]
    fn done_style_produces_valid_style() {
        let style = done_style();
        let pb = ProgressBar::hidden();
        pb.set_style(style);
        pb.set_length(100);
        pb.finish();
    }

    #[test]
    fn initial_style_produces_valid_style() {
        let style = initial_style();
        let pb = ProgressBar::hidden();
        pb.set_style(style);
        pb.set_position(500);
    }

    // Verify the initial template has no bar/total/eta (bytes-only).
    #[test]
    fn initial_template_shows_only_bytes() {
        assert!(
            !INITIAL_TEMPLATE.contains("{bar"),
            "initial style must not contain a bar: {INITIAL_TEMPLATE}"
        );
        assert!(
            !INITIAL_TEMPLATE.contains("binary_total_bytes"),
            "initial style must not show total bytes: {INITIAL_TEMPLATE}"
        );
        assert!(
            !INITIAL_TEMPLATE.contains("eta"),
            "initial style must not show ETA: {INITIAL_TEMPLATE}"
        );
        assert!(
            INITIAL_TEMPLATE.contains("binary_bytes"),
            "initial style must show downloaded bytes: {INITIAL_TEMPLATE}"
        );
    }

    // r[verify cli.progress-bar.aggregate-layers]
    #[tokio::test]
    async fn aggregate_layers_switches_to_bar_on_known_total() {
        use indicatif::ProgressDrawTarget;

        let pb = ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::hidden());
        pb.set_style(initial_style());

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let handle = tokio::spawn(run_progress_bars(pb.clone(), rx));

        tx.send(ProgressEvent::ManifestFetched {
            layer_count: 1,
            image_digest: "sha256:abc".into(),
        })
        .await
        .unwrap();

        // Before known total: bar should still have initial length of 0
        assert_eq!(pb.length(), Some(0));

        // Send layer with known total — triggers style switch
        tx.send(ProgressEvent::LayerStarted {
            index: 0,
            digest: "sha256:layer0".into(),
            total_bytes: Some(2000),
            title: None,
            media_type: "application/wasm".into(),
        })
        .await
        .unwrap();

        tx.send(ProgressEvent::LayerProgress {
            index: 0,
            bytes_downloaded: 1000,
        })
        .await
        .unwrap();

        tokio::task::yield_now().await;

        // After known total: length should reflect the total
        assert_eq!(pb.length(), Some(2000));
        assert_eq!(pb.position(), 1000);

        drop(tx);
        let _ = handle.await;
    }
}
