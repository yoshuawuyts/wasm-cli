//! Progress bar rendering for the `wasm install` command.
//!
//! This module handles the display of download progress for packages being
//! installed. Each package gets a single aggregated progress bar that combines
//! all layer downloads. Packages are displayed as a flat, column-aligned list
//! with per-package progress bars and checkmark completion markers.
//!
//! The [`InstallDisplay`] type manages the phased display: syncing, planning,
//! installing, and done.

use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use wasm_package_manager::ProgressEvent;

/// Manages the phased install display.
///
/// Supports four phases: syncing the registry, planning the install,
/// installing packages concurrently, and a final completion summary.
/// Package rows are flat and column-aligned without tree glyphs.
pub(crate) struct InstallDisplay {
    multi: MultiProgress,
    /// All package rows, in insertion order.
    entries: Vec<RowEntry>,
    /// Monotonically increasing counter for unique row IDs.
    next_id: usize,
    /// Phase spinner shown below the package list.
    phase_spinner: Option<ProgressBar>,
    /// Padding width for column-aligned package names.
    name_width: usize,
}

/// Opaque handle returned by [`InstallDisplay::add_bar`] to identify a specific
/// row entry when finishing it.  Using an ID instead of the
/// package name avoids misidentification when two installs share the same
/// display name and version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BarId(usize);

/// Metadata kept for each row.
struct RowEntry {
    id: BarId,
    bar: ProgressBar,
    name: String,
    version: Option<String>,
    is_complete: bool,
}

impl InstallDisplay {
    /// Create a new install display backed by the given [`MultiProgress`].
    pub(crate) fn new(multi: MultiProgress) -> Self {
        Self {
            multi,
            entries: Vec::new(),
            next_id: 0,
            phase_spinner: None,
            name_width: 0,
        }
    }

    /// Start the syncing phase spinner.
    // r[impl cli.progress-bar.phase-syncing]
    // r[impl cli.progress-bar.spinner-interval]
    pub(crate) fn start_sync(&mut self) {
        self.set_phase_spinner("Syncing registry");
    }

    /// Start the planning phase spinner.
    // r[impl cli.progress-bar.phase-planning]
    pub(crate) fn start_planning(&mut self) {
        self.set_phase_spinner("Planning");
    }

    /// Display the resolved plan as a flat list of packages.
    ///
    /// Each entry is rendered as `name version` with column-aligned padding.
    /// This pre-creates the progress bar rows so that `add_bar` can later
    /// attach progress tracking to each row by name.
    // r[impl cli.progress-bar.flat-rows]
    // r[impl cli.progress-bar.version-display]
    pub(crate) fn show_plan(&mut self, packages: &[(&str, Option<&str>)]) {
        self.clear_phase_spinner();

        // Compute column width from the longest `name version` string.
        self.name_width = packages
            .iter()
            .map(|(name, version)| match version {
                Some(v) => format!("{name} {v}").len(),
                None => name.len(),
            })
            .max()
            .unwrap_or(0);

        for &(name, version) in packages {
            self.create_row(name, version);
        }
    }

    /// Start the installing phase spinner.
    // r[impl cli.progress-bar.phase-installing]
    pub(crate) fn start_installing(&mut self) {
        self.set_phase_spinner("Installing");
    }

    /// Look up an existing row by display name and version, and return its
    /// progress bar handle and [`BarId`].  If no pre-created row exists
    /// (e.g. a late fallback dependency), a new row is appended.
    // r[impl cli.progress-bar.bar-yellow]
    // r[impl cli.progress-bar.size-grey]
    // r[impl cli.progress-bar.eta-grey]
    pub(crate) fn add_bar(&mut self, name: &str, version: Option<&str>) -> (ProgressBar, BarId) {
        // Try to find an existing planned row for this package, matching on
        // both name and version to avoid misidentification when the same
        // display name appears with different versions.
        if let Some(entry) = self
            .entries
            .iter()
            .find(|e| e.name == name && e.version.as_deref() == version)
        {
            let pb = entry.bar.clone();
            // Reset the bar from its "finished" plan state so that position
            // and length updates from run_progress_bars are rendered.
            pb.reset();
            pb.set_style(initial_style());
            return (pb, entry.id);
        }

        // Fallback: append a new row for a late-discovered dependency.
        let label_len = match version {
            Some(v) => format!("{name} {v}").len(),
            None => name.len(),
        };
        if label_len > self.name_width {
            self.name_width = label_len;
            self.realign_prefixes();
        }
        self.create_row(name, version)
    }

