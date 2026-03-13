# Local Command

The `local` subcommand manages locally available WebAssembly files.

r[cli.local.help]
The CLI MUST provide `--help` output for the `local` command.

r[cli.local-list.help]
The CLI MUST provide `--help` output for the `local list` command.

r[cli.local-clean.help]
The CLI MUST provide `--help` output for the `local clean` command.

r[cli.local-clean.removes-lockfile]
`wasm local clean` MUST remove the lockfile (`wasm.lock.toml`).

r[cli.local-clean.removes-vendor-wasm]
`wasm local clean` MUST remove the contents of `vendor/wasm`.

r[cli.local-clean.removes-vendor-wit]
`wasm local clean` MUST remove the contents of `vendor/wit`.

## Offline Mode

r[cli.offline.local-allowed]
When `--offline` is set, local operations MUST still succeed.
