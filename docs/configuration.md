# Configuration

`wasm(1)` uses a local storage system to manage downloaded packages and metadata. This guide explains the storage layout and configuration options.

## Storage Location

`wasm(1)` follows the [XDG Base Directory specification](https://specifications.freedesktop.org/basedir-spec/latest/) for storing data:

| XDG Variable       | Purpose                                                | Unix / macOS default | Windows default |
| ------------------ | ------------------------------------------------------ | -------------------- | --------------- |
| `$XDG_CONFIG_HOME` | User-specific configuration files                      | `~/.config`          | `%APPDATA%`     |
| `$XDG_DATA_HOME`   | User-specific data files                               | `~/.local/share`     | `%LOCALAPPDATA%`|
| `$XDG_STATE_HOME`  | User-specific state data (logs, history, recent files) | `~/.local/state`     | `%LOCALAPPDATA%`|

Setting the XDG environment variable always takes precedence on every platform.

## Configuration Files

`wasm(1)` loads configuration from two locations and merges them. Settings in the local config take precedence over the global config.

| Location | Path |
| -------- | ---- |
| Global   | `$XDG_CONFIG_HOME/wasm/config.toml` |
| Local    | `.config/wasm/config.toml` (relative to the current working directory) |

To view the current configuration and file locations:

```bash
wasm self config
```

### Configuration Format

The configuration file uses TOML format. Here's an example with all available options:

```toml
# ~/.config/wasm/config.toml

# Per-registry credential helpers
# These allow you to securely retrieve credentials from password managers
# or other secret stores without storing them in plain text.
# Each command's stdout (trimmed) is used as the credential value.

[registries."ghcr.io"]
credential-helper.username = "/path/to/get-user.sh"
credential-helper.password = "/path/to/get-pass.sh"
```

### Credential Helpers

Credential helpers allow you to integrate with password managers and secret stores for secure authentication. When `wasm` needs to authenticate with a registry, it first checks if a credential helper is configured. If not, it falls back to the Docker credential store.

Credential helpers use two separate commands: one for the username and one for the password. Each command is executed through the shell and its stdout (trimmed) is used as the credential value.

#### 1Password Integration

To use 1Password with `wasm`, first ensure you have the [1Password CLI](https://developer.1password.com/docs/cli/) installed and configured. Then add your registry credentials to 1Password and configure the helper:

```toml
[registries."ghcr.io"]
credential-helper.username = "op read 'op://Vault/ghcr/username'"
credential-helper.password = "op read 'op://Vault/ghcr/token'"
```

#### Custom Scripts

For custom secret stores or other password managers, you can use scripts:

```toml
[registries."my-registry.example.com"]
credential-helper.username = "/path/to/get-username.sh"
credential-helper.password = "/path/to/get-password.sh"
```

Each script should output the credential value to stdout (trailing whitespace is trimmed).

### Security Notes

- **Credentials are cached in memory** during program execution for performance, but are never written to disk.
- **Prefer credential helpers** over storing credentials in Docker's credential store when using sensitive tokens.
- **Protect the config file**: Set appropriate permissions on your config file (e.g., `chmod 600 ~/.config/wasm/config.toml`) since it contains commands that will be executed.
- **Keep scripts secure**: Ensure credential helper scripts have appropriate permissions (e.g., `chmod 700`).
- **Command execution**: Credential helper commands are executed through the shell with your user privileges. Only configure commands you trust.

## Storage Layout

The storage directory has the following structure:

```
~/.local/share/wasm/
├── store/              # Content-addressable blob storage
│   ├── content/        # OCI image layers and artifacts
│   └── index/          # Cache index files
└── db/
    └── metadata.db3    # SQLite database with package metadata
```

### Components

#### Content-Addressable Store (`store/`)

The store directory uses [`cacache`](https://docs.rs/cacache/) for content-addressable storage. It stores the entire OCI image including any signatures and attestations:

- **Immutable**: Content is stored by its SHA-256 hash
- **Deduplicated**: Identical content is stored only once
- **OCI-Compatible**: Stores image layers and manifests following OCI specifications

#### Metadata Database (`db/metadata.db3`)

The metadata database is a SQLite database that stores package metadata.

## Storage Management

### Viewing Storage Usage

Check storage usage with:

```bash
wasm self state
```

### Cleaning Up Storage

Remove unused content and optimize the database:

```bash
wasm self clean
```

This command:
- Removes orphaned content from the store
- Vacuums the SQLite database
- Reclaims disk space

### Listing Stored Packages

View all locally stored packages:

```bash
wasm package list
```

## Database Migrations

The storage system uses a migration system to evolve the schema over time:

- **Automatic**: Migrations run automatically when opening the database
- **Versioned**: Each migration is numbered and tracked
- **Forward-Only**: The system only supports forward migrations

Current migrations include:
1. Initial schema creation
2. Known packages table
3. Tag type classification
