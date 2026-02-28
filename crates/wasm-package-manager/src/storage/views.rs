use oci_client::manifest::OciImageManifest;

use super::models::ImageEntry;
use super::models::KnownPackage;
use super::models::WitInterface;

/// A public view of an OCI image entry, without internal database IDs.
///
/// This type is freely constructable and is the primary public API type
/// for representing stored OCI images. Internal code uses [`ImageEntry`]
/// with database IDs; this view type strips those away.
#[derive(Debug, Clone)]
pub struct ImageView {
    /// Registry hostname
    pub ref_registry: String,
    /// Repository path
    pub ref_repository: String,
    /// Optional mirror registry hostname
    pub ref_mirror_registry: Option<String>,
    /// Optional tag
    pub ref_tag: Option<String>,
    /// Optional digest
    pub ref_digest: Option<String>,
    /// OCI image manifest
    pub manifest: OciImageManifest,
    /// Size of the image on disk in bytes
    pub size_on_disk: u64,
}

impl ImageView {
    /// Returns the full reference string for this image (e.g., "ghcr.io/user/repo:tag").
    #[must_use]
    pub fn reference(&self) -> String {
        let mut reference = format!("{}/{}", self.ref_registry, self.ref_repository);
        if let Some(tag) = &self.ref_tag {
            reference.push(':');
            reference.push_str(tag);
        } else if let Some(digest) = &self.ref_digest {
            reference.push('@');
            reference.push_str(digest);
        }
        reference
    }
}

impl From<ImageEntry> for ImageView {
    fn from(entry: ImageEntry) -> Self {
        Self {
            ref_registry: entry.ref_registry,
            ref_repository: entry.ref_repository,
            ref_mirror_registry: entry.ref_mirror_registry,
            ref_tag: entry.ref_tag,
            ref_digest: entry.ref_digest,
            manifest: entry.manifest,
            size_on_disk: entry.size_on_disk,
        }
    }
}

/// A public view of a known package, without internal database IDs.
///
/// This type is freely constructable and is the primary public API type
/// for representing known packages. Internal code uses [`KnownPackage`]
/// with database IDs; this view type strips those away.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnownPackageView {
    /// Registry hostname
    pub registry: String,
    /// Repository path
    pub repository: String,
    /// Optional package description
    pub description: Option<String>,
    /// Release tags
    pub tags: Vec<String>,
    /// Signature tags (kept for API compatibility, always empty)
    #[serde(default)]
    pub signature_tags: Vec<String>,
    /// Attestation tags (kept for API compatibility, always empty)
    #[serde(default)]
    pub attestation_tags: Vec<String>,
    /// Timestamp of last seen
    pub last_seen_at: String,
    /// Timestamp of creation
    pub created_at: String,
}

impl KnownPackageView {
    /// Returns the full reference string for this package (e.g., "ghcr.io/user/repo").
    #[must_use]
    pub fn reference(&self) -> String {
        format!("{}/{}", self.registry, self.repository)
    }

    /// Returns the full reference string with the most recent tag.
    #[must_use]
    pub fn reference_with_tag(&self) -> String {
        if let Some(tag) = self.tags.first() {
            format!("{}:{}", self.reference(), tag)
        } else {
            format!("{}:latest", self.reference())
        }
    }
}

impl From<KnownPackage> for KnownPackageView {
    fn from(pkg: KnownPackage) -> Self {
        Self {
            registry: pkg.registry,
            repository: pkg.repository,
            description: pkg.description,
            tags: pkg.tags,
            signature_tags: pkg.signature_tags,
            attestation_tags: pkg.attestation_tags,
            last_seen_at: pkg.last_seen_at,
            created_at: pkg.created_at,
        }
    }
}

/// A public view of a WIT interface, without internal database IDs.
///
/// This type is freely constructable and is the primary public API type
/// for representing WIT interfaces. Internal code uses [`WitInterface`]
/// with database IDs; this view type strips those away.
#[derive(Debug, Clone)]
pub struct WitInterfaceView {
    /// The WIT package name (e.g. "wasi:http").
    pub package_name: String,
    /// Semver version string, if known.
    pub version: Option<String>,
    /// Human-readable description of the interface.
    pub description: Option<String>,
    /// Full WIT text representation, when available.
    pub wit_text: Option<String>,
    /// When this row was created.
    pub created_at: String,
}

impl From<WitInterface> for WitInterfaceView {
    fn from(iface: WitInterface) -> Self {
        Self {
            package_name: iface.package_name,
            version: iface.version,
            description: iface.description,
            wit_text: iface.wit_text,
            created_at: iface.created_at,
        }
    }
}
