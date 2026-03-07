# Lockfile

r[lockfile.parse]
The lockfile parser MUST handle TOML lockfiles with version and packages.

r[lockfile.serialize]
The lockfile serializer MUST produce valid TOML output.

r[lockfile.no-dependencies.parse]
Parsing packages without dependencies MUST succeed.

r[lockfile.no-dependencies.serialize]
Serializing packages without dependencies MUST produce valid output.

r[lockfile.mixed-types.parse]
The lockfile MUST support both component and interface package types.

r[lockfile.mixed-types.all-packages]
Iterating all packages MUST yield both component and interface entries.

r[lockfile.required-fields]
All package entries and dependency entries in the lockfile MUST include name, version, registry, and digest fields.
