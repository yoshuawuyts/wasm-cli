//! Shared wire types for the `wasm-meta-registry` API.
//!
//! This crate contains the data types serialized as JSON between the
//! meta-registry server and its clients.  It has no HTTP, database, or
//! runtime dependencies — only `serde`.

// ============================================================
// Existing types (moved from wasm-meta-registry-client)
// ============================================================

/// Whether a package is a runnable Wasm component or a WIT interface
/// definition.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::PackageKind;
///
/// let kind: PackageKind = serde_json::from_str(r#""component""#).unwrap();
/// assert_eq!(kind, PackageKind::Component);
///
/// let json = serde_json::to_string(&PackageKind::Interface).unwrap();
/// assert_eq!(json, r#""interface""#);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageKind {
    /// A runnable Wasm component.
    Component,
    /// A WIT interface type package.
    Interface,
}

impl std::fmt::Display for PackageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Component => write!(f, "component"),
            Self::Interface => write!(f, "interface"),
        }
    }
}

/// A declared dependency on another WIT package, as returned in the
/// `/v1/packages` response.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::PackageDependencyRef;
///
/// let dep = PackageDependencyRef {
///     package: "wasi:io".into(),
///     version: Some("0.2.0".into()),
/// };
/// assert_eq!(dep.package, "wasi:io");
/// assert_eq!(dep.version.as_deref(), Some("0.2.0"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageDependencyRef {
    /// Declared package name (e.g. `"wasi:io"`).
    pub package: String,
    /// Declared version, if any (e.g. `"0.2.0"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// A public view of a known package from a meta-registry.
///
/// This type matches the JSON schema returned by the `/v1/packages` endpoint
/// and is the primary wire type shared between the meta-registry server and
/// its clients.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::{KnownPackage, PackageKind};
///
/// let pkg = KnownPackage {
///     registry: "ghcr.io".into(),
///     repository: "user/my-component".into(),
///     kind: Some(PackageKind::Component),
///     description: Some("A useful component".into()),
///     tags: vec!["v1.0.0".into(), "latest".into()],
///     signature_tags: vec![],
///     attestation_tags: vec![],
///     last_seen_at: "2025-01-01T00:00:00Z".into(),
///     created_at: "2024-06-15T12:00:00Z".into(),
///     wit_namespace: None,
///     wit_name: None,
///     dependencies: vec![],
/// };
///
/// assert_eq!(pkg.reference(), "ghcr.io/user/my-component");
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnownPackage {
    /// Registry hostname (e.g. `"ghcr.io"`).
    pub registry: String,
    /// Repository path (e.g. `"user/repo"`).
    pub repository: String,
    /// Whether this package is a component or an interface.
    ///
    /// `None` when the kind has not been determined yet (e.g. the
    /// database was created before the `kind` column existed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PackageKind>,
    /// Optional package description.
    pub description: Option<String>,
    /// Release tags.
    pub tags: Vec<String>,
    /// Signature tags (kept for API compatibility, always empty).
    #[serde(default)]
    pub signature_tags: Vec<String>,
    /// Attestation tags (kept for API compatibility, always empty).
    #[serde(default)]
    pub attestation_tags: Vec<String>,
    /// Timestamp of last seen.
    pub last_seen_at: String,
    /// Timestamp of creation.
    pub created_at: String,
    /// Optional WIT namespace (e.g. `"ba"`, `"wasi"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wit_namespace: Option<String>,
    /// Optional WIT package name within the namespace (e.g. `"http"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wit_name: Option<String>,
    /// Declared WIT dependencies of this package's latest indexed version.
    ///
    /// The field MAY be omitted when no WIT metadata has been extracted for
    /// this package; omission MUST be treated as equivalent to an empty list.
    // r[impl client.known-package.dependencies]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<PackageDependencyRef>,
}

