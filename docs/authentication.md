# Authentication

`wasm(1)` uses OCI-compatible authentication to access container registries for pulling and pushing WebAssembly packages.

## Overview

`wasm(1)` supports multiple authentication methods:

1. **Credential Helpers** (recommended): Configure per-registry credential helpers in the config file to securely retrieve credentials from password managers like 1Password
2. **Docker Credential Store**: Automatically uses credentials stored by Docker or Podman

The authentication resolution order is:
1. Check config file for a credential helper for the registry
2. Fall back to Docker credential store
3. Use anonymous access if no credentials are found

## Authentication Methods

### Credential Helpers (Recommended)

You can configure credential helpers in your config file (`~/.config/wasm/config.toml`) to securely retrieve credentials from password managers or secret stores.

```toml
# Using separate scripts
[registries."ghcr.io"]
credential-helper.username = "/path/to/get-user.sh"
credential-helper.password = "/path/to/get-pass.sh"

# Using separate scripts for another registry
[registries."my-registry.example.com"]
credential-helper.username = "/path/to/get-user.sh"
credential-helper.password = "/path/to/get-pass.sh"
```

See [Configuration](configuration.md#credential-helpers) for detailed setup instructions.

### Docker Credential Store

`wasm(1)` uses the [`docker_credential`](https://docs.rs/docker_credential/) crate to access credentials stored by Docker/Podman credential helpers.

The authentication flow:

1. When pulling or pushing a package, `wasm(1)` extracts the registry hostname from the reference
2. It queries the Docker credential store for credentials associated with that registry
3. If credentials are found, they're used for authentication
4. If no credentials are found, anonymous access is attempted

### Supported Credential Types

- **Username/Password**: Basic authentication with username and password
- **Anonymous**: No authentication (for public registries)

**Note**: Identity tokens are currently not supported.

## Setting Up Authentication

### Using Credential Helpers

1. Create the config directory and file:
   ```bash
   mkdir -p ~/.config/wasm
   touch ~/.config/wasm/config.toml
   ```

2. Add your credential helper configuration:
   ```toml
   [registries."ghcr.io"]
   credential-helper.username = "op read 'op://Vault/ghcr/username'"
   credential-helper.password = "op read 'op://Vault/ghcr/token'"
   ```

3. Verify the configuration:
   ```bash
   wasm self config
   ```

### Using Docker Login

The easiest way to set up authentication is to use Docker's login command:

```bash
# For Docker Hub
docker login

# For GitHub Container Registry
docker login ghcr.io

# For a custom registry
docker login myregistry.example.com
```

Once logged in, `wasm` will automatically use these credentials.

### Using Podman Login

If you use Podman instead of Docker:

```bash
# For GitHub Container Registry
podman login ghcr.io

# For a custom registry
podman login myregistry.example.com
```

## Troubleshooting

### Anonymous Access

If you see an "anonymous access" message, it means:
- No credentials were found for the registry
- The tool is attempting to access the registry without authentication
- This works for public repositories but will fail for private ones

### Unsupported Identity Tokens

If you receive an "identity tokens not supported" error:
- The credential store returned an identity token
- `wasm` currently only supports username/password authentication
- Try logging in again with username/password credentials

### Credential Store Not Found

If credential lookups fail:
- Ensure Docker or Podman is installed and configured
- Verify you've logged in to the registry at least once
- Check that credential helpers are properly configured in `~/.docker/config.json`

### Credential Helper Errors

If credential helper commands fail:
- Verify the command works when run manually in your terminal
- Check that the output format matches what `wasm` expects (see [Configuration](configuration.md#credential-helpers))
- Ensure the credential helper program is installed and in your PATH
