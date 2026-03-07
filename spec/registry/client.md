# Registry Client

## Known Package

The `KnownPackage` type is the shared wire type returned by the meta-registry
`/v1/packages` endpoint.

r[client.known-package.reference]
`KnownPackage::reference()` MUST return `"{registry}/{repository}"`.

r[client.known-package.reference-with-tag]
`KnownPackage::reference_with_tag()` MUST return `"{registry}/{repository}:{tag}"`
using the first tag when tags are present.

r[client.known-package.reference-default-tag]
`KnownPackage::reference_with_tag()` MUST fall back to `"latest"` when no tags
are present.

r[client.known-package.dependencies]
`KnownPackage` MUST represent the declared WIT package dependencies for the
package's latest indexed version in a `dependencies` field. The field MAY be
omitted when no dependency information is available; omission MUST be treated
as equivalent to an empty list. Each entry MUST carry the declared package
name and an optional version string.

## Registry Client

The `RegistryClient` fetches packages from the meta-registry `/v1/packages`
endpoint with conditional ETag support and exponential-backoff retries.

r[client.fetch.etag-not-modified]
When the server responds with 304 Not Modified, `fetch_packages` MUST return
`FetchResult::NotModified`.

r[client.fetch.updated]
When the server responds with new data, `fetch_packages` MUST return
`FetchResult::Updated` containing the package list and optional ETag.

r[client.fetch.retry]
`fetch_packages` MUST retry transient errors up to 3 times with exponential
backoff.

r[client.fetch.server-error]
`fetch_packages` MUST treat 5xx responses as transient errors.

r[client.fetch.client-error]
`fetch_packages` MUST treat non-success, non-304 responses as errors.