impl KnownPackage {
    /// Returns the full reference string for this package (e.g., `"ghcr.io/user/repo"`).
    ///
    /// # Example
    ///
    /// ```rust
    /// use wasm_meta_registry_types::KnownPackage;
    ///
    /// let pkg = KnownPackage {
    ///     registry: "ghcr.io".into(),
    ///     repository: "user/repo".into(),
    ///     kind: None,
    ///     description: None,
    ///     tags: vec![],
    ///     signature_tags: vec![],
    ///     attestation_tags: vec![],
    ///     last_seen_at: String::new(),
    ///     created_at: String::new(),
    ///     wit_namespace: None,
    ///     wit_name: None,
    ///     dependencies: vec![],
    /// };
    ///
    /// assert_eq!(pkg.reference(), "ghcr.io/user/repo");
    /// ```
    #[must_use]
    pub fn reference(&self) -> String {
        format!("{}/{}", self.registry, self.repository)
    }

    /// Returns the full reference string with the most recent tag.
    ///
    /// Uses the first tag in [`tags`](KnownPackage::tags), or `"latest"` when
    /// no tags are present.
    ///
    /// # Example
    ///
    /// ```rust
    /// use wasm_meta_registry_types::KnownPackage;
    ///
    /// let pkg = KnownPackage {
    ///     registry: "ghcr.io".into(),
    ///     repository: "user/repo".into(),
    ///     kind: None,
    ///     description: None,
    ///     tags: vec!["v1.0".into(), "latest".into()],
    ///     signature_tags: vec![],
    ///     attestation_tags: vec![],
    ///     last_seen_at: String::new(),
    ///     created_at: String::new(),
    ///     wit_namespace: None,
    ///     wit_name: None,
    ///     dependencies: vec![],
    /// };
    ///
    /// assert_eq!(pkg.reference_with_tag(), "ghcr.io/user/repo:v1.0");
    /// ```
    #[must_use]
    pub fn reference_with_tag(&self) -> String {
        if let Some(tag) = self.tags.first() {
            format!("{}:{}", self.reference(), tag)
        } else {
            format!("{}:latest", self.reference())
        }
    }
}

// ============================================================
// New types for the rich API
// ============================================================

/// Full detail view of a package, including all known versions and metadata.
///
/// Returned by `GET /v1/packages/detail/{registry}/{*repo}`.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::{PackageDetail, PackageKind};
///
/// let detail = PackageDetail {
///     registry: "ghcr.io".into(),
///     repository: "webassembly/wasi/http".into(),
///     kind: Some(PackageKind::Interface),
///     description: Some("WASI HTTP interfaces".into()),
///     wit_namespace: Some("wasi".into()),
///     wit_name: Some("http".into()),
///     versions: vec![],
/// };
///
/// assert_eq!(detail.registry, "ghcr.io");
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageDetail {
    /// Registry hostname.
    pub registry: String,
    /// Repository path.
    pub repository: String,
    /// Whether this package is a component or an interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PackageKind>,
    /// Optional package description.
    pub description: Option<String>,
    /// Optional WIT namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wit_namespace: Option<String>,
    /// Optional WIT package name within the namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wit_name: Option<String>,
    /// All known versions of this package, ordered by most recent first.
    pub versions: Vec<PackageVersion>,
}

/// Metadata for a single version of a package.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::PackageVersion;
///
/// let version = PackageVersion {
///     tag: Some("0.3.0".into()),
///     digest: "sha256:abcdef1234".into(),
///     size_bytes: Some(1024),
///     created_at: Some("2025-01-01T00:00:00Z".into()),
///     synced_at: Some("2025-01-02T00:00:00Z".into()),
///     annotations: None,
///     worlds: vec![],
///     components: vec![],
///     dependencies: vec![],
///     referrers: vec![],
///     layers: vec![],
///     wit_text: None,
///     type_docs: std::collections::HashMap::new(),
/// };
///
/// assert_eq!(version.digest, "sha256:abcdef1234");
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageVersion {
    /// The version tag, if any (e.g. `"0.3.0"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Content-addressable digest of the manifest.
    pub digest: String,
    /// Total size of the manifest and its layers in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    /// ISO 8601 creation timestamp from the OCI manifest annotation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// ISO 8601 timestamp for when the registry first recorded this manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<String>,
    /// Well-known OCI annotations extracted from the manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<OciAnnotations>,
    /// WIT worlds defined in this version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worlds: Vec<WitWorldSummary>,
    /// Wasm components found in this version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ComponentSummary>,
    /// Declared WIT dependencies for this version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<PackageDependencyRef>,
    /// Referrers (signatures, SBOMs, attestations) for this version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub referrers: Vec<ReferrerSummary>,
    /// OCI layers in this manifest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<LayerInfo>,
    /// The WIT source text, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wit_text: Option<String>,
    /// Cross-package type documentation, keyed by fully qualified type name
    /// (e.g. `"wasi:io/poll/pollable"` → `"A \"pollable\" handle..."`).
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub type_docs: std::collections::HashMap<String, String>,
}

