# Run Command

The `run` subcommand executes a WebAssembly component.

r[cli.run.help]
The CLI MUST provide `--help` output for the `run` command.

r[run.core-module-rejected]
The run command MUST reject core WebAssembly modules with a clear error message.

r[run.missing-file]
The run command MUST report a clear error when the target file does not exist.

r[run.not-installed]
When the input looks like a manifest key (`scope:component` syntax) but is not
listed in `[components]` in `wasm.toml`, the run command MUST abort with a
user-friendly error.

r[run.not-installed.hint-cache]
If a copy of the component is available in the local cache, the error MUST
suggest using the `--global/-g` flag.

r[run.not-installed.hint-registry]
If the component is not cached but is found in the package index, the error
MUST suggest using the `--install/-i` flag.

r[run.oci-layer-lookup]
When running an OCI reference, the run command MUST retrieve the component
bytes using the `application/wasm` layer digest from the pulled manifest, not
the OCI reference string.

## HTTP world support

r[run.http-world-detection]
The run command MUST auto-detect whether a component targets the
`wasi:http/proxy` world by checking for a `wasi:http/incoming-handler` export.

r[run.http-server]
When a component targets the `wasi:http/proxy` world, the run command MUST
start a local HTTP server that proxies incoming requests to the component.

r[run.http-listen-flag]
The `--listen` flag MUST allow the user to configure the HTTP server bind
address. The default bind address MUST be `127.0.0.1:8080`.

r[run.http-listen-message]
When the HTTP server starts, the run command MUST print the listening address
to stderr.
