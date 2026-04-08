use super::models::RawKnownPackage;
use wasm_meta_registry_types::PackageKind;

// Re-export the canonical `KnownPackage` from the types crate so that
// existing consumers (`wasm_package_manager::storage::KnownPackage`) keep
// working without any source changes.
pub use wasm_meta_registry_types::KnownPackage;

/// Parameters for upserting a known package entry.
///
/// Groups the arguments that were previously passed individually to
/// `add_known_package_with_params`, eliminating the need for
/// `#[allow(clippy::too_many_arguments)]`.
#[derive(Debug, Clone)]
pub struct KnownPackageParams<'a> {
    /// OCI registry hostname (e.g. `ghcr.io`).
    pub registry: &'a str,
    /// OCI repository path (e.g. `example/my-component`).
    pub repository: &'a str,
    /// Optional tag to associate with this package.
    pub tag: Option<&'a str>,
    /// Human-readable description from OCI annotations.
    pub description: Option<&'a str>,
    /// WIT namespace (e.g. `wasi` in `wasi:http`).
    pub wit_namespace: Option<&'a str>,
    /// WIT package name (e.g. `http` in `wasi:http`).
    pub wit_name: Option<&'a str>,
    /// Whether this package is a component or interface.
    pub kind: Option<PackageKind>,
}

impl From<RawKnownPackage> for KnownPackage {
    fn from(pkg: RawKnownPackage) -> Self {
        Self {
            registry: pkg.registry,
            repository: pkg.repository,
            kind: pkg.kind,
            description: pkg.description,
            tags: pkg.tags,
            signature_tags: pkg.signature_tags,
            attestation_tags: pkg.attestation_tags,
            last_seen_at: pkg.last_seen_at,
            created_at: pkg.created_at,
            wit_namespace: pkg.wit_namespace,
            wit_name: pkg.wit_name,
            dependencies: vec![],
        }
    }
}
