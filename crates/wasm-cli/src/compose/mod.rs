#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::Result;
use wasm_package_manager::compose;

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
    /// Name of a `.wac` file in `seams/` to compose. For example, `wasm compose
    /// foo` resolves to `seams/foo.wac`. If omitted, all `.wac` files in
    /// `seams/` are composed.
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
        let linker = match self.linker {
            LinkerMode::Static => compose::LinkerMode::Static,
            LinkerMode::Dynamic => compose::LinkerMode::Dynamic,
        };

        let results = compose::compose(self.name.as_deref(), &linker, &self.output)?;

        for out_path in &results {
            println!("Composed component written to {}", out_path.display());
        }

        Ok(())
    }
}
