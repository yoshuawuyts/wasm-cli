# wasm-meta-registry-client

HTTP client for fetching package metadata from a
[wasm-meta-registry](../wasm-meta-registry) instance.

## Features

- Shared `KnownPackage` type matching the meta-registry `/v1/packages` API
- `RegistryClient` with ETag-based conditional fetches and exponential-backoff
  retries (behind the `client` feature, enabled by default)

## Usage

```rust,no_run
use wasm_meta_registry_client::{KnownPackage, RegistryClient, FetchResult};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = RegistryClient::new("http://localhost:3000");
    match client.fetch_packages(None, 100).await? {
        FetchResult::NotModified => println!("up to date"),
        FetchResult::Updated { packages, .. } => {
            for pkg in &packages {
                println!("{}", pkg.reference());
            }
        }
    }
    Ok(())
}
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