/// Metadata for a single OCI layer.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::LayerInfo;
///
/// let layer = LayerInfo {
///     digest: "sha256:abc123".into(),
///     media_type: Some("application/wasm".into()),
///     size_bytes: Some(1024),
/// };
///
/// assert_eq!(layer.media_type.as_deref(), Some("application/wasm"));
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayerInfo {
    /// Content-addressable digest (e.g. `"sha256:fedcba…"`).
    pub digest: String,
    /// MIME type (e.g. `"application/wasm"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Size of this layer in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
}

/// A WIT world with its declared imports and exports.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::WitWorldSummary;
///
/// let world = WitWorldSummary {
///     name: "proxy".into(),
///     description: None,
///     imports: vec![],
///     exports: vec![],
/// };
///
/// assert_eq!(world.name, "proxy");
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WitWorldSummary {
    /// The world's name within its package (e.g. `"proxy"`, `"command"`).
    pub name: String,
    /// Optional human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Interfaces this world imports (depends on).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<WitInterfaceRef>,
    /// Interfaces this world exports (implements).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<WitInterfaceRef>,
}

/// Reference to a declared WIT interface in an import or export.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::WitInterfaceRef;
///
/// let iface = WitInterfaceRef {
///     package: "wasi:io".into(),
///     interface: Some("streams".into()),
///     version: Some("0.2.2".into()),
///     docs: None,
/// };
///
/// assert_eq!(iface.package, "wasi:io");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WitInterfaceRef {
    /// Declared package name (e.g. `"wasi:io"`).
    pub package: String,
    /// Declared sub-interface name (e.g. `"streams"`).
    /// `None` means the entire package is imported/exported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    /// Declared version (e.g. `"0.2.2"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// First sentence of the interface's documentation, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

/// Summary of a compiled Wasm component found in an OCI manifest.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::ComponentSummary;
///
/// let component = ComponentSummary {
///     name: Some("my-handler".into()),
///     description: None,
///     targets: vec![],
///     producers: vec![],
///     kind: Some("component".into()),
///     size_bytes: None,
///     languages: vec![],
///     children: vec![],
///     source: None,
///     homepage: None,
///     licenses: None,
///     authors: None,
///     revision: None,
///     component_version: None,
///     bill_of_materials: vec![],
///     imports: vec![],
///     exports: vec![],
/// };
///
/// assert_eq!(component.name.as_deref(), Some("my-handler"));
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComponentSummary {
    /// Human-readable name extracted from the component's metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional description from the component's metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// WIT worlds this component targets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<ComponentTargetRef>,
    /// Producer toolchain entries (e.g. language, SDK, processed-by).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub producers: Vec<ProducerEntry>,
    /// Whether this is a "component" or "module".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Total size in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Languages used (extracted from producers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    /// Nested child components or modules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ComponentSummary>,
    /// Source code URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Homepage URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// SPDX license expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licenses: Option<String>,
    /// Authors string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<String>,
    /// Source control revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Software version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_version: Option<String>,
    /// Source-level dependencies (bill of materials).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bill_of_materials: Vec<BomEntry>,
    /// WIT imports (interfaces this component/module depends on).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<WitInterfaceRef>,
    /// WIT exports (interfaces this component/module provides).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<WitInterfaceRef>,
}

