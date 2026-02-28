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

## Installation
```sh
$ cargo add wasm
```

## Using `wasm`

<!-- commands-start -->
```
Unified WebAssembly developer tools

Usage: wasm [OPTIONS] [COMMAND]

Commands:
  run       Execute a Wasm Component
  init      Create a new wasm component in an existing directory
  install   Install a dependency from an OCI registry
  inspect   Inspect a Wasm Component
  convert   Convert a Wasm Component to another format
  local     Detect and manage local WASM files
  registry  Manage Wasm Components and WIT interfaces in OCI registries
  compose   Compose Wasm Components with other components
  self      Configure the `wasm(1)` tool, generate completions, & manage state
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

Global Options:
      --color <WHEN>
          When to use colored output.

          Can also be controlled via environment variables: - NO_COLOR=1 (disables color) - CLICOLOR=0 (disables color) - CLICOLOR_FORCE=1 (forces color)

          [default: auto]
          [possible values: auto, always, never]

      --offline
          Run in offline mode.

          Disables all network operations. Commands that require network access will fail with an error. Local-only commands will continue to work.
```
<!-- commands-end -->

## Example: Installing a Package

Initialize a new project and install the [Bytecode Alliance WASI HTTP sample][ba-sample]:

```bash
# Create a new project
$ wasm init

# Install the Bytecode Alliance WASI HTTP sample component
$ wasm install ghcr.io/bytecodealliance/sample-wasi-http-rust/sample-wasi-http-rust:0.1.6
   Installing ghcr.io/bytecodealliance/sample-wasi-http-rust/sample-wasi-http-rust:0.1.6
   └── [a1b2c] application/wasm ━━━━━━━━━━━━ 1.2 MiB

    Finished installation in 1.3s
```

The component is then available in your project under `deps/vendor/wasm/`:

```
deps/
└── vendor/
    └── wasm/
        └── sample-wasi-http-rust.wasm
```

[ba-sample]: https://github.com/bytecodealliance/sample-wasi-http-rust

## Storage Layout

```
~/.local/share/wasm/
├── store/          # content-addressable blob storage (image layers)
└── db/
    └── metadata.db3    # sqlite database (package metadata & references)
```

## Status

Experimental. Early development stage — expect breaking changes. 
Contributions and feedback welcome!

## Notes on AI

This project is developed with GitHub Copilot. We believe language models can be 
valuable tools for coding when paired with human oversight, testing, and 
careful review. For transparency, we mention this in the README.

## Safety
This crate uses ``#![deny(unsafe_code)]`` to ensure everything is implemented in
100% Safe Rust.

## Contributing
Want to join us? Check out our ["Contributing" guide][contributing] and take a
look at some of these issues:

- [Issues labeled "good first issue"][good-first-issue]
- [Issues labeled "help wanted"][help-wanted]

[contributing]: https://github.com/yoshuawuyts/wasm/blob/master.github/CONTRIBUTING.md
[good-first-issue]: https://github.com/yoshuawuyts/wasm/labels/good%20first%20issue
[help-wanted]: https://github.com/yoshuawuyts/wasm/labels/help%20wanted

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
