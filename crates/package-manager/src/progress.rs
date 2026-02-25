/// Events emitted during package pull/install operations to report progress.
///
/// These events are sent via a `tokio::sync::mpsc::Sender<ProgressEvent>` channel
/// and can be consumed by CLI progress bars or TUI progress displays.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// Manifest has been fetched from the registry.
    ManifestFetched {
        /// The number of layers in the manifest.
        layer_count: usize,
        /// The content digest of the image (e.g., "sha256:abc123...").
        image_digest: String,
    },
    /// A layer download has started.
    LayerStarted {
        /// Zero-based index of this layer.
        index: usize,
        /// The content digest of the layer (e.g., "sha256:abc123...").
        digest: String,
        /// The expected total bytes, if known from the content-length header.
        total_bytes: Option<u64>,
        /// The `org.opencontainers.image.title` annotation, if present.
        title: Option<String>,
        /// The media type of the layer (e.g., "application/wasm").
        media_type: String,
    },
    /// Incremental byte progress for a layer download.
    LayerProgress {
        /// Zero-based index of this layer.
        index: usize,
        /// Cumulative bytes downloaded so far.
        bytes_downloaded: u64,
    },
    /// A layer has been fully downloaded.
    LayerDownloaded {
        /// Zero-based index of this layer.
        index: usize,
    },
    /// A layer has been written to the content-addressable store.
    LayerStored {
        /// Zero-based index of this layer.
        index: usize,
    },
    /// All layers have been installed successfully.
    InstallComplete,
}
