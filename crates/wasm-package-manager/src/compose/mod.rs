//! WAC-based component composition.
//!
//! This module provides functionality to compose Wasm components from `.wac`
//! scripts using the WAC toolchain (parser, resolver, graph encoder).
//!
//! Requires the `compose` feature to be enabled.

mod errors;
mod resolver;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
pub use errors::ComposeError;

/// How to link dependencies in the composed component.
#[derive(Clone, Debug, Default)]
pub enum LinkerMode {
    /// Embed all dependencies into the output component (default).
    #[default]
    Static,
    /// Import dependencies rather than embedding them.
    Dynamic,
}

/// Compose a set of `.wac` files under a `seams/` directory.
///
/// If `name` is `Some`, only the named `.wac` file is composed.
/// If `name` is `None`, all `.wac` files in `seams/` are composed.
///
/// Composed components are written to the `output` directory.
///
/// # Errors
///
/// Returns an error if no `.wac` files are found, the named file
/// does not exist, or any composition step fails.
pub fn compose(name: Option<&str>, linker: &LinkerMode, output: &Path) -> Result<Vec<PathBuf>> {
    let wac_files = collect_wac_files(name)?;

    if wac_files.is_empty() {
        return Err(ComposeError::NoWacFiles.into());
    }

    std::fs::create_dir_all(output)
        .with_context(|| format!("could not create output directory '{}'", output.display()))?;

    let mut results = Vec::new();
    for wac_file in &wac_files {
        let out_path = compose_one(wac_file, linker, output)?;
        results.push(out_path);
    }

    Ok(results)
}

/// Collect the `.wac` files to process.
fn collect_wac_files(name: Option<&str>) -> Result<Vec<PathBuf>> {
    let seams_dir = PathBuf::from("seams");

    if let Some(name) = name {
        // Reject names with path separators or traversal sequences.
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(ComposeError::InvalidName {
                name: name.to_string(),
            }
            .into());
        }

        // Treat the argument as a name and look under seams/
        let wac_path = seams_dir.join(format!("{name}.wac"));
        if wac_path.exists() {
            return Ok(vec![wac_path]);
        }

        // Not found — list what's available
        let available = list_available_wac_files(&seams_dir);
        let hint = if available.is_empty() {
            "no .wac files exist in `seams/`".to_string()
        } else {
            format!(
                "available WAC files:\n{}",
                available
                    .iter()
                    .map(|f| format!("  - {f}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        return Err(ComposeError::WacNotFound {
            name: name.to_string(),
            hint,
        }
        .into());
    }

    // No name given — compose all .wac files in seams/
    if !seams_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in std::fs::read_dir(&seams_dir)
        .with_context(|| format!("could not read '{}'", seams_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("wac") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// List available `.wac` file stems in the seams directory.
fn list_available_wac_files(seams_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(seams_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wac") {
                path.file_stem().and_then(|s| s.to_str()).map(String::from)
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

/// Parse, resolve, and encode a single `.wac` file.
///
/// Returns the path of the composed output file.
fn compose_one(wac_file: &Path, linker: &LinkerMode, output: &Path) -> Result<PathBuf> {
    let source = std::fs::read_to_string(wac_file)
        .with_context(|| format!("could not read '{}'", wac_file.display()))?;

    let document = wac_parser::Document::parse(&source).map_err(|e| ComposeError::ParseFailed {
        file: wac_file.display().to_string(),
        reason: e.to_string(),
    })?;

    let base = std::env::current_dir().context("could not determine current directory")?;
    let fs_resolver = resolver::build_resolver(&base)?;

    let keys =
        wac_resolver::packages(&document).map_err(|e| ComposeError::PackageDiscoveryFailed {
            file: wac_file.display().to_string(),
            reason: e.to_string(),
        })?;

    let packages =
        fs_resolver
            .resolve(&keys)
            .map_err(|e| ComposeError::PackageResolutionFailed {
                file: wac_file.display().to_string(),
                reason: e.to_string(),
            })?;

    let resolution = document
        .resolve(packages)
        .map_err(|e| ComposeError::ResolutionFailed {
            file: wac_file.display().to_string(),
            reason: e.to_string(),
        })?;

    let mut encode_options = wac_graph::EncodeOptions::default();
    if matches!(linker, LinkerMode::Dynamic) {
        encode_options.define_components = false;
    }

    let bytes = resolution
        .encode(encode_options)
        .map_err(|e| ComposeError::EncodeFailed {
            file: wac_file.display().to_string(),
            reason: e.to_string(),
        })?;

    let stem = wac_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("composed");

    let out_path = output.join(format!("{stem}.wasm"));
    std::fs::write(&out_path, bytes)
        .with_context(|| format!("could not write '{}'", out_path.display()))?;

    Ok(out_path)
}
