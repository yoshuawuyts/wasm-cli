use std::path::PathBuf;

use miette::{IntoDiagnostic, WrapErr};

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
    pub(crate) async fn run(self) -> miette::Result<()> {
        let base = &self.path;
        let deps = base.join("deps");

        tokio::fs::create_dir_all(deps.join("vendor/wit"))
            .await
            .into_diagnostic()
            .wrap_err("failed to create vendor/wit directory")?;
        tokio::fs::create_dir_all(deps.join("vendor/wasm"))
            .await
            .into_diagnostic()
            .wrap_err("failed to create vendor/wasm directory")?;

        // Create composition workspace directories
        tokio::fs::create_dir_all(base.join("types"))
            .await
            .into_diagnostic()
            .wrap_err("failed to create types directory")?;
        tokio::fs::create_dir_all(base.join("seams"))
            .await
            .into_diagnostic()
            .wrap_err("failed to create seams directory")?;
        tokio::fs::create_dir_all(base.join("build"))
            .await
            .into_diagnostic()
            .wrap_err("failed to create build directory")?;

        let manifest = wasm_manifest::Manifest::default();
        let manifest = toml::to_string_pretty(&manifest).into_diagnostic()?;
        tokio::fs::write(deps.join("wasm.toml"), manifest.as_bytes())
            .await
            .into_diagnostic()
            .wrap_err("failed to write wasm.toml")?;

        let lockfile = wasm_manifest::Lockfile::default();
        write_lock_file(deps.join("wasm.lock.toml"), &lockfile)
            .await
            .into_diagnostic()
            .wrap_err("failed to write wasm.lock.toml")?;

        Ok(())
    }
}