    /// Mark a row as complete with a green `✓` marker.
    ///
    /// The row is looked up exclusively via `bar_id`; the caller does not
    /// need to pass the [`ProgressBar`] handle.  If all layer totals were
    /// `None` (so the bar length is still 0), the length is set to the
    /// current position so that [`DONE_TEMPLATE`] renders the actual
    /// downloaded byte count instead of `0 B`.
    // r[impl cli.progress-bar.checkmark-complete]
    pub(crate) fn finish_bar(&mut self, bar_id: BarId) {
        let Some(entry) = self.entries.iter_mut().find(|e| e.id == bar_id) else {
            tracing::debug!("finish_bar called with unknown BarId({bar_id:?})");
            return;
        };

        if entry.bar.length() == Some(0) {
            entry.bar.set_length(entry.bar.position());
        }

        let prefix = build_prefix(&entry.name, entry.version.as_deref(), self.name_width, true);
        entry.bar.set_style(done_style());
        entry.bar.set_prefix(prefix);
        entry
            .bar
            .finish_with_message(console::style("✓").green().to_string());
        entry.is_complete = true;
    }

    /// Returns the number of successfully completed package rows.
    pub(crate) fn completed_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_complete).count()
    }

    /// Display the final completion summary.
    // r[impl cli.progress-bar.phase-done]
    pub(crate) fn finish_all(&mut self, count: usize, elapsed: Duration) {
        self.clear_phase_spinner();

        let msg = format!(
            "{} Installed {} packages in {:.1}s",
            console::style("✓").green().bold(),
            count,
            elapsed.as_secs_f64()
        );
        let pb = self.multi.add(ProgressBar::new(0));
        pb.set_style(ProgressStyle::with_template("{msg}").expect("valid template"));
        pb.finish_with_message(msg);
    }

    /// Create a row entry and add it to the display.
    fn create_row(&mut self, name: &str, version: Option<&str>) -> (ProgressBar, BarId) {
        let prefix = build_prefix(name, version, self.name_width, false);

        // Insert the new row *before* the phase spinner so it stays at
        // the bottom of the output.
        let pb = if let Some(ref spinner) = self.phase_spinner {
            self.multi.insert_before(spinner, ProgressBar::new(0))
        } else {
            self.multi.add(ProgressBar::new(0))
        };

        pb.set_style(plan_style());
        pb.set_prefix(prefix);
        pb.finish(); // show immediately as a static line

        let id = BarId(self.next_id);
        self.next_id += 1;

        self.entries.push(RowEntry {
            id,
            bar: pb.clone(),
            name: name.to_string(),
            version: version.map(String::from),
            is_complete: false,
        });

        (pb, id)
    }

    /// Set (or replace) the phase spinner with the given label.
    fn set_phase_spinner(&mut self, label: &str) {
        self.clear_phase_spinner();

        let spinner = self.multi.add(ProgressBar::new_spinner());
        let style = ProgressStyle::with_template(&format!("{{spinner}} {label}"))
            .expect("valid spinner template")
            .tick_strings(&spinner_ticks());
        spinner.set_style(style);
        spinner.enable_steady_tick(Duration::from_millis(80));
        self.phase_spinner = Some(spinner);
    }

    /// Remove the current phase spinner, if any.
    fn clear_phase_spinner(&mut self) {
        if let Some(spinner) = self.phase_spinner.take() {
            spinner.finish_and_clear();
        }
    }

    /// Update all row prefixes to the current `name_width`.
    ///
    /// Called when a fallback row causes `name_width` to grow so that
    /// previously created rows stay column-aligned.
    fn realign_prefixes(&mut self) {
        for entry in &mut self.entries {
            let prefix = build_prefix(
                &entry.name,
                entry.version.as_deref(),
                self.name_width,
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
/// in the output.  When fewer than two segments are available, the full
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

/// Build the ANSI-colored prefix string for a row.
///
/// The prefix is `name version` padded to `name_width` for column alignment.
/// When complete, the name is shown in green; otherwise white.
/// The version is space-separated (not `@`-separated).
// r[impl cli.progress-bar.version-display]
// r[impl cli.progress-bar.flat-rows]
// r[impl cli.progress-bar.name-color-downloading]
// r[impl cli.progress-bar.name-color-complete]
fn build_prefix(name: &str, version: Option<&str>, name_width: usize, is_complete: bool) -> String {
    let label = match version {
        Some(v) => format!("{name} {v}"),
        None => name.to_string(),
    };

    let padded = format!("{label:<name_width$}");

    if is_complete {
        console::style(padded).green().to_string()
    } else {
        padded
    }
}

/// Build the Braille spinner tick strings for [`ProgressStyle::tick_strings`].
///
/// The last element is the "done" tick shown when the spinner finishes.
/// We use a space since the phase spinner is always cleared via
/// `finish_and_clear()` — the checkmark is reserved for completed
/// package rows and the final summary line.
// r[impl cli.progress-bar.spinner-chars]
fn spinner_ticks() -> Vec<&'static str> {
    vec![
        "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", // braille frames
        " ", // done tick (cleared immediately, never visible)
    ]
}

/// Template for the initial state before layer sizes are known: just bytes
/// downloaded, no bar or total.  Once the first `LayerStarted` with a known
/// `total_bytes` arrives, `run_progress_bars` switches to [`PROGRESS_TEMPLATE`].
const INITIAL_TEMPLATE: &str = "{prefix} {binary_bytes:.dim}";

/// Template for in-progress downloads: yellow bar, dim size and ETA.
const PROGRESS_TEMPLATE: &str =
    "{prefix} {bar:12.yellow} {binary_bytes:.dim}/{binary_total_bytes:.dim} {eta:.dim}";

/// Template for completed downloads: checkmark only.
const DONE_TEMPLATE: &str = "{prefix} {msg}";

/// Template for plan rows before downloading starts: just the prefix.
const PLAN_TEMPLATE: &str = "{prefix}";

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

/// Style for completed downloads: green checkmark.
fn done_style() -> ProgressStyle {
    ProgressStyle::with_template(DONE_TEMPLATE).expect("valid progress bar template")
}

/// Style for planned rows before downloading starts.
fn plan_style() -> ProgressStyle {
    ProgressStyle::with_template(PLAN_TEMPLATE).expect("valid progress bar template")
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
                // (InstallDisplay::finish_bar) after this task completes.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // r[verify cli.progress-bar.version-display]
    #[test]
    fn prefix_uses_space_separator_for_version() {
        let prefix = build_prefix("wasi:http", Some("0.2.0"), 20, false);
        let plain = console::strip_ansi_codes(&prefix);
        assert!(
            plain.contains("wasi:http 0.2.0"),
            "prefix must use space separator: {plain}"
        );
        assert!(
            !plain.contains('@'),
            "prefix must not contain @ separator: {plain}"
        );
    }

    // r[verify cli.progress-bar.flat-rows]
    #[test]
    fn prefix_has_no_tree_glyphs() {
        let prefix = build_prefix("wasi:http", Some("0.2.0"), 20, false);
        let plain = console::strip_ansi_codes(&prefix);
        assert!(
            !plain.contains("├──"),
            "prefix must not contain tree glyphs: {plain}"
        );
        assert!(
            !plain.contains("└──"),
            "prefix must not contain tree glyphs: {plain}"
        );
    }

    // r[verify cli.progress-bar.flat-rows]
    #[test]
    fn prefix_is_column_aligned() {
        let p1 = build_prefix("wasi:http", Some("0.2.0"), 20, false);
        let p2 = build_prefix("wasi:io", Some("0.2.3"), 20, false);
        let plain1 = console::strip_ansi_codes(&p1);
        let plain2 = console::strip_ansi_codes(&p2);
        assert_eq!(
            plain1.len(),
            plain2.len(),
            "prefixes must be padded to equal width: '{plain1}' vs '{plain2}'"
        );
    }

    // r[verify cli.progress-bar.version-display]
    #[test]
    fn prefix_no_version_when_none() {
        let prefix = build_prefix("wasi:http", None, 20, false);
        let plain = console::strip_ansi_codes(&prefix);
        assert!(
            plain.starts_with("wasi:http"),
            "prefix must start with package name: {plain}"
        );
    }

    // r[verify cli.progress-bar.checkmark-complete]
    #[test]
    fn prefix_green_when_complete() {
        let prefix = build_prefix("wasi:http", Some("0.2.0"), 20, true);
        let plain = console::strip_ansi_codes(&prefix);
        assert!(
            plain.contains("wasi:http 0.2.0"),
            "completed prefix must contain package name: {plain}"
        );
    }

    // r[verify cli.progress-bar.spinner-chars]
    #[test]
    fn spinner_tick_chars_are_braille() {
        let ticks = spinner_ticks();
        // 10 braille chars + 1 done tick (space)
        assert_eq!(ticks.len(), 11);
        assert_eq!(ticks[0], "⠋");
        assert_eq!(ticks[9], "⠏");
        assert_eq!(
            ticks[10], " ",
            "done tick should be a space, not a checkmark"
        );
    }

    // r[verify cli.progress-bar.spinner-interval]
    #[test]
    fn spinner_interval_is_80ms() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.start_sync();
        // The spinner is created via `set_phase_spinner` which calls
        // `enable_steady_tick(Duration::from_millis(80))`.
        // Verify the spinner is active (steady tick was set).
        let spinner = display
            .phase_spinner
            .as_ref()
            .expect("spinner should exist");
        assert!(
            !spinner.is_finished(),
            "spinner should be ticking (not finished)"
        );
    }

    // r[verify cli.progress-bar.flat-rows]
    #[test]
    fn show_plan_creates_aligned_rows() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.show_plan(&[
            ("wasi:http", Some("0.2.3")),
            ("wasi:io", Some("0.2.3")),
            ("ba:sample-wasi-http-rust", Some("0.1.6")),
        ]);

        // All rows should have the same name_width
        assert_eq!(display.entries.len(), 3);

        // name_width should match the longest label
        let expected_width = "ba:sample-wasi-http-rust 0.1.6".len();
        assert_eq!(display.name_width, expected_width);
    }

    // r[verify cli.progress-bar.checkmark-complete]
    #[test]
    fn finish_bar_shows_checkmark() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.show_plan(&[("wasi:http", Some("0.2.0"))]);
        let (_, bar_id) = display.add_bar("wasi:http", Some("0.2.0"));
        display.finish_bar(bar_id);

        let entry = &display.entries[0];
        assert!(entry.is_complete);
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

    // r[verify cli.progress-bar.checkmark-complete]
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

    // r[verify cli.progress-bar.bar-yellow]
    #[test]
    fn progress_style_produces_valid_style() {
        let style = progress_style();
        let pb = ProgressBar::hidden();
        pb.set_style(style);
        pb.set_length(100);
        pb.set_position(50);
    }

    // r[verify cli.progress-bar.checkmark-complete]
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

    #[test]
    fn plan_style_produces_valid_style() {
        let style = plan_style();
        let pb = ProgressBar::hidden();
        pb.set_style(style);
        pb.finish();
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

    // r[verify cli.progress-bar.flat-rows]
    #[test]
    fn add_bar_reuses_planned_row() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.show_plan(&[("wasi:http", Some("0.2.0")), ("wasi:io", Some("0.2.3"))]);
        assert_eq!(display.entries.len(), 2);

        // add_bar for a planned name should reuse the existing row
        let (_, id) = display.add_bar("wasi:http", Some("0.2.0"));
        assert_eq!(display.entries.len(), 2, "should not add a new row");
        assert_eq!(id, BarId(0));
    }

    // r[verify cli.progress-bar.flat-rows]
    #[test]
    fn add_bar_appends_fallback_row() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.show_plan(&[("wasi:http", Some("0.2.0"))]);
        assert_eq!(display.entries.len(), 1);

        // add_bar for an unknown name should append a new row
        let (_, id) = display.add_bar("wasi:io", Some("0.2.3"));
        assert_eq!(display.entries.len(), 2, "should append a fallback row");
        assert_eq!(id, BarId(1));
    }

    // r[verify cli.progress-bar.flat-rows]
    #[test]
    fn show_plan_empty_list() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.show_plan(&[]);
        assert_eq!(display.entries.len(), 0);
        assert_eq!(display.name_width, 0);
    }

    // r[verify cli.progress-bar.phase-done]
    #[test]
    fn finish_all_creates_completion_message() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.show_plan(&[("wasi:http", Some("0.2.0"))]);
        display.finish_all(1, Duration::from_secs_f64(1.2));
        // Should not panic; verifies message construction is valid.
    }

    // r[verify cli.progress-bar.phase-syncing]
    // r[verify cli.progress-bar.phase-planning]
    // r[verify cli.progress-bar.phase-installing]
    #[test]
    fn phase_spinners_replace_each_other() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.start_sync();
        assert!(
            display.phase_spinner.is_some(),
            "sync should create spinner"
        );

        display.start_planning();
        assert!(
            display.phase_spinner.is_some(),
            "planning should replace spinner"
        );

        display.start_installing();
        assert!(
            display.phase_spinner.is_some(),
            "installing should replace spinner"
        );
    }

    // r[verify cli.progress-bar.bar-yellow]
    #[test]
    fn add_bar_resets_finished_state_for_progress() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.show_plan(&[("wasi:http", Some("0.2.0"))]);

        // After show_plan, the bar is in a "finished" state (plan_style).
        // add_bar must reset it so progress updates are rendered.
        let (pb, _id) = display.add_bar("wasi:http", Some("0.2.0"));

        // The bar should accept position/length updates after reset.
        pb.set_length(1000);
        pb.set_position(500);
        assert_eq!(pb.length(), Some(1000));
        assert_eq!(pb.position(), 500);
    }

    // r[verify cli.progress-bar.flat-rows]
    #[test]
    fn add_bar_same_name_different_version_creates_new_row() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.show_plan(&[("wasi:http", Some("0.2.0"))]);
        assert_eq!(display.entries.len(), 1);

        // Same name but different version should create a new row
        let (_, id) = display.add_bar("wasi:http", Some("0.3.0"));
        assert_eq!(display.entries.len(), 2);
        assert_eq!(id, BarId(1));
    }

    // r[verify cli.progress-bar.flat-rows]
    #[test]
    fn fallback_row_realigns_existing_prefixes() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        display.show_plan(&[("wasi:http", Some("0.2.0"))]);
        let old_width = display.name_width;

        // Add a fallback row with a much longer name
        display.add_bar("ba:very-long-package-name", Some("1.0.0"));

        // name_width should have increased
        assert!(display.name_width > old_width);

        // All entries should be realigned — the name_width is consistent
        // across the display after the fallback row was added.
        assert_eq!(display.name_width, "ba:very-long-package-name 1.0.0".len());
    }

    // r[verify cli.progress-bar.name-color-downloading]
    #[test]
    fn prefix_unstyled_during_download() {
        let prefix = build_prefix("wasi:http", Some("0.2.3"), 20, false);
        // When not complete the prefix should be plain text (no ANSI escapes).
        assert_eq!(
            prefix,
            console::strip_ansi_codes(&prefix),
            "downloading prefix must be unstyled"
        );
    }

    // r[verify cli.progress-bar.name-color-complete]
    #[test]
    fn prefix_green_on_completion() {
        // Force colors on so the test is deterministic regardless of TTY.
        console::set_colors_enabled(true);
        let prefix = build_prefix("wasi:http", Some("0.2.3"), 20, true);
        // When complete the prefix contains ANSI green escape codes.
        assert_ne!(
            prefix,
            console::strip_ansi_codes(&prefix),
            "completed prefix must contain ANSI styling"
        );
    }

    // r[verify cli.progress-bar.plan-timing]
    #[test]
    fn plan_displayed_before_installing_phase() {
        use indicatif::ProgressDrawTarget;

        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let mut display = InstallDisplay::new(multi);

        // Simulate the phase sequence: planning → show_plan → installing.
        display.start_planning();
        display.show_plan(&[("wasi:io", Some("0.2.3"))]);
        // Rows exist before the installing phase begins.
        assert_eq!(display.completed_count(), 0);
        assert_eq!(
            display.entries.len(),
            1,
            "plan must be visible before installing starts"
        );
        display.start_installing();
        // Rows remain after starting the install phase.
        assert_eq!(display.entries.len(), 1);
    }
}
