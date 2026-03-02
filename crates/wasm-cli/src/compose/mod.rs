#![allow(clippy::print_stdout, clippy::print_stderr)]

mod resolver;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// How to link dependencies in the composed component.
#[derive(Clone, Debug, Default, clap::ValueEnum)]
pub(crate) enum LinkerMode {
    /// Embed all dependencies into the output component (default).
    #[default]
    Static,
    /// Import dependencies rather than embedding them.
    Dynamic,
}

/// Compose Wasm components from WAC scripts
#[derive(clap::Args)]
pub(crate) struct Opts {
    /// Name of a `.wac` file in `seams/` to compose.
    ///
    /// For example, `wasm compose foo` resolves to `seams/foo.wac`.
    /// If omitted, all `.wac` files in `seams/` are composed.
    #[arg()]
    name: Option<String>,

    /// How to link dependencies.
    #[arg(long, value_enum, default_value_t = LinkerMode::Static)]
    linker: LinkerMode,

    /// Output path for the composed component.
    #[arg(short, long, default_value = "build")]
    output: PathBuf,
}

impl Opts {
    pub(crate) fn run(self) -> Result<()> {
        let wac_files = self.collect_wac_files()?;

        if wac_files.is_empty() {
            bail!("no .wac files found; add files to `seams/`");
        }

        std::fs::create_dir_all(&self.output).with_context(|| {
            format!(
                "could not create output directory '{}'",
                self.output.display()
            )
        })?;

        for wac_file in &wac_files {
            self.compose_one(wac_file)?;
        }

        Ok(())
    }

    /// Collect the `.wac` files to process.
    fn collect_wac_files(&self) -> Result<Vec<PathBuf>> {
        let seams_dir = PathBuf::from("seams");

        if let Some(ref name) = self.name {
            // Reject names with path separators or traversal sequences.
            if name.contains('/') || name.contains('\\') || name.contains("..") {
                bail!("invalid composition name '{name}': must be a plain name, not a path");
            }

            // Treat the argument as a name and look under seams/
            let wac_path = seams_dir.join(format!("{name}.wac"));
            if wac_path.exists() {
                return Ok(vec![wac_path]);
            }

            // Not found — list what's available
            let available = Self::list_available_wac_files(&seams_dir);
            if available.is_empty() {
                bail!("WAC file 'seams/{name}.wac' not found and no .wac files exist in `seams/`");
            }
            bail!(
                "WAC file 'seams/{name}.wac' not found. Available WAC files:\n{}",
                available
                    .iter()
                    .map(|f| format!("  - {f}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
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
    fn compose_one(&self, wac_file: &PathBuf) -> Result<()> {
        let source = std::fs::read_to_string(wac_file)
            .with_context(|| format!("could not read '{}'", wac_file.display()))?;

        let document = wac_parser::Document::parse(&source)
            .map_err(|e| anyhow::anyhow!("parse error in '{}': {e}", wac_file.display()))?;

        let base = std::env::current_dir().context("could not determine current directory")?;
        let fs_resolver = resolver::build_resolver(&base)?;

        let keys = wac_resolver::packages(&document).map_err(|e| {
            anyhow::anyhow!(
                "could not determine packages in '{}': {e}",
                wac_file.display()
            )
        })?;

        let packages = fs_resolver.resolve(&keys).map_err(|e| {
            anyhow::anyhow!(
                "could not resolve packages for '{}': {e}",
                wac_file.display()
            )
        })?;

        let resolution = document
            .resolve(packages)
            .map_err(|e| anyhow::anyhow!("resolution error in '{}': {e}", wac_file.display()))?;

        let mut encode_options = wac_graph::EncodeOptions::default();
        if matches!(self.linker, LinkerMode::Dynamic) {
            encode_options.define_components = false;
        }

        let bytes = resolution
            .encode(encode_options)
            .map_err(|e| anyhow::anyhow!("encode error for '{}': {e}", wac_file.display()))?;

        let stem = wac_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("composed");

        let out_path = self.output.join(format!("{stem}.wasm"));
        std::fs::write(&out_path, bytes)
            .with_context(|| format!("could not write '{}'", out_path.display()))?;
        println!("Composed component written to {}", out_path.display());

        Ok(())
    }
}
