# Architecture

This document describes the high-level architecture of `wasm(1)`. If you want to
familiarize yourself with the codebase, you are in the right place.

## Overview

`wasm(1)` is a unified developer tool for WebAssembly. It can pull and install
Wasm Components and WIT interfaces from OCI registries, run components via
Wasmtime with sandboxed permissions, and manage local state through both a CLI
and an interactive TUI.

The project is a Cargo workspace with six crates:

```
crates/
├── wasm-cli              # Binary — the `wasm(1)` command
├── wasm-package-manager  # Library — OCI registry interaction, caching, metadata
├── wasm-manifest         # Library — manifest and lockfile types
├── wasm-detector         # Library — local .wasm file discovery
├── wasm-meta-registry    # Binary + library — HTTP metadata server for package search
└── xtask                 # Internal — build automation (fmt, clippy, test, SQL migrations)
```

## Crate Dependency Graph

```
wasm-cli ──────────┬──► wasm-package-manager ──► wasm-manifest
                   │
                   ├──► wasm-manifest
                   │
                   └──► wasm-detector

wasm-meta-registry ───► wasm-package-manager ──► wasm-manifest
```

`wasm-cli` is the main entry point. It depends on `wasm-package-manager` for all
registry and storage operations, on `wasm-manifest` for reading project manifests
and lockfiles, and on `wasm-detector` for finding local `.wasm` files.

`wasm-meta-registry` is an independent server binary that also uses
`wasm-package-manager` to index OCI registries and expose a search API.

`xtask` is a development-only crate and is not depended on by any other crate.

## wasm-cli

The `wasm(1)` binary lives in `crates/wasm-cli`. It uses [clap] for argument
parsing and dispatches to one of the following command modules:

| Command      | Module          | Purpose |
|------------- |---------------- |-------- |
| `run`        | `run/`          | Execute a Wasm Component via [wasmtime] with WASI sandboxing |
| `init`       | `init/`         | Scaffold a `deps/` directory with manifest, lockfile, and vendor dirs |
| `add`        | `add/`          | Add a dependency to the manifest without pulling layers |
| `install`    | `install/`      | Pull packages and vendor them into `deps/vendor/` |
| `compose`    | `compose/`      | Compose Wasm components from WAC scripts |
| `local`      | `local/`        | Detect `.wasm` files in the current project |
| `registry`   | `registry/`     | Manage cached packages (pull, tags, search, sync, delete, list, known, inspect) |
| `self`       | `self_/`        | Tool configuration, completions, man pages, state, logs, clean |
| *(none)*     | `tui/`          | Launch the interactive terminal UI when stdin is a terminal |

[clap]: https://docs.rs/clap
[wasmtime]: https://docs.rs/wasmtime

### TUI

The interactive UI is built with [ratatui] and lives in `crates/wasm-cli/src/tui/`.

```
tui/
├── mod.rs          # Entry point, bidirectional channel setup
├── app.rs          # App state, key handling, tab routing
├── components/     # Reusable widgets (tab bar, etc.)
└── views/          # One view per tab (Local, Components, Interfaces, Search, Settings, Log)
```

The TUI and the package manager communicate through two `tokio::sync::mpsc`
channels:

- **`AppEvent`** — sent from the UI thread to the async manager (e.g. Pull,
  Delete, SearchPackages, RequestPackages).
- **`ManagerEvent`** — sent from the manager back to the UI (e.g.
  PackagesList, PullResult, StateInfo).

The UI runs on a **blocking thread** (`spawn_blocking`) because ratatui's event
loop is synchronous. The `Manager` runs on a `tokio::task::LocalSet` because it
is `!Send`.

[ratatui]: https://docs.rs/ratatui

### Run Command and Permissions

`wasm run` executes a Wasm Component using Wasmtime's WASIp2 implementation.
Permissions are resolved through a four-layer merge:

1. **Global config** — `$XDG_CONFIG_HOME/wasm/config.toml` defaults
2. **Global components** — `$XDG_CONFIG_HOME/wasm/components.toml` per-component overrides
3. **Project manifest** — `deps/wasm.toml` per-component permissions
4. **CLI flags** — command-line overrides (highest precedence)

The `RunPermissions` type is defined in `wasm-manifest` and controls environment
variables, directory access, stdio inheritance, and network access.

## wasm-package-manager

The core library lives in `crates/wasm-package-manager`. It handles all
interaction with OCI registries, local caching, and metadata extraction.

```
src/
├── lib.rs              # Public API re-exports, format_size()
├── config.rs           # Config loading (global + local merge), credential helpers
├── credential_helper.rs
├── progress.rs         # ProgressEvent enum for pull progress reporting
├── manager/
│   ├── mod.rs          # Manager — high-level API (pull, install, delete, search, sync)
│   └── logic.rs        # Pure functions (vendor_filename, should_sync, derive_component_name, etc.)
├── oci/
│   ├── client.rs       # OCI registry client (wraps oci-wasm + oci-client)
│   ├── models.rs       # OCI data types
│   ├── raw.rs          # RawImageEntry — internal image metadata with DB IDs
│   ├── image_entry.rs  # ImageEntry — public query result type
│   └── logic.rs        # Pure functions (filter_wasm_layers, classify_tag, compute_orphaned_layers)
├── types/
│   ├── detect.rs       # WIT package detection (is_wit_package)
│   ├── parser.rs       # WIT text parsing and metadata extraction
│   ├── raw.rs          # RawWitPackage — internal type with DB IDs
│   ├── wit_package.rs  # WitPackage — public query result type
│   └── worlds.rs       # World-level analysis
├── components/
│   └── models.rs       # Component data types
├── storage/
│   ├── mod.rs          # Store facade
│   ├── store.rs        # SQLite operations + cacache layer caching
│   ├── config.rs       # StateInfo (cache dirs, database path, log dir)
│   ├── models/         # RawKnownPackage, Migrations
│   ├── known_package.rs # KnownPackage — public query result type
│   ├── schema.sql      # Canonical database schema (source of truth)
│   └── migrations/     # Auto-generated SQL migration files
└── network/            # Network utilities (RegistryClient)
```

