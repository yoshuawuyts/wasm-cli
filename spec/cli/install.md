# Install Command

The `install` subcommand pulls and vendors WebAssembly packages.

r[cli.install.help]
The CLI MUST provide `--help` output for the `install` command.

r[install.no-manifest]
When `wasm install` is called and no local `wasm.toml` manifest exists,
the CLI MUST abort with an error indicating that `wasm.toml` was not found,
and provide hints to run `wasm init` or `wasm registry fetch <component>`.

r[install.wit-deps]
When installing a component, the CLI MUST extract its WIT dependencies
and recursively install each resolvable dependency into `deps/vendor/wit/`.

r[install.wit-deps.lockfile-only]
Transitive WIT dependencies MUST be recorded in `wasm.lock.toml`
`[[interfaces]]` entries. The manifest (`wasm.toml`) MUST NOT be modified
for transitive dependencies.

r[install.wit-deps.skip-offline]
Transitive WIT dependency resolution MUST be skipped in offline mode.
