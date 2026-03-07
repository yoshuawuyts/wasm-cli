# wasm-manifest

Manifest and lockfile format types for WebAssembly packages.

This crate provides types for parsing and serializing WASM package manifests (`wasm.toml`) 
and lockfiles (`wasm.lock`).

## Manifest Format

The manifest file (`wasm.toml`) supports two dependency formats:

### Compact format
```toml
[dependencies]
"wasi:logging" = "ghcr.io/webassembly/wasi-logging:1.0.0"
"wasi:key-value" = "ghcr.io/webassembly/wasi-key-value:2.0.0"
```

### Explicit format
```toml
[dependencies."wasi:logging"]
registry = "ghcr.io"
namespace = "webassembly"
package = "wasi-logging"
version = "1.0.0"
```

## Lockfile Format

The lockfile (`wasm.lock.toml`) tracks resolved dependencies:

```toml
version = 1

[[package]]
name = "wasi:logging"
version = "1.0.0"
registry = "ghcr.io/webassembly/wasi-logging"
digest = "sha256:a1b2c3d4..."

[[package.dependencies]]
name = "wasi:logging"
version = "1.0.0"
registry = "ghcr.io/webassembly/wasi-logging"
digest = "sha256:a1b2c3d4..."
```
