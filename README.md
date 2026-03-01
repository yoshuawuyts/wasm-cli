<h1 align="center">wasm(1)</h1>
<div align="center">
  <strong>
    Unified developer tools for WebAssembly
  </strong>
</div>

<br />

<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/wasm">
    <img src="https://img.shields.io/crates/v/wasm.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/wasm">
    <img src="https://img.shields.io/crates/d/wasm.svg?style=flat-square"
      alt="Download" />
  </a>
  <!-- docs.rs docs -->
  <a href="https://docs.rs/wasm">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="docs.rs docs" />
  </a>
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/wasm">
      API Docs
    </a>
    <span> | </span>
    <a href="https://github.com/yoshuawuyts/wasm/releases">
      Releases
    </a>
    <span> | </span>
    <a href="https://github.com/yoshuawuyts/wasm/blob/master.github/CONTRIBUTING.md">
      Contributing
    </a>
  </h3>
</div>

> [!CAUTION]
> This repository is under active development and therefore unstable. Breaking
> changes are expected. Contributions and ideas however are still welcome!

## Installation

To install the `wasm` command and make it available from the command line, run:

```sh
$ cargo install wasm
```

To interface with the package manager backend programatically from Rust, you can
use the `wasm-package-manager` crate:

```rust
$ cargo add wasm-package-manager
```

## Using `wasm`

<!-- commands-start -->
```
Unified WebAssembly developer tools

Usage: wasm [OPTIONS] [COMMAND]

Commands:
  run       Execute a Wasm Component
  init      Create a new wasm component in an existing directory
  add       Add a dependency to the manifest without installing it
  install   Install a dependency from an OCI registry
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
```
<!-- commands-end -->

## Example

Let's use `wasm(1)` to fetch a Wasm Component locally and then run that. First
we have to setup a manifest and add a place for the downloaded components and
interfaces to go. To do that run:


```bash
# Create a new project
$ wasm init
```

This creates a new `deps/` directory in your project which contains a manifest,
lockfile, and a place for the downloaded artifacts to go. It's recommended to add
`deps/vendor/` to your `.gitignore` file:

```bash
.
└── deps/
    ├── vendor/         # A directory containing downloaded .wasm and .wit files
    ├── wasm.lock.toml  # A generated lockfile to guarantee reproducible builds
    └── wasm.toml       # A readable manifest to declare dependencies
```

Now that we have our basic project structure setup, let's fetch [a basic HTTP
Rust sample][ba-sample]. This component implements the `wasi:http` world and
exposes some basic testing endpoints. To get this we can run `wasm install`:

[ba-sample]: https://github.com/bytecodealliance/sample-wasi-http-rust

```bash
# Install the Bytecode Alliance WASI HTTP sample component
$ wasm install ghcr.io/bytecodealliance/sample-wasi-http-rust/sample-wasi-http-rust:0.1.6
   Installing ghcr.io/bytecodealliance/sample-wasi-http-rust/sample-wasi-http-rust:0.1.6
   └── [a1b2c] application/wasm ━━━━━━━━━━━━ 1.2 MiB

    Finished installation in 1.3s
```

This will have downloaded the `.wasm` component to `deps/vendor/`, and added it
to our manifest and lockfile. Our `deps/wasm.toml` file should now look like this:

```toml
[components]
"root:component" = "ghcr.io/bytecodealliance/sample-wasi-http-rust/sample-wasi-http-rust:0.1.6"
```

TODO: show how to run the component

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

[contributing]: https://github.com/yoshuawuyts/wasm/blob/master.github/CONTRIBUTING.md
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
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br/>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>