/// A source-level dependency from the component's bill of materials.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BomEntry {
    /// Dependency name.
    pub name: String,
    /// Dependency version.
    pub version: String,
    /// Source kind (e.g. "crates.io", "git", "local", "registry").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// A single producer toolchain entry (e.g. `language = "Rust" [1.82.0]`).
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::ProducerEntry;
///
/// let entry = ProducerEntry {
///     field: "language".into(),
///     name: "Rust".into(),
///     version: "1.82.0".into(),
/// };
///
/// assert_eq!(entry.field, "language");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProducerEntry {
    /// Producer field name (e.g. `"language"`, `"processed-by"`, `"sdk"`).
    pub field: String,
    /// Tool or language name (e.g. `"Rust"`, `"wit-component"`).
    pub name: String,
    /// Version string (empty if unknown).
    pub version: String,
}

/// Reference to a WIT world that a component targets.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::ComponentTargetRef;
///
/// let target = ComponentTargetRef {
///     package: "wasi:http".into(),
///     world: "proxy".into(),
///     version: Some("0.3.0".into()),
/// };
///
/// assert_eq!(target.world, "proxy");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ComponentTargetRef {
    /// Declared package name of the targeted world (e.g. `"wasi:http"`).
    pub package: String,
    /// Declared world name (e.g. `"proxy"`).
    pub world: String,
    /// Declared version (e.g. `"0.3.0"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Well-known OCI manifest annotations promoted to structured fields.
///
/// Corresponds to the `org.opencontainers.image.*` annotation keys.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::OciAnnotations;
///
/// let annotations = OciAnnotations {
///     authors: Some("WASI team".into()),
///     licenses: Some("Apache-2.0".into()),
///     ..OciAnnotations::default()
/// };
///
/// assert_eq!(annotations.licenses.as_deref(), Some("Apache-2.0"));
/// ```
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OciAnnotations {
    /// `org.opencontainers.image.created` — date/time the image was built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// `org.opencontainers.image.authors` — contact details for maintainers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<String>,
    /// `org.opencontainers.image.url` — URL for more information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// `org.opencontainers.image.documentation` — documentation URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// `org.opencontainers.image.source` — source code URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `org.opencontainers.image.version` — version of packaged software.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `org.opencontainers.image.revision` — source control revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// `org.opencontainers.image.vendor` — distributing entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    /// `org.opencontainers.image.licenses` — SPDX license expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licenses: Option<String>,
    /// `org.opencontainers.image.title` — human-readable title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `org.opencontainers.image.description` — human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional custom annotations not in the well-known set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<AnnotationEntry>,
}

/// A single custom annotation key-value pair.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::AnnotationEntry;
///
/// let entry = AnnotationEntry {
///     key: "com.example.custom".into(),
///     value: "hello".into(),
/// };
///
/// assert_eq!(entry.key, "com.example.custom");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AnnotationEntry {
    /// The full annotation key.
    pub key: String,
    /// The annotation value.
    pub value: String,
}

