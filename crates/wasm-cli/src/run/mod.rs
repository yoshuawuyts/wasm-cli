//! Execute a Wasm Component via Wasmtime.
//!
//! Runs a Wasm Component from a local file or OCI reference. The component is
//! sandboxed by default — WASI capabilities (env, filesystem, network, stdio)
//! are only granted through CLI flags or layered config.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use wasmparser::{Encoding, Parser, Payload};
use wasmtime::component::Component;
use wasmtime::{Engine, Store};
use wasmtime_wasi::p2::bindings::sync::Command;
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtxBuilder, WasiCtxView, WasiView};

use wasm_manifest::RunPermissions;
use wasm_package_manager::Manager;

/// Options for the `wasm run` command.
#[derive(clap::Parser)]
pub(crate) struct Opts {
    /// Local file path or OCI reference to a Wasm Component.
    #[arg(value_name = "INPUT")]
    input: String,

    /// Pass an environment variable to the guest (repeatable).
    #[arg(long = "env", value_name = "KEY=VAL", num_args = 1)]
    envs: Vec<String>,

    /// Pre-open a host directory for the guest (repeatable).
    #[arg(long = "dir", value_name = "HOST_PATH")]
    dirs: Vec<PathBuf>,

    /// Inherit all host environment variables.
    #[arg(long)]
    inherit_env: bool,

    /// Allow the guest to access the network.
    #[arg(long)]
    inherit_network: bool,

    /// Suppress stdin/stdout/stderr inheritance.
    #[arg(long)]
    no_stdio: bool,
}

/// Host state wired into `Store<WasiState>`.
struct WasiState {
    ctx: wasmtime_wasi::WasiCtx,
    table: ResourceTable,
}

impl WasiView for WasiState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

impl Opts {
    /// Execute the `run` command.
    pub(crate) async fn run(self, offline: bool) -> Result<()> {
        // 1. Resolve input — local files take priority over OCI references.
        let local_path = PathBuf::from(&self.input);
        let is_local = local_path.exists();

        // Only try OCI when the input is not a local file.
        let reference = if is_local {
            None
        } else {
            crate::util::parse_reference(&self.input).ok()
        };

        // 2. Get Wasm bytes.
        let bytes = if let Some(ref oci_ref) = reference {
            let manager = if offline {
                Manager::open_offline().await?
            } else {
                Manager::open().await?
            };
            let _pull_result = manager.pull(oci_ref.clone()).await?;
            let key = oci_ref.whole();
            manager
                .get(&key)
                .await
                .with_context(|| format!("failed to read cached component for {key}"))?
        } else {
            tokio::fs::read(&local_path)
                .await
                .with_context(|| format!("failed to read {}", local_path.display()))?
        };

        // 3. Validate — must be a Wasm Component.
        validate_component(&bytes)?;

        // 4. Resolve permissions (4-layer merge).
        let permissions = self.resolve_permissions(reference.as_ref())?;

        // 5. Build Wasmtime runtime and execute.
        //    This is CPU-bound work so we use spawn_blocking.
        let result = tokio::task::spawn_blocking(move || execute_component(&bytes, &permissions))
            .await
            .context("runtime task panicked")??;

        // 6. Map exit.
        if let Err(()) = result {
            std::process::exit(1);
        }
        Ok(())
    }

    /// Build a [`RunPermissions`] from CLI flags (only the explicitly
    /// provided flags are `Some`).
    fn cli_permissions(&self) -> RunPermissions {
        let mut perms = RunPermissions::default();

        if self.inherit_env {
            perms.inherit_env = Some(true);
        }
        if !self.envs.is_empty() {
            perms.allow_env = Some(self.envs.clone());
        }
        if !self.dirs.is_empty() {
            perms.allow_dirs = Some(self.dirs.clone());
        }
        if self.no_stdio {
            perms.inherit_stdio = Some(false);
        }
        if self.inherit_network {
            perms.inherit_network = Some(true);
        }

        perms
    }

