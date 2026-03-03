use super::models::RawKnownPackage;

// Re-export the canonical `KnownPackage` from the client crate so that
// existing consumers (`wasm_package_manager::storage::KnownPackage`) keep
// working without any source changes.
pub use wasm_meta_registry_client::KnownPackage;

impl From<RawKnownPackage> for KnownPackage {
    fn from(pkg: RawKnownPackage) -> Self {
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
