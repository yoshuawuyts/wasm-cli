# Registry Server

r[server.health]
The `/v1/health` endpoint MUST return `200 OK` with `{"status": "ok"}`.

## Package Indexing

r[server.index.dependencies]
The indexer MUST extract WIT dependencies from each indexed package version by
pulling the wasm layer and parsing its WIT metadata. The extracted dependency
graph MUST be stored in the local database so that the `/v1/packages` endpoint
can include dependency information in its responses.

## Rich API Endpoints

r[server.detail]
`GET /v1/packages/detail/{registry}/{*repository}` MUST return a
`PackageDetail` object containing all known versions with per-version
metadata, or `404 Not Found` when the repository does not exist.

r[server.versions.list]
`GET /v1/packages/versions/{registry}/{*repository}` MUST return a JSON array
of `PackageVersion` objects for every manifest in the repository, ordered
newest first.

r[server.versions.get]
`GET /v1/packages/version/{registry}/{version}/{*repository}` MUST return
the `PackageVersion` matching the given tag, or `404 Not Found` when no such
tag exists.

r[server.search.by-import]
`GET /v1/search/by-import?interface={interface}` MUST return packages whose
WIT worlds import the given interface.

r[server.search.by-export]
`GET /v1/search/by-export?interface={interface}` MUST return packages whose
WIT worlds export the given interface.