    /// Resolve permissions through the 4-layer merge:
    ///
    /// 1. Global defaults from `config.toml` → `[run.permissions]`
    /// 2. Global per-component from `components.toml`
    /// 3. Local per-component from `wasm.toml`
    /// 4. CLI flags
    fn resolve_permissions(
        &self,
        reference: Option<&wasm_package_manager::Reference>,
    ) -> Result<wasm_manifest::ResolvedPermissions> {
        // Layer 1: global config defaults
        let config = wasm_package_manager::Config::load().unwrap_or_default();
        let base = config.run.map(|r| r.permissions).unwrap_or_default();

        // Layer 2: global components.toml per-component override
        let global_component = wasm_package_manager::Config::load_components()
            .ok()
            .flatten()
            .and_then(|manifest| find_matching_permissions(&manifest, reference))
            .unwrap_or_default();
        let merged = base.merge(global_component);

        // Layer 3: local wasm.toml per-component override
        let local_manifest = std::fs::read_to_string("deps/wasm.toml")
            .ok()
            .and_then(|s| toml::from_str::<wasm_manifest::Manifest>(&s).ok());
        let local_component = local_manifest
            .and_then(|m| find_matching_permissions(&m, reference))
            .unwrap_or_default();
        let merged = merged.merge(local_component);

        // Layer 4: CLI flags
        let cli = self.cli_permissions();
        let merged = merged.merge(cli);

        Ok(merged.resolve())
    }
}

/// Look through a manifest for a dependency whose OCI reference matches
/// the given reference and return its permissions (if any).
///
/// Matching is performed by comparing `registry/namespace/package` (without
/// the tag) against each explicit dependency in the manifest.
fn find_matching_permissions(
    manifest: &wasm_manifest::Manifest,
    reference: Option<&wasm_package_manager::Reference>,
) -> Option<RunPermissions> {
    let reference = reference?;
    let ref_registry = reference.registry();
    let ref_repository = reference.repository();

    for (_, dep) in manifest.components.iter().chain(manifest.interfaces.iter()) {
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

/// Confirm the bytes are a Wasm Component (not a core module or WIT-only package).
fn validate_component(bytes: &[u8]) -> Result<()> {
    let parser = Parser::new(0);
    for payload in parser.parse_all(bytes) {
        match payload {
            Ok(Payload::Version { encoding, .. }) => {
                return match encoding {
                    Encoding::Component => Ok(()),
                    Encoding::Module => {
                        bail!(
                            "only Wasm Components can be executed; this appears to be a core module"
                        )
                    }
                };
            }
            Err(e) => bail!("invalid Wasm binary: {e}"),
            _ => {}
        }
    }
    bail!("invalid Wasm binary: no version header found")
}

/// Build the Wasmtime runtime, instantiate the component, and invoke
/// `wasi:cli/run@0.2.0#run`.
fn execute_component(
    bytes: &[u8],
    permissions: &wasm_manifest::ResolvedPermissions,
) -> Result<Result<(), ()>> {
    let engine = Engine::default();
    let component = Component::new(&engine, bytes).context("failed to compile Wasm Component")?;

    // Build WASI context from resolved permissions.
    let mut builder = WasiCtxBuilder::new();

    if permissions.inherit_stdio {
        builder.inherit_stdio();
    }
    if permissions.inherit_env {
        builder.inherit_env();
    }
    // Forward explicitly allowed env vars.
    // Entries containing '=' are treated as KEY=VAL pairs (from --env flags);
    // entries without '=' are treated as variable names to look up from the host.
    for entry in &permissions.allow_env {
        if let Some((k, v)) = entry.split_once('=') {
            builder.env(k, v);
        } else if let Ok(v) = std::env::var(entry) {
            builder.env(entry, &v);
        }
    }
    // Pre-open directories with full read/write permissions.
    for dir in &permissions.allow_dirs {
        builder
            .preopened_dir(
                dir,
                dir.to_string_lossy(),
                DirPerms::all(),
                FilePerms::all(),
            )
            .with_context(|| format!("failed to pre-open directory: {}", dir.display()))?;
    }
    if permissions.inherit_network {
        builder.inherit_network();
    }

    let wasi_ctx = builder.build();
    let state = WasiState {
        ctx: wasi_ctx,
        table: ResourceTable::new(),
    };
    let mut store = Store::new(&engine, state);

    let mut linker = wasmtime::component::Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;

    let command = Command::instantiate(&mut store, &component, &linker)
        .context("failed to instantiate Wasm Component")?;

    let result = command.wasi_cli_run().call_run(&mut store);
    match result {
        Ok(Ok(())) => Ok(Ok(())),
        Ok(Err(())) => {
            eprintln!("Error: guest returned a non-zero exit code");
            Ok(Err(()))
        }
        Err(e) => {
            eprintln!("Error: {e:#}");
            Ok(Err(()))
        }
    }
}
