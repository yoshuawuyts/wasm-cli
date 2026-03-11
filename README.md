<h1 align="center">wasm-cli</h1>
<div align="center">
  <strong>
    Unified developer tools for WebAssembly
  </strong>
</div>

## Introduction

`wasm-cli` is a _package manager_ for WebAssembly Components and WebAssembly
Interface Types (WIT). It can search, fetch, and publish WebAssembly to registries
like GitHub Packages and Docker Hub. It also automatically resolves dependencies, tracks releases, and generates lockfiles to ensure deterministic builds.

`wasm-cli` is intended to be used together with language-specific wasm
toolchains. The idea is that you can use a language-specific compiler to compile
source code to `.wasm` binaries. And then use `wasm-cli` to handle everything
else, including: executing, publishing, linking, and debugging.

`wasm-cli` can either be used directly from the command line, or embedded into
other applications via the `wasm-package-manager` Rust crate. This makes it
possible for other Wasm tools to search and install Wasm Components without ever
leaving the application. Coupled with the "search-by-interface" functionality, this makes it possible to filter the search down only compatible components.

> [!CAUTION]
> This repository is under active development and therefore unstable. Breaking
> changes are expected. Contributions and ideas however are still welcome!

## Quick start

```bash
$ wasm init                              # Create a `wasm.toml` and `wasm.toml.lock` locally
$ wasm install ba:sample-wasi-http-rust  # Install a `wasi:http` server as a dependency
$ wasm run ba:sample-wasi-http-rust      # Run the `wasi:http` server
$ curl localhost:8080                    # Send a request to the `wasi:http` server
```

## Usage

<!-- commands-start -->
```
Unified WebAssembly developer tools

Usage: wasm [OPTIONS] [COMMAND]

Commands:
  run       Execute a Wasm Component
  init      Create a new wasm component in an existing directory
  install   Install a dependency from an OCI registry
  compose   Compose Wasm components from WAC scripts
  local     Detect and manage local WASM files
  registry  Manage Wasm Components and WIT interfaces in OCI registries
  self      Configure the `wasm(1)` tool, generate completions, & manage state
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

Global Options:
      --color <WHEN>  When to use colored output [default: auto] [possible values: auto, always, never]
      --offline       Run in offline mode
  -v, --verbose...    Increase logging verbosity
  -q, --quiet...      Decrease logging verbosity
```
<!-- commands-end -->

## Installation

### Bash (Linux / macOS)

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/yoshuawuyts/wasm-cli/releases/latest/download/install.sh | sh
```

### PowerShell (Windows)

```powershell
irm https://github.com/yoshuawuyts/wasm-cli/releases/latest/download/install.ps1 | iex
```

### Cargo (Rust)

```sh
cargo install wasm
```


## Crates

This project is composed of several crates:

| Crate                                                 | Description                                                                                          |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| [`wasm`](crates/wasm-cli)                             | The `wasm(1)` command-line interface providing unified WebAssembly developer tools                   |
| [`wasm-package-manager`](crates/wasm-package-manager) | A stateful library to interact with OCI registries storing WebAssembly Components                    |
| [`wasm-detector`](crates/wasm-detector)               | A library to detect local `.wasm` files in a repository                                              |
| [`wasm-manifest`](crates/wasm-manifest)               | Manifest and lockfile format types for WebAssembly packages                                          |
| [`wasm-meta-registry`](crates/wasm-meta-registry)     | An HTTP server that indexes OCI registries for WebAssembly package metadata and exposes a search API |
| [`xtask`](crates/xtask)                               | Internal development automation tasks (formatting, linting, testing, migrations)                     |

## Contributing
Want to join us? Check out our ["Contributing" guide][contributing] and take a
look at some of these issues:

- [Issues labeled "good first issue"][good-first-issue]
- [Issues labeled "help wanted"][help-wanted]

[contributing]: CONTRIBUTING.md
[good-first-issue]: https://github.com/yoshuawuyts/wasm/labels/good%20first%20issue
[help-wanted]: https://github.com/yoshuawuyts/wasm/labels/help%20wanted

## Safety
This crate uses ``#![forbid(unsafe_code)]`` to ensure everything is implemented in
100% Safe Rust.

## Notes on AI
This project is developed with GitHub Copilot. We believe language models can be 
valuable tools for coding when paired with human oversight, testing, and 
careful review. For transparency, we mention this in the README.

## License

<sup>
Licensed under <a href="LICENSE">Apache License, Version
2.0</a>, with <a href="LICENSE">LLVM Exceptions</a>.
</sup>

<br/>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be licensed as above, without any additional terms or conditions.
</sub>
