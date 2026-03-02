use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use wac_resolver::FileSystemPackageResolver;

/// Build a [`FileSystemPackageResolver`] backed by the project manifest and
/// well-known directories.
///
/// Resolution order:
/// 1. Vendored artifacts listed in `deps/wasm.toml` (components → `deps/vendor/wasm/`,
///    interfaces → `deps/vendor/wit/`).
/// 2. Local files found in the `types/` directory at the project root.
///
/// The returned resolver is intended for use with
/// [`wac_parser::Document::resolve`].
pub(crate) fn build_resolver(base: &Path) -> Result<FileSystemPackageResolver> {
    let deps = base.join("deps");
    let manifest_path = deps.join("wasm.toml");
    let wasm_vendor = deps.join("vendor/wasm");
    let wit_vendor = deps.join("vendor/wit");

    let mut overrides: HashMap<String, PathBuf> = HashMap::new();

    // Read manifest if it exists, and build overrides from it.
    if manifest_path.exists() {
        let manifest_str = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("could not read '{}'", manifest_path.display()))?;
        let manifest: wasm_manifest::Manifest = toml::from_str(&manifest_str)?;

        // Map [components] entries to vendored .wasm files
        for name in manifest.components.keys() {
            let wasm_file = wasm_vendor.join(format!("{name}.wasm"));
            if wasm_file.exists() {
                overrides.insert(name.clone(), wasm_file);
            }
        }

        // Map [interfaces] entries to vendored .wasm or .wit files
        for name in manifest.interfaces.keys() {
            let wasm_file = wit_vendor.join(format!("{name}.wasm"));
            let wit_file = wit_vendor.join(format!("{name}.wit"));
            if wasm_file.exists() {
                overrides.insert(name.clone(), wasm_file);
            } else if wit_file.exists() {
                overrides.insert(name.clone(), wit_file);
            }
        }
    }

    // Also scan types/ for local packages
    let types_dir = base.join("types");
    if types_dir.is_dir() {
        scan_directory_for_packages(&types_dir, &mut overrides)?;
    }

    Ok(FileSystemPackageResolver::new(base, overrides, false))
}

/// Scan a directory for `.wasm` and `.wit` files, adding them to the overrides
/// map. The key is derived from the file stem.
fn scan_directory_for_packages(dir: &Path, overrides: &mut HashMap<String, PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("could not read directory '{}'", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str());
            if matches!(ext, Some("wasm" | "wit"))
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                // Only insert if not already overridden (manifest takes precedence)
                overrides.entry(stem.to_string()).or_insert(path);
            }
        }
    }

    Ok(())
}
