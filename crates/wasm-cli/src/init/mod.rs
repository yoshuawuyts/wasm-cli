use std::path::PathBuf;

use crate::util::write_lock_file;

/// Options for the `init` command.
#[derive(clap::Parser)]
pub(crate) struct Opts {
    /// The directory in which to create the wasm package files.
    ///
    /// Defaults to the current directory.
    #[arg(default_value = ".")]
    path: PathBuf,
}

impl Opts {
    pub(crate) async fn run(self) -> anyhow::Result<()> {
        let base = &self.path;
        let deps = base.join("deps");

        tokio::fs::create_dir_all(deps.join("vendor/wit")).await?;
        tokio::fs::create_dir_all(deps.join("vendor/wasm")).await?;

        // Create composition workspace directories
        tokio::fs::create_dir_all(base.join("types")).await?;
        tokio::fs::create_dir_all(base.join("seams")).await?;
        tokio::fs::create_dir_all(base.join("build")).await?;

        let manifest = wasm_manifest::Manifest::default();
        let manifest = toml::to_string_pretty(&manifest)?;
        tokio::fs::write(deps.join("wasm.toml"), manifest.as_bytes()).await?;

        let lockfile = wasm_manifest::Lockfile::default();
        write_lock_file(deps.join("wasm.lock.toml"), &lockfile).await?;

        Ok(())
    }
}
