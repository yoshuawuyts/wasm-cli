# Usage Guide

This guide covers basic usage patterns for `wasm(1)`, a unified developer tool for WebAssembly.

## Installation

### From crates.io

```bash
cargo install wasm
```

### As a Library

```bash
cargo add wasm
```

## Command Overview

`wasm` provides several command categories:

- **Interactive TUI**: Launch with no arguments
- **Package Management**: Pull, push, and list packages
- **Local Discovery**: Detect and manage local Wasm files
- **Inspection**: Examine Wasm component structure
- **Self Management**: Configure and manage the tool itself

## Interactive Mode

Launch the interactive terminal user interface:

```bash
wasm
```

The TUI provides:
- Package browsing and search
- Interactive package details
- Keyboard navigation
- Visual package management

**Note**: The TUI only launches when running in an interactive terminal (not when piped or redirected).

## Package Management

### Pulling Packages

Download a package from a registry:

```bash
# Pull from GitHub Container Registry
wasm package pull ghcr.io/example/my-component:latest

# Pull from Docker Hub
wasm package pull myuser/my-component:v1.0.0

# Pull from a custom registry
wasm package pull registry.example.com/org/component:tag
```

The package is stored locally in content-addressable storage and can be listed with `wasm package list`.

### Pushing Packages

Push a local package to a registry:

```bash
wasm package push ghcr.io/myuser/my-component:v1.0.0
```

**Note**: You must be authenticated to push packages. See [Authentication](authentication.md) for details.

### Listing Packages

View all locally stored packages:

```bash
wasm package list
```

This shows:
- Registry and repository
- Tags
- Digests
- Pull timestamps
- Storage size

## Local Wasm File Discovery

### Listing Local Files

Detect Wasm files in the current directory:

```bash
wasm local list
```

This recursively scans for `.wasm` files and displays:
- File paths
- File sizes
- Component type (if applicable)

The detector respects `.gitignore` rules and standard ignore patterns.

## Inspecting Wasm Components

### Basic Inspection

Examine a Wasm component file:

```bash
wasm inspect file.wasm
```

This displays:
- Component structure
- Imports and exports
- Metadata
- Dependencies

### Detailed Information

For more detailed information, the inspect command shows:
- Component type information
- Interface definitions
- World descriptions
- Custom sections

## Self Management

### Viewing State

Check storage location and usage:

```bash
wasm self state
```

Displays:
- Executable location
- Data directory paths
- Storage sizes
- Migration status

### Cleaning Storage

Clean up unused content and optimize storage:

```bash
wasm self clean
```

This operation:
- Removes orphaned content
- Vacuums the database
- Reclaims disk space

## Common Workflows

### Exploring a Registry

1. Search for packages (coming soon) or use the TUI
2. Pull interesting packages to inspect them
3. Examine with `wasm inspect` or the TUI

### Publishing a Package

1. Build your Wasm component
2. Authenticate with your registry (see [Authentication](authentication.md))
3. Push with `wasm package push registry.example.com/myorg/component:v1.0.0`

### Managing Local Development

1. Use `wasm local list` to discover Wasm files in your project
2. Inspect components with `wasm inspect`
3. Test components locally before publishing

### Cleaning Up After Development

1. Run `wasm self state` to check storage usage
2. Remove unused packages manually or with future commands
3. Run `wasm self clean` to reclaim space

## Package Reference Format

Packages are referenced using OCI-style references:

```
[registry/]repository[:tag|@digest]
```

### Examples

```bash
# Full reference with registry and tag
ghcr.io/owner/repo:latest

# With digest instead of tag
ghcr.io/owner/repo@sha256:abcd1234...

# Custom registry with port (untested)
localhost:5000/myrepo:dev
```

### Registry Resolution

- Common registries: `ghcr.io`, `docker.io`, `mcr.microsoft.com`, `quay.io`
- Private registries require full domain specification

## Command-Line Help

Each command and subcommand has built-in help:

```bash
# Top-level help
wasm --help

# Subcommand help
wasm package --help
wasm package pull --help

# Self commands
wasm self --help
```

