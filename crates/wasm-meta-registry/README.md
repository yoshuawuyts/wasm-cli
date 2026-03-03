# wasm-meta-registry

An HTTP server that indexes OCI registries for WebAssembly package metadata and
exposes a search API.

## Overview

`wasm-meta-registry` reads a directory of per-namespace TOML registry files,
periodically syncs manifest and config metadata via `wasm-package-manager`, and
serves search results over HTTP. The `wasm` CLI can query this API for remote
package discovery — users then install packages from the actual OCI registries.

## Registry format

Create a `registry/` directory with one TOML file per WIT namespace:

```
registry/
  ba.toml
  wasi.toml
  microsoft.toml
```

Each file defines a `[namespace]` table and zero or more `[[component]]` and
`[[interface]]` entries:

```toml
# wasi.toml
[namespace]
name = "wasi"
registry = "ghcr.io/webassembly"

[[interface]]
name = "io"
repository = "wasi/io"

[[interface]]
name = "clocks"
repository = "wasi/clocks"
```

```toml
# ba.toml
[namespace]
name = "ba"
registry = "ghcr.io/bytecodealliance"

[[component]]
name = "sample-wasi-http-rust"
repository = "sample-wasi-http-rust/sample-wasi-http-rust"
```

- **`[namespace]`** — maps a WIT namespace to an OCI registry base path
- **`[[component]]`** — a runnable Wasm component
- **`[[interface]]`** — a WIT interface type package
- **`name`** — the package name under the namespace (e.g., `wasi:io`)
- **`repository`** — the OCI repository path, relative to the namespace's `registry`

The filename (without `.toml`) must match the `namespace.name` field inside.

## Usage

```sh
wasm-meta-registry registry/ --sync-interval 3600 --bind 0.0.0.0:8080
```

## API Endpoints

- `GET /v1/health` — Health check
- `GET /v1/search?q={query}&offset={n}&limit={n}` — Search packages
- `GET /v1/packages?offset={n}&limit={n}` — List all packages
- `GET /v1/packages/{registry}/{repository}` — Get a specific package

## License

Licensed under Apache License, Version 2.0, with LLVM Exceptions.
