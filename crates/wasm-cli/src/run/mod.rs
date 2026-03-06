#![allow(clippy::print_stdout, clippy::print_stderr)]

//! Execute a Wasm Component via Wasmtime.
//!
//! Runs a Wasm Component from a local file or OCI reference. The component is
//! sandboxed by default — WASI capabilities (env, filesystem, network, stdio)
//! are only granted through CLI flags or layered config.
//!
//! Both `wasi:cli/command` and `wasi:http/proxy` worlds are supported.
//! Components that export `wasi:http/incoming-handler` are served as HTTP
//! servers; all others are executed as CLI commands.

mod errors;
mod http;

use std::net::SocketAddr;
use std::path::PathBuf;

use errors::RunError;
use miette::{Context, IntoDiagnostic};
use wasmparser::{Encoding, Parser, Payload};
use wasmtime::component::Component;
use wasmtime::{Engine, Store};
use wasmtime_wasi::p2::bindings::sync::Command;
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtxBuilder, WasiCtxView, WasiView};

use wasm_manifest::RunPermissions;
use wasm_package_manager::manager::Manager;

/// Options for the `wasm run` command.
#[derive(clap::Parser)]
pub(crate) struct Opts {
    /// Local file path, OCI reference, or manifest key (scope:component)
    /// for a Wasm Component.
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

