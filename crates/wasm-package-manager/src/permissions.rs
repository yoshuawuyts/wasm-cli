//! Permission resolution for running WebAssembly components.
//!
//! Implements the 4-layer permission merge:
//!
//! 1. Global defaults from `config.toml` → `[run.permissions]`
//! 2. Global per-component from `components.toml`
//! 3. Local per-component from `wasm.toml`
//! 4. Caller-provided overrides (e.g. CLI flags)

use wasm_manifest::{ResolvedPermissions, RunPermissions};

use crate::Reference;
use crate::config::Config;

/// Resolve permissions through the 4-layer merge.
///
/// The `cli_overrides` parameter represents the outermost layer (e.g. CLI
/// flags). Pass [`RunPermissions::default()`] when there are no overrides.
///
/// # Layers
///
/// 1. Global defaults from `config.toml` → `[run.permissions]`
/// 2. Global per-component from `components.toml`
/// 3. Local per-component from `wasm.toml`
/// 4. `cli_overrides`
pub fn resolve_permissions(
    reference: Option<&Reference>,
    cli_overrides: RunPermissions,
) -> ResolvedPermissions {
    // Layer 1: global config defaults
    let config = Config::load().unwrap_or_default();
    let base = config.run.map(|r| r.permissions).unwrap_or_default();

    // Layer 2: global components.toml per-component override
    let global_component = Config::load_components()
        .ok()
        .flatten()
        .and_then(|manifest| find_matching_permissions(&manifest, reference))
        .unwrap_or_default();
    let merged = base.merge(global_component);

    // Layer 3: local wasm.toml per-component override
    let local_manifest = std::fs::read_to_string("wasm.toml")
        .ok()
        .and_then(|s| toml::from_str::<wasm_manifest::Manifest>(&s).ok());
    let local_component = local_manifest
        .and_then(|m| find_matching_permissions(&m, reference))
        .unwrap_or_default();
    let merged = merged.merge(local_component);

    // Layer 4: caller-provided overrides
    let merged = merged.merge(cli_overrides);

    merged.resolve()
}

/// Look through a manifest for a dependency whose OCI reference matches
/// the given reference and return its permissions (if any).
///
/// Matching is performed by comparing `registry/namespace/package` (without
/// the tag) against each explicit dependency in the manifest.
#[must_use]
pub fn find_matching_permissions(
    manifest: &wasm_manifest::Manifest,
    reference: Option<&Reference>,
) -> Option<RunPermissions> {
    let reference = reference?;
    let ref_registry = reference.registry();
    let ref_repository = reference.repository();

    for (_, dep) in manifest
        .dependencies
        .components
        .iter()
        .chain(manifest.dependencies.interfaces.iter())
    {
        match dep {
            wasm_manifest::Dependency::Explicit {
                registry,
                namespace,
                package,
                permissions,
                ..
            } => {
                let dep_repository = format!("{namespace}/{package}");
                if registry == ref_registry && dep_repository == ref_repository {
                    return permissions.clone();
                }
            }
            wasm_manifest::Dependency::Compact(_) => {}
        }
    }
    None
}