### Manager

`Manager` is the main entry point. It composes a `Client` (OCI), a `Store`
(SQLite + cacache), and a `Config`. Key operations:

- **`pull`** / **`pull_with_progress`** — fetch an OCI image, store layers in
  cacache, record metadata in SQLite, and extract WIT interface information.
- **`install`** / **`install_with_progress`** — pull then hard-link (vendor)
  layers into a project-local directory.
- **`delete`** — remove a cached package and its orphaned layers.
- **`search_packages`** / **`list_known_packages`** — query the local metadata
  database.
- **`sync_from_meta_registry`** — update the local package index from a
  meta-registry server.

### Storage

Storage is split into two systems:

- **SQLite** (`wasm.db`) — stores all structured metadata (OCI manifests, tags,
  WIT interfaces, worlds, components). Managed via `rusqlite` with
  forward-only migrations.
- **cacache** — content-addressable blob store for OCI image layers.
  Deduplicates identical layers across packages. Vendoring uses hard links so
  disk usage is shared with the cache.

### Database Schema

The SQLite schema (`schema.sql`) follows a three-layer design:

1. **OCI layer** — `oci_repository`, `oci_manifest`, `oci_tag`, `oci_layer`,
   `oci_referrer`, plus annotation tables. Models the OCI distribution spec.
2. **WIT layer** — `wit_interface`, `wit_world`, `wit_world_import`,
   `wit_world_export`, `wit_interface_dependency`. Models the WebAssembly
   Interface Type system. Foreign keys link imports/exports/dependencies to
   resolved interfaces (best-effort — NULL if the dependency is not yet
   cached).
3. **Wasm layer** — `wasm_component`, `component_target`. Links compiled
   components to the worlds they target.

To change the schema, edit `schema.sql` and run
`cargo xtask sql migrate --name <description>`. Never hand-write migration files.

## wasm-manifest

A small serialization library in `crates/wasm-manifest`. It defines the types
for reading and writing project manifests (`deps/wasm.toml`) and lockfiles
(`deps/wasm.lock.toml`).

Key types:

- **`Manifest`** — has `components` and `interfaces` maps of `String → Dependency`.
- **`Dependency`** — either a compact string (`"ghcr.io/org/pkg:1.0"`) or an
  explicit table with `registry`, `namespace`, `package`, `version`, and
  optional `permissions`.
- **`Lockfile`** — lists resolved packages with digests for reproducible builds.
- **`RunPermissions`** / **`ResolvedPermissions`** — sandbox controls for the
  `wasm run` command.

## wasm-detector

A small library in `crates/wasm-detector` that finds `.wasm` files in a
directory tree. It uses the [ignore] crate to respect `.gitignore` rules and
also scans well-known directories (`target/wasm32-*`, `pkg/`, `dist/`) that
may be git-ignored.

[ignore]: https://docs.rs/ignore

## wasm-meta-registry

An HTTP server in `crates/wasm-meta-registry` that indexes OCI registries and
exposes a search API. It consists of:

- **`config.rs`** — per-namespace TOML registry file parsing and configuration.
- **`indexer.rs`** — background thread that periodically syncs package metadata
  using `wasm-package-manager::Manager`.
- **`server.rs`** — [axum] HTTP router with search endpoints.

[axum]: https://docs.rs/axum

## xtask

Internal build automation in `crates/xtask`. The command `cargo xtask test` runs
the full CI suite:

1. `cargo fmt` — formatting check
2. `cargo clippy` — lint check (with `-D warnings`)
3. `cargo test` — test suite
4. `cargo xtask sql check` — verify migrations are in sync with `schema.sql`
5. README freshness check — ensures `README.md` matches `wasm --help` output

SQL migrations are managed through `cargo xtask sql migrate` and
`cargo xtask sql install` (installs `sqlite3def`).

## Project-Level Conventions

- **100% safe Rust** — `#![forbid(unsafe_code)]` is set workspace-wide.
- **Edition 2024** — all crates use the latest Rust edition.
- **Dual license** — MIT OR Apache-2.0.
- **XDG directories** — configuration, data, and state follow the XDG Base
  Directory specification.
- **`#[must_use]`** — applied to public functions that return values.
- **Public types** — public API types (`ImageEntry`, `KnownPackage`,
  `WitPackage`) omit database IDs and are separate from internal `Raw*` model
  types.
- **Pure logic** — side-effect-free functions are grouped in `logic.rs` files
  for easy testing.