/// An OCI referrer — an artifact that references a subject manifest,
/// such as a signature, SBOM, or attestation.
///
/// # Example
///
/// ```rust
/// use wasm_meta_registry_types::ReferrerSummary;
///
/// let referrer = ReferrerSummary {
///     artifact_type: "application/vnd.dev.cosign.simplesigning.v1+json".into(),
///     digest: "sha256:fedcba9876".into(),
/// };
///
/// assert_eq!(referrer.artifact_type, "application/vnd.dev.cosign.simplesigning.v1+json");
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReferrerSummary {
    /// The OCI artifact type of the referrer.
    pub artifact_type: String,
    /// Content-addressable digest of the referrer manifest.
    pub digest: String,
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify client.known-package.reference]
    #[test]
    fn known_package_reference() {
        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            kind: None,
            description: None,
            tags: vec![],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
            wit_namespace: None,
            wit_name: None,
            dependencies: vec![],
        };
        assert_eq!(pkg.reference(), "ghcr.io/user/repo");
    }

    // r[verify client.known-package.reference-with-tag]
    #[test]
    fn known_package_reference_with_tag() {
        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            kind: None,
            description: None,
            tags: vec!["v1.0".into(), "latest".into()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
            wit_namespace: None,
            wit_name: None,
            dependencies: vec![],
        };
        assert_eq!(pkg.reference_with_tag(), "ghcr.io/user/repo:v1.0");
    }

    // r[verify client.known-package.reference-default-tag]
    #[test]
    fn known_package_reference_with_tag_default() {
        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            kind: None,
            description: None,
            tags: vec![],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
            wit_namespace: None,
            wit_name: None,
            dependencies: vec![],
        };
        assert_eq!(pkg.reference_with_tag(), "ghcr.io/user/repo:latest");
    }

    // r[verify client.known-package.dependencies]
    #[test]
    fn known_package_dependencies_serialization() {
        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            kind: None,
            description: None,
            tags: vec!["v1.0".into()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
            wit_namespace: Some("wasi".into()),
            wit_name: Some("http".into()),
            dependencies: vec![
                PackageDependencyRef {
                    package: "wasi:io".into(),
                    version: Some("0.2.0".into()),
                },
                PackageDependencyRef {
                    package: "wasi:clocks".into(),
                    version: None,
                },
            ],
        };

        let json = serde_json::to_string(&pkg).unwrap();
        let parsed: KnownPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.dependencies.len(), 2);
        assert_eq!(parsed.dependencies[0].package, "wasi:io");
        assert_eq!(parsed.dependencies[0].version.as_deref(), Some("0.2.0"));
        assert_eq!(parsed.dependencies[1].package, "wasi:clocks");
        assert!(parsed.dependencies[1].version.is_none());
    }

    // r[verify client.known-package.dependencies]
    #[test]
    fn known_package_empty_dependencies_skipped_in_json() {
        let pkg = KnownPackage {
            registry: "ghcr.io".into(),
            repository: "user/repo".into(),
            kind: None,
            description: None,
            tags: vec![],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: String::new(),
            created_at: String::new(),
            wit_namespace: None,
            wit_name: None,
            dependencies: vec![],
        };

        let json = serde_json::to_string(&pkg).unwrap();
        // Empty dependencies should not appear in JSON
        assert!(!json.contains("dependencies"));
    }

    #[test]
    fn package_version_roundtrip() {
        let version = PackageVersion {
            tag: Some("0.3.0".into()),
            digest: "sha256:abcdef".into(),
            size_bytes: Some(2048),
            created_at: Some("2025-01-01T00:00:00Z".into()),
            synced_at: Some("2025-01-02T00:00:00Z".into()),
            annotations: Some(OciAnnotations {
                licenses: Some("Apache-2.0".into()),
                ..OciAnnotations::default()
            }),
            worlds: vec![WitWorldSummary {
                name: "proxy".into(),
                description: None,
                imports: vec![WitInterfaceRef {
                    package: "wasi:io".into(),
                    interface: Some("streams".into()),
                    version: Some("0.2.2".into()),
                    docs: None,
                }],
                exports: vec![WitInterfaceRef {
                    package: "wasi:http".into(),
                    interface: Some("handler".into()),
                    version: Some("0.3.0".into()),
                    docs: None,
                }],
            }],
            components: vec![ComponentSummary {
                name: Some("my-handler".into()),
                description: None,
                targets: vec![ComponentTargetRef {
                    package: "wasi:http".into(),
                    world: "proxy".into(),
                    version: Some("0.3.0".into()),
                }],
                producers: vec![],
                kind: None,
                size_bytes: None,
                languages: vec![],
                children: vec![],
                source: None,
                homepage: None,
                licenses: None,
                authors: None,
                revision: None,
                component_version: None,
                bill_of_materials: vec![],
                imports: vec![],
                exports: vec![],
            }],
            dependencies: vec![PackageDependencyRef {
                package: "wasi:io".into(),
                version: Some("0.2.2".into()),
            }],
            referrers: vec![ReferrerSummary {
                artifact_type: "application/vnd.dev.cosign.simplesigning.v1+json".into(),
                digest: "sha256:fedcba".into(),
            }],
            wit_text: Some("package wasi:http@0.3.0;".into()),
            layers: vec![],
            type_docs: std::collections::HashMap::new(),
        };

        let json = serde_json::to_string_pretty(&version).unwrap();
        let parsed: PackageVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.digest, "sha256:abcdef");
        assert_eq!(parsed.worlds.len(), 1);
        assert_eq!(parsed.worlds[0].name, "proxy");
        assert_eq!(parsed.components.len(), 1);
        assert_eq!(parsed.referrers.len(), 1);
    }
}