## Tips and Tricks

### Shell Completions

Generate shell completions for your preferred shell (user-local paths shown):

```bash
# Bash
wasm self completions bash > ~/.local/share/bash-completion/completions/wasm

# Zsh
wasm self completions zsh > ~/.zfunc/_wasm

# Fish
wasm self completions fish > ~/.config/fish/completions/wasm.fish
```

### Man Pages

Generate man pages for offline documentation. A user-local path is shown below;
for system-wide installation, use `sudo` and `/usr/local/share/man/man1/wasm.1`.

```bash
mkdir -p ~/.local/share/man/man1
wasm self man-pages > ~/.local/share/man/man1/wasm.1
man wasm
```

### Color Support

The CLI supports colored output via the `--color` flag:

```bash
wasm --color auto ...     # automatic color (default)
wasm --color always ...   # always use color
wasm --color never ...    # never use color
```

Color output can also be controlled via environment variables:

- `NO_COLOR=1` — disables color output
- `CLICOLOR=0` — disables color output
- `CLICOLOR_FORCE=1` — forces color output even when not in a terminal

### Quick Package Inspection

Combine commands to quickly pull and inspect:

```bash
wasm package pull ghcr.io/example/component:latest
wasm inspect ~/.local/share/wasm/store/content/<digest>
```

### Finding Package Content

After pulling a package, use `wasm package list` to find its digest, then access content in the store directory.

### Using with CI/CD

In CI/CD pipelines:

1. Authenticate using `docker login` or similar
2. Use `wasm package pull` to retrieve dependencies
3. Use `wasm package push` to publish artifacts
4. Use `wasm self clean` to manage storage between builds

## Troubleshooting

### Package Not Found

If pulling fails with "not found":
- Verify the package reference is correct
- Check authentication (see [Authentication](authentication.md))
- Ensure the package exists and is accessible

### Storage Issues

If you encounter storage errors:
- Run `wasm self state` to check space
- Run `wasm self clean` to free up space
- Check filesystem permissions on `~/.local/share/wasm`

### Network Errors

For network-related failures:
- Check internet connectivity
- Verify registry is accessible
- Check firewall and proxy settings

## Further Reading

- [Authentication](authentication.md) - Set up registry access
- [Configuration](configuration.md) - Understand storage and settings
- [API Documentation](https://docs.rs/wasm) - Library usage

## Component Composition

### Workspace Layout

Running `wasm init` creates a workspace that includes composition directories:

```text
my-workspace/
├── types/         # WIT interface definition files (.wit)
├── seams/         # WAC composition scripts (.wac)
├── build/         # Composed output artifacts
└── deps/
    ├── vendor/
    │   ├── wasm/  # Vendored component binaries
    │   └── wit/   # Vendored WIT interfaces
    ├── wasm.toml
    └── wasm.lock.toml
```

### WAC Scripts

[WAC (WebAssembly Composition)](https://github.com/bytecodealliance/wac) is a
declarative language for composing Wasm components. Place `.wac` files in the
`seams/` directory to define how components are wired together.

### `wasm compose`

Compose Wasm components from WAC scripts:

```bash
# Compose a named WAC file (looks for seams/my-composition.wac)
wasm compose my-composition

# Compose all WAC files in seams/
wasm compose

# Use dynamic linking (import dependencies instead of embedding)
wasm compose my-composition --linker=dynamic

# Specify output directory
wasm compose my-composition -o output/
```

### Package Resolution

When resolving packages referenced in WAC files, the resolver checks:

1. **Manifest entries** — components and interfaces in `deps/wasm.toml` mapped
   to vendored files in `deps/vendor/wasm/` and `deps/vendor/wit/`.
2. **Local directories** — `.wasm` and `.wit` files in `types/`.

## Getting Help

- GitHub Issues: [https://github.com/yoshuawuyts/wasm/issues](https://github.com/yoshuawuyts/wasm/issues)
- Command help: `wasm --help`
- This documentation: `/docs` directory