    /// Address to bind the HTTP server to when running an `wasi:http/proxy`
    /// component (default: `127.0.0.1:8080`).
    #[arg(long, value_name = "ADDR", default_value = "127.0.0.1:8080")]
    listen: SocketAddr,
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
    pub(crate) async fn run(self, offline: bool) -> miette::Result<()> {
        // 1. Resolve input — local files take priority, then manifest keys,
        //    then OCI references.
        let local_path = PathBuf::from(&self.input);
        let is_local = local_path.exists();

        // Try manifest key lookup (scope:component syntax).
        let manifest_path = if is_local {
            None
        } else {
            resolve_manifest_key(&self.input)?
        };

        // Block OCI fallthrough for inputs that look like manifest keys
        // (scope:component) but aren't installed in the local project.
        if !is_local && manifest_path.is_none() && looks_like_manifest_key(&self.input) {
            return Err(not_installed_error(&self.input).await);
        }

        // Only try OCI when the input is not a local file and not a manifest key.
        let reference = if is_local || manifest_path.is_some() {
            None
        } else {
            crate::util::parse_reference(&self.input).ok()
        };

        // 2. Get Wasm bytes.
        let bytes = if let Some(ref vendored) = manifest_path {
            tokio::fs::read(vendored)
                .await
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to read {}", vendored.display()))?
        } else {
            match reference {
                Some(ref oci_ref) => fetch_oci_bytes(oci_ref, offline).await?,
                None => tokio::fs::read(&local_path)
                    .await
                    .into_diagnostic()
                    .wrap_err_with(|| format!("failed to read {}", local_path.display()))?,
            }
        };

        // 3. Validate — must be a Wasm Component.
        validate_component(&bytes)?;

        // 4. Resolve permissions (4-layer merge).
        let permissions = self.resolve_permissions(reference.as_ref());

        // 5. Detect world and execute.
        if exports_http_incoming_handler(&bytes) {
            // wasi:http/proxy — start an HTTP server.
            http::serve(&bytes, &permissions, self.listen).await?;
        } else {
            // wasi:cli/command — run as a CLI program.
            let result =
                tokio::task::spawn_blocking(move || execute_cli_component(&bytes, &permissions))
                    .await
                    .into_diagnostic()
                    .wrap_err("runtime task panicked")??;

            // 6. Map exit.
            if let Err(()) = result {
                std::process::exit(1);
            }
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
    ) -> wasm_manifest::ResolvedPermissions {
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

        merged.resolve()
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

/// Confirm the bytes are a Wasm Component (not a core module or WIT-only package).
fn validate_component(bytes: &[u8]) -> miette::Result<()> {
    let parser = Parser::new(0);
    for payload in parser.parse_all(bytes) {
        match payload {
            Ok(Payload::Version { encoding, .. }) => {
                return match encoding {
                    Encoding::Component => Ok(()),
                    Encoding::Module => Err(RunError::CoreModule.into()),
                };
            }
            Err(e) => {
                return Err(RunError::InvalidBinary {
                    reason: e.to_string(),
                }
                .into());
            }
            _ => {}
        }
    }
    Err(RunError::NoVersionHeader.into())
}

/// Check whether a component exports `wasi:http/incoming-handler`, indicating
/// it targets the `wasi:http/proxy` world rather than `wasi:cli/command`.
///
/// Only top-level component exports are considered; nested component exports
/// are ignored by tracking nesting depth through `Version` / `End` payloads.
fn exports_http_incoming_handler(bytes: &[u8]) -> bool {
    let parser = Parser::new(0);
    let mut depth: u32 = 0;

    for payload in parser.parse_all(bytes) {
        let Ok(payload) = payload else { continue };
        match payload {
            Payload::Version { .. } => {
                depth += 1;
            }
            Payload::End(_) => {
                depth = depth.saturating_sub(1);
            }
            Payload::ComponentExportSection(reader) if depth == 1 => {
                for export in reader.into_iter().flatten() {
                    if export.name.0.starts_with("wasi:http/incoming-handler") {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// Build the Wasmtime runtime, instantiate the component, and invoke
/// `wasi:cli/run@0.2.0#run`.
fn execute_cli_component(
    bytes: &[u8],
    permissions: &wasm_manifest::ResolvedPermissions,
) -> miette::Result<Result<(), ()>> {
    let engine = Engine::default();
    let component = Component::new(&engine, bytes)
        .map_err(crate::util::into_miette)
        .wrap_err("failed to compile Wasm Component")?;

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
            .map_err(crate::util::into_miette)
            .wrap_err_with(|| format!("failed to pre-open directory: {}", dir.display()))?;
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
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(crate::util::into_miette)?;

    let command = Command::instantiate(&mut store, &component, &linker)
        .map_err(crate::util::into_miette)
        .wrap_err("failed to instantiate Wasm Component")?;

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

/// Resolve a `scope:component` manifest key to a vendored file path.
///
/// Reads the lockfile to find the matching component entry, then
/// reconstructs the vendor filename from registry, version, and digest.
/// Returns `None` if the input doesn't match any manifest entry.
fn resolve_manifest_key(input: &str) -> miette::Result<Option<PathBuf>> {
    let lockfile_path = PathBuf::from("deps/wasm.lock.toml");
    let manifest_path = PathBuf::from("deps/wasm.toml");

    let Ok(manifest_str) = std::fs::read_to_string(&manifest_path) else {
        return Ok(None);
    };
    let Ok(manifest) = toml::from_str::<wasm_manifest::Manifest>(&manifest_str) else {
        return Ok(None);
    };

    // Check if the input matches a manifest component key
    if !manifest.dependencies.components.contains_key(input) {
        return Ok(None);
    }

    let Ok(lockfile_str) = std::fs::read_to_string(&lockfile_path) else {
        return Ok(None);
    };
    let Ok(lockfile) = toml::from_str::<wasm_manifest::Lockfile>(&lockfile_str) else {
        return Ok(None);
    };

    // Find the matching lockfile entry
    let package = lockfile
        .components
        .iter()
        .find(|p| p.name == input)
        .ok_or_else(|| RunError::NotInLockfile {
            name: input.to_string(),
        })?;

    // Reconstruct the vendor filename from lockfile data.
    // The lockfile `registry` field is "host/repository" (e.g., "ghcr.io/user/repo").
    let (registry_host, repository) =
        package
            .registry
            .split_once('/')
            .ok_or_else(|| RunError::InvalidRegistryPath {
                path: package.registry.clone(),
                name: input.to_string(),
            })?;

    let filename = wasm_package_manager::manager::vendor_filename(
        registry_host,
        repository,
        Some(package.version.as_str()),
        &package.digest,
    );

    let vendored_path = PathBuf::from("deps/vendor/wasm").join(filename);
    if !vendored_path.exists() {
        return Err(RunError::VendoredFileMissing {
            path: vendored_path.display().to_string(),
            name: input.to_string(),
        }
        .into());
    }

    Ok(Some(vendored_path))
}

/// Fetch component bytes from an OCI registry.
async fn fetch_oci_bytes(
    oci_ref: &wasm_package_manager::Reference,
    offline: bool,
) -> miette::Result<Vec<u8>> {
    let manager = if offline {
        Manager::open_offline().await
    } else {
        Manager::open().await
    }
    .map_err(crate::util::into_miette)?;
    let pull_result = manager
        .pull(oci_ref.clone())
        .await
        .map_err(crate::util::into_miette)?;
    let manifest = pull_result.manifest.as_ref().ok_or(RunError::NoManifest)?;
    let wasm_layers = wasm_package_manager::oci::filter_wasm_layers(&manifest.layers);
    let layer = wasm_layers.first().ok_or(RunError::NoWasmLayer)?;
    let key = &layer.digest;
    manager
        .get(key)
        .await
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read cached component for {key}"))
}

/// Check whether `input` looks like a manifest key (`scope:component`).
///
/// Manifest keys use `scope:component` syntax (e.g. `wasi:http`, `test:hello`)
/// without dots or slashes, which distinguishes them from OCI references
/// (e.g. `ghcr.io/user/repo:tag`). WIT-style names never contain dots or
/// slashes, so rejecting those characters safely separates manifest keys
/// from OCI references.
fn looks_like_manifest_key(input: &str) -> bool {
    let Some((scope, component)) = input.split_once(':') else {
        return false;
    };
    !scope.is_empty() && !component.is_empty() && !input.contains('/') && !input.contains('.')
}

/// Build an error for a manifest-key input that is not installed locally.
///
/// Checks the global cache and the known-package index for a matching
/// component and returns the most actionable hint available.
async fn not_installed_error(input: &str) -> miette::Report {
    // Convert `scope:component` to `scope/component` to match the repository
    // path format used in the OCI cache and known-package index.
    let search_pattern = input.replace(':', "/");

    // Best-effort lookup: if the manager can't be opened (e.g. no cache
    // directory yet for first-time users), fall back to the generic hint.
    let hint = match Manager::open().await {
        Ok(manager) => build_hint_from_manager(&manager, input, &search_pattern),
        Err(_) => default_install_hint(input),
    };

    miette::miette!(
        help = hint,
        "component '{input}' is not installed in the local project"
    )
}

/// Inspect the manager's cache and known-package index and return a hint.
fn build_hint_from_manager(manager: &Manager, input: &str, search_pattern: &str) -> String {
    if is_in_cache(manager, search_pattern) {
        return format!(
            "a copy of the component is available from the local cache. \
             Call `wasm run -g {input}` to run it."
        );
    }
    if is_in_registry(manager, search_pattern) {
        return format!(
            "a component with the same name is available from the registry. \
             Call `wasm run -i {input}` to install it before running it."
        );
    }
    default_install_hint(input)
}

/// Check whether a component matching `pattern` exists in the local cache.
///
/// Matches when a cached image's repository equals the pattern or ends
/// with `/<pattern>` to avoid false positives from substring matching.
fn is_in_cache(manager: &Manager, pattern: &str) -> bool {
    let Ok(entries) = manager.list_all() else {
        return false;
    };
    let suffix = format!("/{pattern}");
    entries
        .iter()
        .any(|e| e.ref_repository == pattern || e.ref_repository.ends_with(&suffix))
}

/// Check whether a component matching `pattern` exists in the known-package index.
fn is_in_registry(manager: &Manager, pattern: &str) -> bool {
    let Ok(packages) = manager.search_packages(pattern, 0, 1) else {
        return false;
    };
    !packages.is_empty()
}

/// Fallback hint when neither cache nor registry has the component.
fn default_install_hint(input: &str) -> String {
    format!("run `wasm install {input}` to add it to the project before running it.")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal component that exports `wasi:http/incoming-handler@0.2.0`.
    fn build_http_component() -> Vec<u8> {
        use wasm_encoder::{ComponentExportKind, ComponentExportSection};
        let mut component = wasm_encoder::Component::new();

        let mut exports = ComponentExportSection::new();
        exports.export(
            "wasi:http/incoming-handler@0.2.0",
            ComponentExportKind::Instance,
            0,
            None,
        );
        component.section(&exports);

        component.finish()
    }

    #[test]
    fn detect_http_world_in_http_component() {
        let bytes = build_http_component();
        assert!(
            exports_http_incoming_handler(&bytes),
            "should detect wasi:http/incoming-handler export"
        );
    }

    #[test]
    fn detect_cli_world_in_minimal_component() {
        let bytes = include_bytes!("../../tests/fixtures/minimal_component.wasm");
        assert!(
            !exports_http_incoming_handler(bytes),
            "minimal component should not be detected as HTTP"
        );
    }

    #[test]
    fn detect_cli_world_in_core_module() {
        let bytes = include_bytes!("../../tests/fixtures/core_module.wasm");
        assert!(
            !exports_http_incoming_handler(bytes),
            "core module should not be detected as HTTP"
        );
    }
}
