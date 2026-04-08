# wasm-meta-registry-types

Shared wire types for the
[wasm-meta-registry](../wasm-meta-registry) API.

This crate contains only data types with `serde` derive implementations — no
HTTP client, no server code, no database access. It is the single source of
truth for the JSON shapes exchanged between the meta-registry server and its
clients.

## Usage

```rust
use wasm_meta_registry_types::{KnownPackage, PackageDependencyRef};

let pkg = KnownPackage {
    registry: "ghcr.io".into(),
    repository: "user/my-component".into(),
    kind: Some(wasm_meta_registry_types::PackageKind::Component),
    description: Some("A useful component".into()),
    tags: vec!["v1.0.0".into()],
    signature_tags: vec![],
    attestation_tags: vec![],
    last_seen_at: "2025-01-01T00:00:00Z".into(),
    created_at: "2024-06-15T12:00:00Z".into(),
    wit_namespace: None,
    wit_name: None,
    dependencies: vec![],
};

assert_eq!(pkg.reference(), "ghcr.io/user/my-component");
```
