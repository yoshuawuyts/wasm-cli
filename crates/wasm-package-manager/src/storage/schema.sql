-- schema.sql — canonical representation of the full database schema.
--
-- This file is the single source of truth for the database structure.
-- To change the schema, edit this file, then run:
--
--     cargo xtask sql migrate --name <description>
--
-- Never hand-write migration files.
--
-- NOTE: Use CURRENT_TIMESTAMP instead of datetime('now') for DEFAULT values.
-- The sqlite3def tool cannot parse datetime('now') in DDL.
--
-- Three-layer design:
--   1. OCI layer  — models the OCI distribution spec: repositories,
--                    manifests, tags, layers, annotations, referrers
--   2. WIT layer  — models the WebAssembly Interface Type system:
--                    packages, worlds, imports, exports,
--                    and inter-package dependencies
--   3. Wasm layer — models compiled WebAssembly components and
--                    which worlds they target
--
-- Plus operational tables:
--   - migrations  — schema version tracking
--   - _sync_meta  — registry sync state (ETags, timestamps)
--
-- All relationships between the WIT/Wasm layers and specific OCI
-- sources are nominal (by declared name) with optional resolution
-- to a concrete row, preserving ambiguity for end-user choice.

-- ============================================================
-- Operational tables
-- ============================================================

