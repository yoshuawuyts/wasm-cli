# wasm-meta-registry

An HTTP server that indexes OCI registries for WebAssembly package metadata and
exposes a search API.

## Overview

`wasm-meta-registry` takes a TOML config listing OCI repositories, periodically
syncs manifest and config metadata via `wasm-package-manager`, and serves search
results over HTTP. The `wasm` CLI can query this API for remote package
discovery — users then install packages from the actual OCI registries.

## Configuration

Create a `registries.toml` file:

```toml
# Sync interval in seconds (default: 3600)
sync_interval = 3600

# HTTP server bind address
bind = "0.0.0.0:8080"

[[packages]]
registry = "ghcr.io"
repository = "bytecodealliance/sample-wasi-http-rust/sample-wasi-http-rust"

[[packages]]
registry = "ghcr.io"
repository = "webassembly/wasi/clocks"
```

## Usage

```sh
wasm-meta-registry registries.toml
```

## API Endpoints

- `GET /v1/health` — Health check
- `GET /v1/search?q={query}&offset={n}&limit={n}` — Search packages
- `GET /v1/packages?offset={n}&limit={n}` — List all packages
- `GET /v1/packages/{registry}/{repository}` — Get a specific package

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
