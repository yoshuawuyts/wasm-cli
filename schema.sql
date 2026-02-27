-- ============================================================
-- OCI-based Wasm Component Registry — SQLite3 Schema
-- ============================================================

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS "migrations" (
    "id"         INTEGER PRIMARY KEY,
    "version"    INTEGER NOT NULL UNIQUE,
    "applied_at" TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- ============================================================
-- OCI LAYER: Repositories, Manifests, Tags, Layers, Referrers
-- ============================================================

CREATE TABLE IF NOT EXISTS "oci_repository" (
    "id"          INTEGER PRIMARY KEY,
    "registry"    TEXT NOT NULL,
    "repository"  TEXT NOT NULL,
    "created_at"  TEXT NOT NULL DEFAULT (datetime('now')),
    "updated_at"  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE("registry", "repository")
);

CREATE TRIGGER IF NOT EXISTS "trg_oci_repository_updated_at"
    AFTER UPDATE ON "oci_repository"
    FOR EACH ROW
    WHEN OLD."updated_at" = NEW."updated_at"
    BEGIN
        UPDATE "oci_repository"
        SET "updated_at" = datetime('now')
        WHERE "id" = OLD."id";
    END;

-- Well-known OCI annotations promoted to columns.
-- See: https://specs.opencontainers.org/image-spec/annotations/
CREATE TABLE IF NOT EXISTS "oci_manifest" (
    "id"                INTEGER PRIMARY KEY,
    "oci_repository_id" INTEGER NOT NULL,
    "digest"            TEXT    NOT NULL,
    "media_type"        TEXT,
    "raw_json"          TEXT,
    "size_bytes"        INTEGER,                          -- NULL = unknown, 0 = genuinely zero
    "created_at"        TEXT    NOT NULL DEFAULT (datetime('now')),

    -- Top-level OCI manifest fields (not annotations) ────
    "artifact_type"     TEXT,    -- top-level artifactType (primary type dispatch key)
    "config_media_type" TEXT,    -- config descriptor mediaType
    "config_digest"     TEXT,    -- config descriptor digest

    -- OCI well-known annotations ─────────────────────────
    "oci_created"       TEXT,    -- org.opencontainers.image.created
    "oci_authors"       TEXT,    -- org.opencontainers.image.authors
    "oci_url"           TEXT,    -- org.opencontainers.image.url
    "oci_documentation" TEXT,    -- org.opencontainers.image.documentation
    "oci_source"        TEXT,    -- org.opencontainers.image.source
    "oci_version"       TEXT,    -- org.opencontainers.image.version
    "oci_revision"      TEXT,    -- org.opencontainers.image.revision
    "oci_vendor"        TEXT,    -- org.opencontainers.image.vendor
    "oci_licenses"      TEXT,    -- org.opencontainers.image.licenses
    "oci_ref_name"      TEXT,    -- org.opencontainers.image.ref.name
    "oci_title"         TEXT,    -- org.opencontainers.image.title
    "oci_description"   TEXT,    -- org.opencontainers.image.description
    "oci_base_digest"   TEXT,    -- org.opencontainers.image.base.digest
    "oci_base_name"     TEXT,    -- org.opencontainers.image.base.name

    UNIQUE("oci_repository_id", "digest"),
    FOREIGN KEY ("oci_repository_id") REFERENCES "oci_repository"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- Overflow for non-standard / vendor manifest annotations.
CREATE TABLE IF NOT EXISTS "oci_manifest_annotation" (
    "id"              INTEGER PRIMARY KEY,
    "oci_manifest_id" INTEGER NOT NULL,
    "key"             TEXT    NOT NULL,
    "value"           TEXT    NOT NULL,
    UNIQUE("oci_manifest_id", "key"),
    FOREIGN KEY ("oci_manifest_id") REFERENCES "oci_manifest"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- Tags are mutable pointers to manifests within the SAME repository.
--
-- The composite FK on (oci_repository_id, manifest_digest) referencing
-- oci_manifest(oci_repository_id, digest) guarantees that the tag and
-- manifest share the same repository.  There is intentionally no
-- separate oci_manifest_id column — join back to oci_manifest via
-- (oci_repository_id, manifest_digest) to obtain the surrogate id
-- when needed.
CREATE TABLE IF NOT EXISTS "oci_tag" (
    "id"                INTEGER PRIMARY KEY,
    "oci_repository_id" INTEGER NOT NULL,
    "manifest_digest"   TEXT    NOT NULL,
    "tag"               TEXT    NOT NULL,
    "created_at"        TEXT    NOT NULL DEFAULT (datetime('now')),
    "updated_at"        TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE("oci_repository_id", "tag"),
    FOREIGN KEY ("oci_repository_id") REFERENCES "oci_repository"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY ("oci_repository_id", "manifest_digest")
        REFERENCES "oci_manifest"("oci_repository_id", "digest")
        ON UPDATE NO ACTION ON DELETE CASCADE
);

CREATE TRIGGER IF NOT EXISTS "trg_oci_tag_updated_at"
    AFTER UPDATE ON "oci_tag"
    FOR EACH ROW
    WHEN OLD."updated_at" = NEW."updated_at"
    BEGIN
        UPDATE "oci_tag"
        SET "updated_at" = datetime('now')
        WHERE "id" = OLD."id";
    END;

CREATE TABLE IF NOT EXISTS "oci_layer" (
    "id"              INTEGER PRIMARY KEY,
    "oci_manifest_id" INTEGER NOT NULL,
    "digest"          TEXT    NOT NULL,
    "media_type"      TEXT,
    "size_bytes"      INTEGER,                            -- NULL = unknown
    "position"        INTEGER NOT NULL DEFAULT 0,
    UNIQUE("oci_manifest_id", "digest"),
    UNIQUE("oci_manifest_id", "position"),
    FOREIGN KEY ("oci_manifest_id") REFERENCES "oci_manifest"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- Overflow for non-standard / vendor layer annotations.
CREATE TABLE IF NOT EXISTS "oci_layer_annotation" (
    "id"           INTEGER PRIMARY KEY,
    "oci_layer_id" INTEGER NOT NULL,
    "key"          TEXT    NOT NULL,
    "value"        TEXT    NOT NULL,
    UNIQUE("oci_layer_id", "key"),
    FOREIGN KEY ("oci_layer_id") REFERENCES "oci_layer"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS "oci_referrer" (
    "id"                   INTEGER PRIMARY KEY,
    "subject_manifest_id"  INTEGER NOT NULL,
    "referrer_manifest_id" INTEGER NOT NULL,
    "artifact_type"        TEXT    NOT NULL,
    "created_at"           TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE("subject_manifest_id", "referrer_manifest_id"),
    FOREIGN KEY ("subject_manifest_id")  REFERENCES "oci_manifest"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY ("referrer_manifest_id") REFERENCES "oci_manifest"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- ============================================================
-- WIT LAYER
--
-- A WIT interface is the top-level publishable artifact.
-- It declares a package name + version (e.g. "wasi:http@0.3.0")
-- and contains named worlds.
--
-- The same (package_name, version) may exist from multiple OCI
-- sources — identity is nominal, not structural.
-- ============================================================

CREATE TABLE IF NOT EXISTS "wit_interface" (
    "id"              INTEGER PRIMARY KEY,
    "package_name"    TEXT NOT NULL,                       -- e.g. "wasi:http"
    "version"         TEXT,                                -- e.g. "0.3.0"
    "description"     TEXT,
    "wit_text"        TEXT,                                -- full WIT source

    -- Provenance: which OCI artifact was this extracted from?
    "oci_manifest_id" INTEGER,
    "oci_layer_id"    INTEGER,

    "created_at"      TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY ("oci_manifest_id") REFERENCES "oci_manifest"("id")
        ON UPDATE NO ACTION ON DELETE SET NULL,
    FOREIGN KEY ("oci_layer_id")    REFERENCES "oci_layer"("id")
        ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "uq_wit_interface"
    ON "wit_interface"(
        "package_name",
        COALESCE("version", ''),
        COALESCE("oci_layer_id", -1)
    );

-- A named world defined inside a wit_interface.
-- e.g. "proxy" inside wasi:http → "wasi:http/proxy@0.3.0"
CREATE TABLE IF NOT EXISTS "wit_world" (
    "id"              INTEGER PRIMARY KEY,
    "wit_interface_id" INTEGER NOT NULL,
    "name"            TEXT    NOT NULL,                    -- e.g. "proxy", "command"
    "description"     TEXT,
    "created_at"      TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE("wit_interface_id", "name"),
    FOREIGN KEY ("wit_interface_id") REFERENCES "wit_interface"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE
);

-- A world imports interfaces, referenced nominally.
-- resolved_interface_id is optional (NULL = not yet resolved
-- or ambiguous across sources).
CREATE TABLE IF NOT EXISTS "wit_world_import" (
    "id"                    INTEGER PRIMARY KEY,
    "wit_world_id"          INTEGER NOT NULL,
    "declared_package"      TEXT    NOT NULL,              -- e.g. "wasi:io"
    "declared_interface"    TEXT,                          -- e.g. "streams" (NULL = whole-package)
    "declared_version"      TEXT,                          -- e.g. "0.2.2"
    "resolved_interface_id" INTEGER,                       -- → wit_interface (nullable)

    FOREIGN KEY ("wit_world_id")          REFERENCES "wit_world"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY ("resolved_interface_id") REFERENCES "wit_interface"("id")
        ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "uq_wit_world_import"
    ON "wit_world_import"(
        "wit_world_id",
        "declared_package",
        COALESCE("declared_interface", ''),
        COALESCE("declared_version", '')
    );

-- A world exports interfaces, same nominal semantics.
CREATE TABLE IF NOT EXISTS "wit_world_export" (
    "id"                    INTEGER PRIMARY KEY,
    "wit_world_id"          INTEGER NOT NULL,
    "declared_package"      TEXT    NOT NULL,
    "declared_interface"    TEXT,
    "declared_version"      TEXT,
    "resolved_interface_id" INTEGER,

    FOREIGN KEY ("wit_world_id")          REFERENCES "wit_world"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY ("resolved_interface_id") REFERENCES "wit_interface"("id")
        ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "uq_wit_world_export"
    ON "wit_world_export"(
        "wit_world_id",
        "declared_package",
        COALESCE("declared_interface", ''),
        COALESCE("declared_version", '')
    );

-- Interface-level dependencies: one WIT interface depends on
-- another. e.g. wasi:http depends on wasi:io.
-- Nominal, with optional resolution.
CREATE TABLE IF NOT EXISTS "wit_interface_dependency" (
    "id"                    INTEGER PRIMARY KEY,
    "dependent_id"          INTEGER NOT NULL,
    "declared_package"      TEXT    NOT NULL,
    "declared_version"      TEXT,
    "resolved_interface_id" INTEGER,

    FOREIGN KEY ("dependent_id")          REFERENCES "wit_interface"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY ("resolved_interface_id") REFERENCES "wit_interface"("id")
        ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "uq_wit_interface_dependency"
    ON "wit_interface_dependency"(
        "dependent_id",
        "declared_package",
        COALESCE("declared_version", '')
    );

-- ============================================================
-- WASM LAYER: Components
-- ============================================================

CREATE TABLE IF NOT EXISTS "wasm_component" (
    "id"              INTEGER PRIMARY KEY,
    "oci_manifest_id" INTEGER NOT NULL,
    "oci_layer_id"    INTEGER,
    "name"            TEXT,
    "description"     TEXT,
    "created_at"      TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY ("oci_manifest_id") REFERENCES "oci_manifest"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY ("oci_layer_id")    REFERENCES "oci_layer"("id")
        ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "uq_wasm_component"
    ON "wasm_component"(
        "oci_manifest_id",
        COALESCE("oci_layer_id", -1)
    );

-- A component targets a world.  Nominal, with optional resolution.
CREATE TABLE IF NOT EXISTS "component_target" (
    "id"                INTEGER PRIMARY KEY,
    "wasm_component_id" INTEGER NOT NULL,
    "declared_package"  TEXT    NOT NULL,                  -- e.g. "wasi:http"
    "declared_world"    TEXT    NOT NULL,                  -- e.g. "proxy"
    "declared_version"  TEXT,                              -- e.g. "0.3.0"
    "wit_world_id"      INTEGER,                           -- resolved (nullable)

    FOREIGN KEY ("wasm_component_id") REFERENCES "wasm_component"("id")
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY ("wit_world_id")      REFERENCES "wit_world"("id")
        ON UPDATE NO ACTION ON DELETE SET NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS "uq_component_target"
    ON "component_target"(
        "wasm_component_id",
        "declared_package",
        "declared_world",
        COALESCE("declared_version", '')
    );

-- ============================================================
-- INDEXES
--
-- Only created where no UNIQUE constraint or UNIQUE INDEX already
-- provides coverage via its leftmost column(s).
--
-- Coverage map:
--   oci_repository    — UNIQUE(registry, repository)
--   oci_manifest      — UNIQUE(oci_repository_id, digest)
--   oci_manifest_ann  — UNIQUE(oci_manifest_id, key)
--   oci_tag           — UNIQUE(oci_repository_id, tag)
--   oci_layer         — UNIQUE(oci_manifest_id, digest)
--                        UNIQUE(oci_manifest_id, position)
--   oci_layer_ann     — UNIQUE(oci_layer_id, key)
--   oci_referrer      — UNIQUE(subject_manifest_id, referrer_manifest_id)
--   wit_interface     — uq_wit_interface(package_name, ...)
--   wit_world         — UNIQUE(wit_interface_id, name)
--   wit_world_import  — uq_wit_world_import(wit_world_id, ...)
--   wit_world_export  — uq_wit_world_export(wit_world_id, ...)
--   wit_interface_dep — uq_wit_interface_dependency(dependent_id, ...)
--   wasm_component    — uq_wasm_component(oci_manifest_id, ...)
--   component_target  — uq_component_target(wasm_component_id, ...)
-- ============================================================

-- oci_manifest: cross-repo digest lookup and artifact type filtering
CREATE INDEX IF NOT EXISTS "idx_oci_manifest_digest"
    ON "oci_manifest"("digest");
CREATE INDEX IF NOT EXISTS "idx_oci_manifest_artifact_type"
    ON "oci_manifest"("artifact_type");

-- oci_tag: reverse lookup ("which tags point to this digest?")
CREATE INDEX IF NOT EXISTS "idx_oci_tag_digest"
    ON "oci_tag"("manifest_digest");

-- oci_manifest_annotation: search by annotation key across manifests
CREATE INDEX IF NOT EXISTS "idx_oci_manifest_annotation_key"
    ON "oci_manifest_annotation"("key");

-- oci_layer_annotation: search by annotation key across layers
CREATE INDEX IF NOT EXISTS "idx_oci_layer_annotation_key"
    ON "oci_layer_annotation"("key");

-- OCI promoted annotation columns
CREATE INDEX IF NOT EXISTS "idx_oci_manifest_version"
    ON "oci_manifest"("oci_version");
CREATE INDEX IF NOT EXISTS "idx_oci_manifest_vendor"
    ON "oci_manifest"("oci_vendor");
CREATE INDEX IF NOT EXISTS "idx_oci_manifest_licenses"
    ON "oci_manifest"("oci_licenses");

-- oci_referrer: filter by (subject, artifact_type); reverse lookup by referrer
CREATE INDEX IF NOT EXISTS "idx_oci_referrer_type"
    ON "oci_referrer"("subject_manifest_id", "artifact_type");
CREATE INDEX IF NOT EXISTS "idx_oci_referrer_referrer"
    ON "oci_referrer"("referrer_manifest_id");

-- wit_interface: exact (package_name, version) without COALESCE;
-- provenance lookup by manifest
CREATE INDEX IF NOT EXISTS "idx_wit_iface_name_version"
    ON "wit_interface"("package_name", "version");
CREATE INDEX IF NOT EXISTS "idx_wit_iface_provenance"
    ON "wit_interface"("oci_manifest_id");

-- wit_world: cross-package world name search
CREATE INDEX IF NOT EXISTS "idx_wit_world_name"
    ON "wit_world"("name");

-- World imports/exports: reverse lookups by declared name and resolved id
CREATE INDEX IF NOT EXISTS "idx_world_import_declared"
    ON "wit_world_import"("declared_package", "declared_version");
CREATE INDEX IF NOT EXISTS "idx_world_import_resolved"
    ON "wit_world_import"("resolved_interface_id");
CREATE INDEX IF NOT EXISTS "idx_world_export_declared"
    ON "wit_world_export"("declared_package", "declared_version");
CREATE INDEX IF NOT EXISTS "idx_world_export_resolved"
    ON "wit_world_export"("resolved_interface_id");

-- Interface dependencies: reverse lookups
CREATE INDEX IF NOT EXISTS "idx_wit_dep_declared"
    ON "wit_interface_dependency"("declared_package", "declared_version");
CREATE INDEX IF NOT EXISTS "idx_wit_dep_resolved"
    ON "wit_interface_dependency"("resolved_interface_id");

-- wasm_component: search by name
CREATE INDEX IF NOT EXISTS "idx_wasm_component_name"
    ON "wasm_component"("name");

-- component_target: reverse lookups by declared world and resolved id
CREATE INDEX IF NOT EXISTS "idx_target_declared"
    ON "component_target"("declared_package", "declared_world", "declared_version");
CREATE INDEX IF NOT EXISTS "idx_target_resolved"
    ON "component_target"("wit_world_id");