-- Tracks which schema migrations have been applied, ensuring each
-- migration runs exactly once and in order.
CREATE TABLE migrations (
    -- Surrogate primary key for the migration record.
    id INTEGER PRIMARY KEY,
    -- Sequential migration version number; enforced unique so
    -- the same migration cannot be applied twice.
    version INTEGER NOT NULL UNIQUE,
    -- ISO 8601 timestamp of when this migration was applied.
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Key-value store for registry sync state.  Used by the sync
-- subsystem to track conditional-request headers and timing so
-- that repeated syncs are cheap (ETag/If-None-Match) and
-- rate-limited (minimum interval between syncs).
--
-- Well-known keys:
--   "packages_etag"   — the ETag header value from the last
--                        successful GET /v1/packages response,
--                        sent back as If-None-Match on the next
--                        request to avoid re-downloading unchanged
--                        data (HTTP 304 Not Modified).
--   "last_synced_at"  — ISO 8601 timestamp of the last successful
--                        sync attempt, used to enforce a minimum
--                        interval between syncs (e.g. 3600s) so
--                        the CLI doesn't hit the registry on every
--                        invocation.
--
-- Additional keys may be added as new sync sources are introduced
-- (e.g. per-registry ETags, cursor tokens for paginated APIs).
CREATE TABLE _sync_meta (
    -- The metadata key, e.g. "packages_etag", "last_synced_at".
    -- Serves as the primary key; each key appears at most once.
    `key` TEXT PRIMARY KEY NOT NULL,
    -- The metadata value.  Interpretation depends on the key:
    -- timestamps are ISO 8601 strings, ETags are opaque strings
    -- returned by the server.
    `value` TEXT NOT NULL
);

-- ============================================================
-- OCI LAYER: Repositories, Manifests, Tags, Layers, Referrers
-- ============================================================

-- An OCI repository is the combination of a registry host and a
-- repository path.  This is the unit that the OCI Tags List API
-- operates on (GET /v2/<repository>/tags/list).
CREATE TABLE oci_repository (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The registry hostname, e.g. "ghcr.io", "webassembly.org".
    registry TEXT NOT NULL,
    -- The repository path within the registry,
    -- e.g. "webassembly/wasi/http".
    repository TEXT NOT NULL,
    -- ISO 8601 timestamp of when this repository was first recorded.
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    -- ISO 8601 timestamp of the most recent modification to this
    -- row.  Maintained automatically by trg_oci_repository_updated_at.
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    -- Optional WIT namespace this repository is published under,
    -- e.g. "wasi", "ba".  NULL when the repository was discovered
    -- via a direct OCI reference rather than through the meta-registry.
    wit_namespace TEXT,
    -- Optional WIT package name within the namespace, e.g. "http",
    -- "sample-wasi-http-rust".  NULL when wit_namespace is NULL.
    wit_name TEXT,
    -- Package kind: "component" for a runnable Wasm component,
    -- "interface" for a WIT interface type package.  NULL when
    -- the kind has not been determined yet.
    kind TEXT,
    UNIQUE(registry, repository)
);

-- Automatically advances updated_at on any UPDATE, unless the
-- caller has already set it (guard prevents infinite recursion
-- if PRAGMA recursive_triggers = ON is ever enabled).
CREATE TRIGGER trg_oci_repository_updated_at
    AFTER UPDATE ON oci_repository
    FOR EACH ROW
    WHEN OLD.updated_at = NEW.updated_at
    BEGIN
        UPDATE oci_repository
           SET updated_at = CURRENT_TIMESTAMP
         WHERE id = OLD.id;
    END;

-- An OCI manifest represents a single immutable revision inside a
-- repository, identified by its content-addressable digest.
-- Well-known OCI annotation keys are promoted to first-class columns
-- for indexed queries; all other annotations overflow into
-- oci_manifest_annotation.
-- See: https://specs.opencontainers.org/image-spec/annotations/
CREATE TABLE oci_manifest (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The repository this manifest belongs to.
    oci_repository_id INTEGER NOT NULL,
    -- Content-addressable digest of the manifest,
    -- e.g. "sha256:abcdef1234…".
    digest TEXT NOT NULL,
    -- The manifest's own media type,
    -- e.g. "application/vnd.oci.image.manifest.v1+json".
    media_type TEXT,
    -- The full manifest JSON document, stored verbatim for
    -- offline inspection without re-fetching from the registry.
    raw_json TEXT,
    -- Total size of the manifest and its layers in bytes.
    -- NULL means the size has not been populated yet;
    -- 0 means genuinely zero bytes.
    size_bytes INTEGER,
    -- ISO 8601 timestamp of when this manifest was first recorded.
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Top-level OCI manifest fields (not annotations) ────────

    -- The artifactType field from the OCI manifest.  This is the
    -- primary type-dispatch key used by clients to determine what
    -- kind of artifact a manifest contains (e.g. "application/wasm").
    artifact_type TEXT,
    -- The mediaType of the config descriptor in the manifest,
    -- used by older OCI artifact conventions and some Wasm toolchains.
    config_media_type TEXT,
    -- The digest of the config descriptor in the manifest,
    -- companion to config_media_type.
    config_digest TEXT,

    -- OCI well-known annotation columns ──────────────────────
    -- Each corresponds to a reserved org.opencontainers.image.*
    -- annotation key.  Promoted to columns for direct indexing
    -- and thin-sync queries without JSON parsing.

    -- org.opencontainers.image.created
    -- Date/time the image was built, in RFC 3339 format.
    oci_created TEXT,
    -- org.opencontainers.image.authors
    -- Free-form contact details for the people or organization
    -- responsible for the image.
    oci_authors TEXT,
    -- org.opencontainers.image.url
    -- URL where users can find more information about the image.
    oci_url TEXT,
    -- org.opencontainers.image.documentation
    -- URL to the documentation for the image.
    oci_documentation TEXT,
    -- org.opencontainers.image.source
    -- URL to the source code used to build the image.
    oci_source TEXT,
    -- org.opencontainers.image.version
    -- Version of the packaged software; may follow semver but
    -- is not required to.
    oci_version TEXT,
    -- org.opencontainers.image.revision
    -- Source-control revision identifier for the packaged software,
    -- e.g. a git commit SHA.
    oci_revision TEXT,
    -- org.opencontainers.image.vendor
    -- Name of the distributing entity, organization, or individual.
    oci_vendor TEXT,
    -- org.opencontainers.image.licenses
    -- License(s) under which the contained software is distributed,
    -- expressed as an SPDX License Expression.
    oci_licenses TEXT,
    -- org.opencontainers.image.ref.name
    -- Name of the reference for a target, typically a tag name
    -- matching the OCI reference grammar.
    oci_ref_name TEXT,
    -- org.opencontainers.image.title
    -- Human-readable title of the image.
    oci_title TEXT,
    -- org.opencontainers.image.description
    -- Human-readable description of the software packaged in the image.
    oci_description TEXT,
    -- org.opencontainers.image.base.digest
    -- Digest of the base image this image was built upon
    -- (e.g. from a Dockerfile FROM statement).
    oci_base_digest TEXT,
    -- org.opencontainers.image.base.name
    -- Image reference (name) of the base image this image was
    -- built upon.
    oci_base_name TEXT,

    UNIQUE(oci_repository_id, digest),
    FOREIGN KEY (oci_repository_id) REFERENCES oci_repository(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- Key-value overflow table for manifest annotations that are not
-- in the well-known org.opencontainers.image.* set.  Stores
-- vendor-specific or custom annotation keys without requiring
-- schema migrations for each new key.
CREATE TABLE oci_manifest_annotation (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The manifest this annotation belongs to.
    oci_manifest_id INTEGER NOT NULL,
    -- The full annotation key, e.g. "com.example.custom-key".
    `key` TEXT NOT NULL,
    -- The annotation value.
    `value` TEXT NOT NULL,
    UNIQUE(oci_manifest_id, `key`),
    FOREIGN KEY (oci_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- A tag is a mutable, human-readable pointer to a manifest within
-- the same repository.  Multiple tags can point to the same manifest.
--
-- The composite FK on (oci_repository_id, manifest_digest) referencing
-- oci_manifest(oci_repository_id, digest) guarantees that the tag and
-- manifest share the same repository.  There is intentionally no
-- separate oci_manifest_id column to avoid split-brain references —
-- join back to oci_manifest via (oci_repository_id, manifest_digest)
-- to obtain the surrogate id when needed.
CREATE TABLE oci_tag (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The repository this tag belongs to.
    oci_repository_id INTEGER NOT NULL,
    -- The digest of the manifest this tag currently points to.
    -- Denormalized from oci_manifest to enable the composite FK
    -- that enforces same-repository membership.
    manifest_digest TEXT NOT NULL,
    -- The tag string, e.g. "1.0.0", "latest".
    tag TEXT NOT NULL,
    -- ISO 8601 timestamp of when this tag was first created.
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    -- ISO 8601 timestamp of the most recent modification to this
    -- row (e.g. when the tag is moved to a new digest).
    -- Maintained automatically by trg_oci_tag_updated_at.
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(oci_repository_id, tag),
    FOREIGN KEY (oci_repository_id) REFERENCES oci_repository(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (oci_repository_id, manifest_digest)
        REFERENCES oci_manifest(oci_repository_id, digest)
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- Automatically advances updated_at on any UPDATE, with a guard
-- to prevent infinite recursion under recursive_triggers.
CREATE TRIGGER trg_oci_tag_updated_at
    AFTER UPDATE ON oci_tag
    FOR EACH ROW
    WHEN OLD.updated_at = NEW.updated_at
    BEGIN
        UPDATE oci_tag
           SET updated_at = CURRENT_TIMESTAMP
         WHERE id = OLD.id;
    END;

-- An individual content-addressable blob (layer) referenced by a
-- manifest.  Each layer has its own digest and a position defining
-- its order within the manifest's layer array.
CREATE TABLE oci_layer (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The manifest this layer belongs to.
    oci_manifest_id INTEGER NOT NULL,
    -- Content-addressable digest of this layer blob,
    -- e.g. "sha256:fedcba9876…".
    digest TEXT NOT NULL,
    -- The media type of this layer,
    -- e.g. "application/wasm", "application/vnd.oci.image.layer.v1.tar+gzip".
    media_type TEXT,
    -- Size of this individual layer in bytes.
    -- NULL means the size has not been populated yet;
    -- 0 means genuinely zero bytes.
    size_bytes INTEGER,
    -- Zero-based ordinal position of this layer within the manifest's
    -- layer array.  Determines deterministic ordering.
    position INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (oci_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);

CREATE UNIQUE INDEX uq_oci_layer_digest ON oci_layer(oci_manifest_id, digest);
CREATE UNIQUE INDEX uq_oci_layer_position ON oci_layer(oci_manifest_id, position);

-- Key-value overflow table for layer-level annotations.  Some Wasm
-- toolchains attach metadata at the layer descriptor level rather
-- than the manifest level.
CREATE TABLE oci_layer_annotation (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The layer this annotation belongs to.
    oci_layer_id INTEGER NOT NULL,
    -- The full annotation key.
    `key` TEXT NOT NULL,
    -- The annotation value.
    `value` TEXT NOT NULL,
    UNIQUE(oci_layer_id, `key`),
    FOREIGN KEY (oci_layer_id) REFERENCES oci_layer(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- Models the OCI distribution-spec Referrers API: artifacts such as
-- signatures, SBOMs, and attestations that reference a subject
-- manifest.  A referrer is itself a manifest (with its own digest,
-- layers, and annotations) stored in oci_manifest.
CREATE TABLE oci_referrer (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The manifest being signed, attested, or otherwise referenced.
    subject_manifest_id INTEGER NOT NULL,
    -- The artifact manifest itself (the signature, SBOM, etc.).
    -- This row's own digest, layers, and annotations live in
    -- oci_manifest / oci_layer / etc.
    referrer_manifest_id INTEGER NOT NULL,
    -- The OCI artifact type of the referrer, e.g.
    -- "application/vnd.dev.cosign.simplesigning.v1+json" for
    -- Cosign signatures.  This is the primary filter key when
    -- listing referrers for a subject.
    artifact_type TEXT NOT NULL,
    -- ISO 8601 timestamp of when this referrer relationship was
    -- first recorded.
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(subject_manifest_id, referrer_manifest_id),
    FOREIGN KEY (subject_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (referrer_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- ============================================================
-- WIT LAYER: Packages, Worlds, Imports, Exports, Dependencies
--
-- A WIT package is the top-level publishable artifact in the
-- WebAssembly Interface Types system.  It declares a package name
-- and version (e.g. "wasi:http@0.3.0") and contains zero or more
-- named worlds.
--
-- The same (package_name, version) may exist from multiple OCI
-- sources — identity is nominal, not structural.  This preserves
-- the real-world ambiguity (e.g. "wasi:http@0.3.0" published by
-- both ghcr.io and webassembly.org) and lets the end-user choose.
-- ============================================================

CREATE TABLE wit_package (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The WIT package name (namespace:name), e.g. "wasi:http".
    -- This is the primary search key for package lookups.
    package_name TEXT NOT NULL,
    -- The semver version string, e.g. "0.3.0".
    -- NULL if the package was recorded without a version.
    version TEXT,
    -- Human-readable description of this WIT package, used for
    -- search results and thin-sync metadata.
    description TEXT,
    -- The full WIT source text, stored verbatim for offline
    -- inspection and tooling without re-fetching from the registry.
    wit_text TEXT,

    -- Provenance: which OCI artifact was this WIT package
    -- extracted from?  Both are nullable because a WIT package
    -- can be registered out-of-band (e.g. manually or from a
    -- local file) without any OCI backing.

    -- The OCI manifest this WIT package was found in.
    -- SET NULL on delete so the WIT metadata survives even if
    -- the OCI manifest is purged.
    oci_manifest_id INTEGER,
    -- The specific OCI layer (blob) this WIT package was
    -- extracted from — either a .wit text file or a .wasm
    -- binary with embedded WIT.
    oci_layer_id INTEGER,

    -- ISO 8601 timestamp of when this package was first recorded.
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (oci_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE SET NULL,
    FOREIGN KEY (oci_layer_id) REFERENCES oci_layer(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);

-- Deduplication index using COALESCE to canonicalize NULLs,
-- preventing SQLite's NULL ≠ NULL semantics from allowing
-- duplicate rows.
CREATE UNIQUE INDEX uq_wit_packages ON wit_package(
    package_name,
    COALESCE(version, ''),
    COALESCE(oci_layer_id, -1)
);

-- A named world defined inside a WIT package.  A world is a
-- contract that declares which interfaces a component must import
-- and export in order to run.  For example, "proxy" inside
-- wasi:http defines the "wasi:http/proxy@0.3.0" world.
CREATE TABLE wit_world (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The WIT package this world is defined in.
    wit_package_id INTEGER NOT NULL,
    -- The world's name within the package, e.g. "proxy", "command".
    name TEXT NOT NULL,
    -- Human-readable description of what this world contract
    -- represents, used for search results and documentation.
    description TEXT,
    -- ISO 8601 timestamp of when this world was first recorded.
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(wit_package_id, name),
    FOREIGN KEY (wit_package_id) REFERENCES wit_package(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- Records that a world imports (depends on) an interface.
-- References are nominal: the declared name is always stored, and
-- resolution to a specific wit_package row is optional.
-- NULL resolved_package_id means "not yet resolved" or
-- "ambiguous across multiple OCI sources."
CREATE TABLE wit_world_import (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The world that declares this import.
    wit_world_id INTEGER NOT NULL,
    -- The declared package name of the imported interface,
    -- e.g. "wasi:io".
    declared_package TEXT NOT NULL,
    -- The declared sub-interface name within the package,
    -- e.g. "streams".  NULL means the entire package is imported
    -- rather than a specific sub-interface.
    declared_interface TEXT,
    -- The declared version of the imported interface,
    -- e.g. "0.2.2".  NULL if no version was specified.
    declared_version TEXT,
    -- Optionally resolved to a specific wit_package row.
    -- NULL if resolution has not been performed or if multiple
    -- OCI sources publish the same package and the choice is
    -- ambiguous.
    resolved_package_id INTEGER,
    FOREIGN KEY (wit_world_id) REFERENCES wit_world(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (resolved_package_id) REFERENCES wit_package(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE UNIQUE INDEX uq_wit_world_import ON wit_world_import(
    wit_world_id,
    declared_package,
    COALESCE(declared_interface, ''),
    COALESCE(declared_version, '')
);

-- Records that a world exports (implements) an interface.
-- Same nominal semantics as wit_world_import.
CREATE TABLE wit_world_export (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The world that declares this export.
    wit_world_id INTEGER NOT NULL,
    -- The declared package name of the exported interface,
    -- e.g. "wasi:http".
    declared_package TEXT NOT NULL,
    -- The declared sub-interface name within the package,
    -- e.g. "handler".  NULL means the entire package is exported.
    declared_interface TEXT,
    -- The declared version of the exported interface.
    -- NULL if no version was specified.
    declared_version TEXT,
    -- Optionally resolved to a specific wit_package row.
    -- NULL if unresolved or ambiguous.
    resolved_package_id INTEGER,
    FOREIGN KEY (wit_world_id) REFERENCES wit_world(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (resolved_package_id) REFERENCES wit_package(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE UNIQUE INDEX uq_wit_world_export ON wit_world_export(
    wit_world_id,
    declared_package,
    COALESCE(declared_interface, ''),
    COALESCE(declared_version, '')
);

-- Records that one WIT package depends on another at the package
-- level.  For example, wasi:http depends on wasi:io.  This enables
-- dependency graph traversal and impact analysis ("what breaks if
-- wasi:io changes?").  Nominal, with optional resolution.
CREATE TABLE wit_package_dependency (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The WIT package that declares this dependency.
    dependent_id INTEGER NOT NULL,
    -- The declared package name of the dependency,
    -- e.g. "wasi:io".
    declared_package TEXT NOT NULL,
    -- The declared version of the dependency, e.g. "0.2.2".
    -- NULL if no version was specified.
    declared_version TEXT,
    -- Optionally resolved to a specific wit_package row.
    -- NULL if unresolved or ambiguous.
    resolved_package_id INTEGER,
    FOREIGN KEY (dependent_id) REFERENCES wit_package(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (resolved_package_id) REFERENCES wit_package(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE UNIQUE INDEX uq_wit_package_dependency ON wit_package_dependency(
    dependent_id,
    declared_package,
    COALESCE(declared_version, '')
);

-- ============================================================
-- WASM LAYER: Components
-- ============================================================

-- A compiled WebAssembly component binary discovered inside an OCI
-- manifest revision.  The full OCI URL can be reconstructed by
-- joining through oci_manifest → oci_repository; the optional name
-- is a human-readable identifier extracted from the component's
-- embedded metadata.
CREATE TABLE wasm_component (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The OCI manifest revision this component was discovered in.
    oci_manifest_id INTEGER NOT NULL,
    -- The specific OCI layer (blob) that contains this component's
    -- .wasm binary.  NULL if the layer association has not been
    -- established (e.g. single-layer manifests where it's implicit).
    -- SET NULL on delete so the component metadata survives even if
    -- the layer is purged.
    oci_layer_id INTEGER,
    -- Human-readable name extracted from the component's embedded
    -- metadata.  NULL if the component has no embedded name.
    name TEXT,
    -- Human-readable description extracted from the component's
    -- embedded metadata.  NULL if none is present.
    description TEXT,
    -- ISO 8601 timestamp of when this component was first recorded.
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (oci_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (oci_layer_id) REFERENCES oci_layer(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);

-- Prevents duplicate component rows when re-indexing a manifest.
CREATE UNIQUE INDEX uq_wasm_component ON wasm_component(
    oci_manifest_id,
    COALESCE(oci_layer_id, -1)
);

-- Records that a component targets (is built against) a specific
-- WIT world.  A component says "I target wasi:http/proxy@0.3.0" —
-- the world defines which interfaces must be imported and exported.
-- Nominal, with optional resolution.
CREATE TABLE component_target (
    -- Surrogate primary key.
    id INTEGER PRIMARY KEY,
    -- The component that declares this target.
    wasm_component_id INTEGER NOT NULL,
    -- The declared package name of the targeted world's type,
    -- e.g. "wasi:http".
    declared_package TEXT NOT NULL,
    -- The declared world name within the package, e.g. "proxy".
    declared_world TEXT NOT NULL,
    -- The declared version of the targeted world, e.g. "0.3.0".
    -- NULL if no version was specified.
    declared_version TEXT,
    -- Optionally resolved to a specific wit_world row.
    -- NULL if resolution has not been performed or if the world
    -- exists in multiple OCI sources and the choice is ambiguous.
    wit_world_id INTEGER,
    FOREIGN KEY (wasm_component_id) REFERENCES wasm_component(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (wit_world_id) REFERENCES wit_world(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE UNIQUE INDEX uq_component_target ON component_target(
    wasm_component_id,
    declared_package,
    declared_world,
    COALESCE(declared_version, '')
);

-- ============================================================
-- INDEXES
--
-- Only created where no UNIQUE constraint or UNIQUE INDEX already
-- provides coverage via its leftmost column(s).
--
-- Coverage map (UNIQUE → leftmost-column prefix scans it serves):
--   _sync_meta        — PRIMARY KEY(key)
--   oci_repository    — UNIQUE(registry, repository)
--   oci_manifest      — UNIQUE(oci_repository_id, digest)
--   oci_manifest_ann  — UNIQUE(oci_manifest_id, key)
--   oci_tag           — UNIQUE(oci_repository_id, tag)
--   oci_layer         — UNIQUE(oci_manifest_id, digest)
--                        UNIQUE(oci_manifest_id, position)
--   oci_layer_ann     — UNIQUE(oci_layer_id, key)
--   oci_referrer      — UNIQUE(subject_manifest_id, referrer_manifest_id)
--   wit_package      — uq_wit_packages(package_name, ...)
--   wit_world         — UNIQUE(wit_package_id, name)
--   wit_world_import  — uq_wit_world_import(wit_world_id, ...)
--   wit_world_export  — uq_wit_world_export(wit_world_id, ...)
--   wit_package_dep   — uq_wit_package_dependency(dependent_id, ...)
--   wasm_component    — uq_wasm_component(oci_manifest_id, ...)
--   component_target  — uq_component_target(wasm_component_id, ...)
-- ============================================================

-- Cross-repo digest lookup: find a manifest by digest regardless
-- of which repository it belongs to.
CREATE INDEX idx_oci_manifest_digest ON oci_manifest(digest);
-- Filter manifests by artifact type, e.g. find all Wasm components
-- or all WIT package artifacts across the registry.
CREATE INDEX idx_oci_manifest_artifact_type ON oci_manifest(artifact_type);
-- Reverse tag lookup: find all tags that point to a given digest,
-- e.g. for displaying all names for a manifest.
CREATE INDEX idx_oci_tag_digest ON oci_tag(manifest_digest);
-- Search manifest annotations by key across all manifests,
-- e.g. "find all manifests with com.example.custom-key".
CREATE INDEX idx_oci_manifest_annotation_key ON oci_manifest_annotation(`key`);
-- Search layer annotations by key across all layers.
CREATE INDEX idx_oci_layer_annotation_key ON oci_layer_annotation(`key`);
-- Promoted OCI annotation columns: enable direct filtering
-- by version, vendor, or license without JSON parsing.
CREATE INDEX idx_oci_manifest_version ON oci_manifest(oci_version);
CREATE INDEX idx_oci_manifest_vendor ON oci_manifest(oci_vendor);
CREATE INDEX idx_oci_manifest_licenses ON oci_manifest(oci_licenses);
-- Filter referrers by (subject, artifact_type), e.g. "find all
-- Cosign signatures for this manifest".
CREATE INDEX idx_oci_referrer_type ON oci_referrer(subject_manifest_id, artifact_type);
-- Reverse referrer lookup: given a manifest, find all subjects it
-- refers to.  Needed for GC (is this manifest still a referrer?)
-- and bidirectional graph traversal.
CREATE INDEX idx_oci_referrer_referrer ON oci_referrer(referrer_manifest_id);
-- Exact (package_name, version) lookup on raw columns, without
-- the COALESCE overhead of uq_wit_packages.
CREATE INDEX idx_wit_package_name_version ON wit_package(package_name, version);
-- Find all WIT packages extracted from a given OCI manifest,
-- for provenance tracking and re-indexing.
CREATE INDEX idx_wit_package_provenance ON wit_package(oci_manifest_id);
-- Cross-package world name search, e.g. "find all worlds named
-- 'proxy' across all WIT packages".
CREATE INDEX idx_wit_world_name ON wit_world(name);
-- Reverse lookup on world imports: find all worlds that import
-- a given package (by declared name and version).
CREATE INDEX idx_world_import_declared ON wit_world_import(declared_package, declared_version);
-- Reverse lookup on resolved world imports: find all worlds
-- that resolved their import to a specific wit_package row.
CREATE INDEX idx_world_import_resolved ON wit_world_import(resolved_package_id);
-- Reverse lookup on world exports: find all worlds that export
-- a given package (by declared name and version).
CREATE INDEX idx_world_export_declared ON wit_world_export(declared_package, declared_version);
-- Reverse lookup on resolved world exports.
CREATE INDEX idx_world_export_resolved ON wit_world_export(resolved_package_id);
-- Reverse lookup on package dependencies: find all packages
-- that depend on a given package (by declared name and version).
CREATE INDEX idx_wit_dep_declared ON wit_package_dependency(declared_package, declared_version);
-- Reverse lookup on resolved package dependencies.
CREATE INDEX idx_wit_dep_resolved ON wit_package_dependency(resolved_package_id);
-- Search components by their human-readable name.
CREATE INDEX idx_wasm_component_name ON wasm_component(name);
-- Reverse lookup on component targets: find all components that
-- target a given world (by declared package, world name, version).
CREATE INDEX idx_target_declared ON component_target(declared_package, declared_world, declared_version);
-- Reverse lookup on resolved component targets: find all components
-- that resolved their target to a specific wit_world row.
CREATE INDEX idx_target_resolved ON component_target(wit_world_id);
