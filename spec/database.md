# Database

## Migrations

r[db.migrations.create-tables]
Running all migrations MUST create the required database tables.

r[db.migrations.idempotent]
Running migrations MUST be idempotent.

r[db.migrations.info]
Migration info MUST be retrievable.

## Known Packages

r[db.known-packages.upsert-new]
Upserting a new known package MUST insert it.

r[db.known-packages.upsert-existing]
Upserting an existing known package MUST update it.

r[db.known-packages.get]
A known package MUST be retrievable by ID after upsert.

r[db.known-packages.search]
Known package search MUST return matching results.

r[db.known-packages.search-empty]
Known package search MUST handle no results gracefully.

r[db.known-packages.reference]
Known package reference strings MUST be generated correctly.

r[db.known-packages.reference-default-tag]
Known package references with a default tag MUST be generated correctly.

r[db.known-packages.search-by-wit-name]
Searching known packages by WIT name (e.g. `wasi:http`) MUST convert
the name to a repository search pattern and return the best match.

r[db.known-packages.search-by-wit-name-not-found]
Searching known packages by WIT name MUST return `None` when no match
is found.

## WIT Packages

r[db.wit-package.find-oci-reference]
Given a WIT package name and version, the store MUST resolve the OCI
registry and repository by JOINing through `oci_manifest` → `oci_repository`.

r[db.wit-package.find-oci-reference-not-found]
Looking up an OCI reference for a WIT package that does not exist MUST
return `None`.

r[db.wit-package.find-oci-reference-no-version]
Looking up an OCI reference for a WIT package without a version MUST
still resolve correctly when the package was stored without a version.

## WIT Package Dependencies

Dependencies between WIT packages are recorded in the `wit_package_dependency`
table. This allows the resolver to compute a full transitive dependency graph
before any package is installed.

r[db.wit-package-dependency.populate-on-sync]
On sync, the local database MUST be populated with dependency versions from
the meta-registry. For each package in the sync response that carries dependency
information, a `wit_package` row and corresponding `wit_package_dependency` rows
MUST be created so that the dependency graph is available for pre-planned
installation without additional network requests.

r[db.wit-package-dependency.get-for-package]
Given a registry and repository, the store MUST return all declared dependencies
of that package. For pulled packages the dependencies are sourced from the
**latest** indexed manifest (by insertion order). For sync stubs (packages
stored without an OCI manifest link) the dependencies are sourced by matching
`oci_repository.wit_namespace` / `oci_repository.wit_name`.

r[db.wit-package-dependency.upsert-idempotent]
Upserting the same package dependency MUST be idempotent (inserting duplicate
edges MUST be silently ignored).
