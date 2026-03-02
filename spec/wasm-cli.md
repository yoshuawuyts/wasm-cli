# wasm-cli Specification

This document defines the requirements for the `wasm(1)` CLI binary.
Requirements are derived from the existing test suite.

## CLI

The `wasm` command-line interface provides subcommands for managing WebAssembly
components and interfaces.

### Help and Version

r[cli.help.main]
The CLI MUST provide `--help` output for the top-level command.

r[cli.help.local]
The CLI MUST provide `--help` output for the `local` command.

r[cli.help.local-list]
The CLI MUST provide `--help` output for the `local list` command.

r[cli.help.registry]
The CLI MUST provide `--help` output for the `registry` command.

r[cli.help.registry-pull]
The CLI MUST provide `--help` output for the `registry pull` command.

r[cli.help.registry-tags]
The CLI MUST provide `--help` output for the `registry tags` command.

r[cli.help.registry-search]
The CLI MUST provide `--help` output for the `registry search` command.

r[cli.help.registry-sync]
The CLI MUST provide `--help` output for the `registry sync` command.

r[cli.help.registry-delete]
The CLI MUST provide `--help` output for the `registry delete` command.

r[cli.help.registry-list]
The CLI MUST provide `--help` output for the `registry list` command.

r[cli.help.registry-known]
The CLI MUST provide `--help` output for the `registry known` command.

r[cli.help.registry-inspect]
The CLI MUST provide `--help` output for the `registry inspect` command.

r[cli.help.self]
The CLI MUST provide `--help` output for the `self` command.

r[cli.help.self-clean]
The CLI MUST provide `--help` output for the `self clean` command.

r[cli.help.self-state]
The CLI MUST provide `--help` output for the `self state` command.

r[cli.help.self-log]
The CLI MUST provide `--help` output for the `self log` command.

r[cli.help.init]
The CLI MUST provide `--help` output for the `init` command.

r[cli.help.install]
The CLI MUST provide `--help` output for the `install` command.

r[cli.help.run]
The CLI MUST provide `--help` output for the `run` command.

r[cli.version]
The CLI MUST print a version string containing the program name when invoked
with `--version`.

### Color Support

r[cli.color.auto]
The CLI MUST accept `--color auto`.

r[cli.color.always]
The CLI MUST accept `--color always`.

r[cli.color.never]
The CLI MUST accept `--color never`.

r[cli.color.invalid]
The CLI MUST reject invalid `--color` values with an error message.

r[cli.color.in-help]
The `--color` flag MUST appear in `--help` output.

r[cli.color.no-color-env]
The CLI MUST respect the `NO_COLOR` environment variable.

r[cli.color.clicolor-env]
The CLI MUST respect the `CLICOLOR` environment variable.

r[cli.color.subcommand]
The `--color` flag MUST work when combined with subcommands.

### Offline Mode

r[cli.offline.accepted]
The CLI MUST accept an `--offline` flag.

r[cli.offline.in-help]
The `--offline` flag MUST appear in `--help` output.

r[cli.offline.registry-blocked]
When `--offline` is set, registry operations MUST fail with a clear error
mentioning offline mode.

r[cli.offline.local-allowed]
When `--offline` is set, local operations MUST still succeed.

r[cli.offline.with-inspect]
The `--offline` flag MUST be accepted alongside the `registry inspect` command.

r[cli.offline.with-subcommand]
The `--offline` flag MUST be accepted alongside any subcommand.

### Shell Completions

r[cli.completions.bash]
The CLI MUST generate valid Bash completions.

r[cli.completions.zsh]
The CLI MUST generate valid Zsh completions.

r[cli.completions.fish]
The CLI MUST generate valid Fish completions.

r[cli.completions.invalid]
The CLI MUST reject invalid shell names for completions.

### Man Pages

r[cli.man-pages]
The CLI MUST generate man pages that reference the program name.

## Init Command

The `init` subcommand scaffolds a new project directory.

r[init.current-dir]
Running `wasm init` without arguments MUST create the directory structure,
manifest, and lockfile in the current directory.

r[init.explicit-path]
Running `wasm init <path>` MUST create the directory structure and files at
the specified path.

## Install Command

The `install` subcommand pulls and vendors WebAssembly packages.

r[install.wit-deps]
When installing a component, the CLI MUST extract its WIT dependencies
and recursively install each resolvable dependency into `deps/vendor/wit/`.

r[install.wit-deps.lockfile-only]
Transitive WIT dependencies MUST be recorded in `wasm.lock.toml`
`[[types]]` entries. The manifest (`wasm.toml`) MUST NOT be modified
for transitive dependencies.

r[install.wit-deps.skip-offline]
Transitive WIT dependency resolution MUST be skipped in offline mode.

## Run Command

The `run` subcommand executes a WebAssembly component.

r[run.core-module-rejected]
The run command MUST reject core WebAssembly modules with a clear error message.

r[run.missing-file]
The run command MUST report a clear error when the target file does not exist.

## Dotenv

The CLI supports loading environment variables from `.env` files.

r[dotenv.detection]
The CLI MUST detect and report the presence of a `.env` file in `self config`
output, including the number of variables defined.

r[dotenv.not-found]
When no `.env` file exists, the CLI MUST report it as `not found`.

r[dotenv.loading]
The CLI MUST load variables from a `.env` file successfully.

r[dotenv.precedence]
System environment variables MUST take precedence over `.env` file variables.

## TUI

The terminal user interface renders views using `ratatui`.

### Local View

r[tui.local-view.empty]
The local view MUST render an empty state when no files are present.

r[tui.local-view.populated]
The local view MUST render a list of discovered WASM files.

### Interfaces View

r[tui.types-view.empty]
The interfaces view MUST render an empty state.

r[tui.types-view.populated]
The interfaces view MUST render a populated list of WIT interfaces.

### Packages View

r[tui.packages-view.empty]
The packages view MUST render an empty state.

r[tui.packages-view.populated]
The packages view MUST render a populated list of packages.

r[tui.packages-view.filter-active]
The packages view MUST render a filter input when filtering is active.

r[tui.packages-view.filter-results]
The packages view MUST render filtered results.

### Package Detail View

r[tui.package-detail-view.full]
The package detail view MUST render full package metadata.

r[tui.package-detail-view.no-tag]
The package detail view MUST handle missing tags gracefully.

### Search View

r[tui.search-view.empty]
The search view MUST render an empty state.

r[tui.search-view.populated]
The search view MUST render a populated list of packages.

r[tui.search-view.active]
The search view MUST render a search input when search is active.

r[tui.search-view.many-tags]
The search view MUST render packages with many tags.

### Known Package Detail View

r[tui.known-package-detail-view.full]
The known package detail view MUST render full metadata.

r[tui.known-package-detail-view.minimal]
The known package detail view MUST render minimal metadata.

### Settings View

r[tui.settings-view.loading]
The settings view MUST render a loading state.

r[tui.settings-view.populated]
The settings view MUST render a populated state with system information.

### Log View

r[tui.log-view.empty]
The log view MUST render an empty state.

r[tui.log-view.populated]
The log view MUST render log lines.

r[tui.log-view.scrolled]
The log view MUST support scrolling through log lines.

### Tab Bar

r[tui.tab-bar.first-selected]
The tab bar MUST render with the first tab selected.

r[tui.tab-bar.second-selected]
The tab bar MUST render with the second tab selected.

r[tui.tab-bar.third-selected]
The tab bar MUST render with the third tab selected.

r[tui.tab-bar.loading]
The tab bar MUST render a loading status message.

r[tui.tab-bar.error]
The tab bar MUST render an error status message.
