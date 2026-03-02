#![allow(clippy::print_stdout, clippy::print_stderr)]

mod resolver;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

/// Where to write the composed component.
#[derive(Clone, Debug, clap::ValueEnum)]
pub(crate) enum OutputFormat {
    /// Write to the `build/` directory (default).
    Build,
    /// Write to stdout.
    Stdout,
}

/// Compose Wasm components from WAC scripts
#[derive(clap::Args)]
pub(crate) struct Opts {
    /// Path to a `.wac` file. If omitted, scans the `seams/` directory.
    #[arg()]
    file: Option<PathBuf>,

    /// Where to write the composed component.
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Build)]
    output: OutputFormat,

    /// Import dependencies instead of embedding them.
    #[arg(long)]
    import_dependencies: bool,
}

impl Opts {
    pub(crate) fn run(self) -> Result<()> {
        let wac_files = self.collect_wac_files()?;

        if wac_files.is_empty() {
            bail!("no .wac files found; provide a path or add files to `seams/`");
        }

        for wac_file in &wac_files {
            self.compose_one(wac_file)?;
        }

        Ok(())
    }

    /// Collect the `.wac` files to process.
    fn collect_wac_files(&self) -> Result<Vec<PathBuf>> {
        if let Some(ref file) = self.file {
            if !file.exists() {
                bail!("WAC file '{}' not found", file.display());
            }
            return Ok(vec![file.clone()]);
        }

        let seams_dir = PathBuf::from("seams");
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
        if self.import_dependencies {
            encode_options.define_components = false;
        }

        let bytes = resolution
            .encode(encode_options)
            .map_err(|e| anyhow::anyhow!("encode error for '{}': {e}", wac_file.display()))?;

        let stem = wac_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("composed");

        match self.output {
            OutputFormat::Build => {
                let build_dir = PathBuf::from("build");
                std::fs::create_dir_all(&build_dir).with_context(|| {
                    format!(
                        "could not create output directory '{}'",
                        build_dir.display()
                    )
                })?;
                let out_path = build_dir.join(format!("{stem}.wasm"));
                std::fs::write(&out_path, bytes)
                    .with_context(|| format!("could not write '{}'", out_path.display()))?;
                println!("Composed component written to {}", out_path.display());
            }
            OutputFormat::Stdout => {
                use std::io::Write;
                std::io::stdout()
                    .write_all(&bytes)
                    .context("could not write to stdout")?;
            }
        }

        Ok(())
    }
}
