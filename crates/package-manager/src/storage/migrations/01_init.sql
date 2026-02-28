CREATE TABLE _sync_meta (
    `key` TEXT PRIMARY KEY NOT NULL,
    `value` TEXT NOT NULL
);
CREATE TABLE oci_repository (
    id INTEGER PRIMARY KEY,
    registry TEXT NOT NULL,
    repository TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(registry, repository)
);
CREATE TABLE oci_manifest (
    id INTEGER PRIMARY KEY,
    oci_repository_id INTEGER NOT NULL,
    digest TEXT NOT NULL,
    media_type TEXT,
    raw_json TEXT,
    size_bytes INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    artifact_type TEXT,
    config_media_type TEXT,
    config_digest TEXT,
    oci_created TEXT,
    oci_authors TEXT,
    oci_url TEXT,
    oci_documentation TEXT,
    oci_source TEXT,
    oci_version TEXT,
    oci_revision TEXT,
    oci_vendor TEXT,
    oci_licenses TEXT,
    oci_ref_name TEXT,
    oci_title TEXT,
    oci_description TEXT,
    oci_base_digest TEXT,
    oci_base_name TEXT,
    UNIQUE(oci_repository_id, digest),
    FOREIGN KEY (oci_repository_id) REFERENCES oci_repository(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);
CREATE TABLE oci_manifest_annotation (
    id INTEGER PRIMARY KEY,
    oci_manifest_id INTEGER NOT NULL,
    `key` TEXT NOT NULL,
    `value` TEXT NOT NULL,
    UNIQUE(oci_manifest_id, `key`),
    FOREIGN KEY (oci_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);
CREATE TABLE oci_tag (
    id INTEGER PRIMARY KEY,
    oci_repository_id INTEGER NOT NULL,
    manifest_digest TEXT NOT NULL,
    tag TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(oci_repository_id, tag),
    FOREIGN KEY (oci_repository_id) REFERENCES oci_repository(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (oci_repository_id, manifest_digest)
        REFERENCES oci_manifest(oci_repository_id, digest)
        ON UPDATE NO ACTION ON DELETE CASCADE
);
CREATE TABLE oci_layer (
    id INTEGER PRIMARY KEY,
    oci_manifest_id INTEGER NOT NULL,
    digest TEXT NOT NULL,
    media_type TEXT,
    size_bytes INTEGER,
    position INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (oci_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);
CREATE TABLE oci_layer_annotation (
    id INTEGER PRIMARY KEY,
    oci_layer_id INTEGER NOT NULL,
    `key` TEXT NOT NULL,
    `value` TEXT NOT NULL,
    UNIQUE(oci_layer_id, `key`),
    FOREIGN KEY (oci_layer_id) REFERENCES oci_layer(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);
CREATE TABLE oci_referrer (
    id INTEGER PRIMARY KEY,
    subject_manifest_id INTEGER NOT NULL,
    referrer_manifest_id INTEGER NOT NULL,
    artifact_type TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(subject_manifest_id, referrer_manifest_id),
    FOREIGN KEY (subject_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (referrer_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);
CREATE TABLE wit_interface (
    id INTEGER PRIMARY KEY,
    package_name TEXT NOT NULL,
    version TEXT,
    description TEXT,
    wit_text TEXT,
    oci_manifest_id INTEGER,
    oci_layer_id INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (oci_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE SET NULL,
    FOREIGN KEY (oci_layer_id) REFERENCES oci_layer(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);
CREATE TABLE wit_world (
    id INTEGER PRIMARY KEY,
    wit_interface_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(wit_interface_id, name),
    FOREIGN KEY (wit_interface_id) REFERENCES wit_interface(id)
        ON UPDATE NO ACTION ON DELETE CASCADE
);
CREATE TABLE wit_world_import (
    id INTEGER PRIMARY KEY,
    wit_world_id INTEGER NOT NULL,
    declared_package TEXT NOT NULL,
    declared_interface TEXT,
    declared_version TEXT,
    resolved_interface_id INTEGER,
    FOREIGN KEY (wit_world_id) REFERENCES wit_world(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (resolved_interface_id) REFERENCES wit_interface(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);
CREATE TABLE wit_world_export (
    id INTEGER PRIMARY KEY,
    wit_world_id INTEGER NOT NULL,
    declared_package TEXT NOT NULL,
    declared_interface TEXT,
    declared_version TEXT,
    resolved_interface_id INTEGER,
    FOREIGN KEY (wit_world_id) REFERENCES wit_world(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (resolved_interface_id) REFERENCES wit_interface(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);
CREATE TABLE wit_interface_dependency (
    id INTEGER PRIMARY KEY,
    dependent_id INTEGER NOT NULL,
    declared_package TEXT NOT NULL,
    declared_version TEXT,
    resolved_interface_id INTEGER,
    FOREIGN KEY (dependent_id) REFERENCES wit_interface(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (resolved_interface_id) REFERENCES wit_interface(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);
CREATE TABLE wasm_component (
    id INTEGER PRIMARY KEY,
    oci_manifest_id INTEGER NOT NULL,
    oci_layer_id INTEGER,
    name TEXT,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (oci_manifest_id) REFERENCES oci_manifest(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (oci_layer_id) REFERENCES oci_layer(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);
CREATE TABLE component_target (
    id INTEGER PRIMARY KEY,
    wasm_component_id INTEGER NOT NULL,
    declared_package TEXT NOT NULL,
    declared_world TEXT NOT NULL,
    declared_version TEXT,
    wit_world_id INTEGER,
    FOREIGN KEY (wasm_component_id) REFERENCES wasm_component(id)
        ON UPDATE NO ACTION ON DELETE CASCADE,
    FOREIGN KEY (wit_world_id) REFERENCES wit_world(id)
        ON UPDATE NO ACTION ON DELETE SET NULL
);
CREATE TRIGGER trg_oci_repository_updated_at
    AFTER UPDATE ON oci_repository
    FOR EACH ROW
    WHEN OLD.updated_at = NEW.updated_at
    BEGIN
        UPDATE oci_repository
           SET updated_at = CURRENT_TIMESTAMP
         WHERE id = OLD.id;
    END;
CREATE TRIGGER trg_oci_tag_updated_at
    AFTER UPDATE ON oci_tag
    FOR EACH ROW
    WHEN OLD.updated_at = NEW.updated_at
    BEGIN
        UPDATE oci_tag
           SET updated_at = CURRENT_TIMESTAMP
         WHERE id = OLD.id;
    END;
CREATE UNIQUE INDEX uq_oci_layer_digest ON oci_layer(oci_manifest_id, digest);
CREATE UNIQUE INDEX uq_oci_layer_position ON oci_layer(oci_manifest_id, position);
CREATE UNIQUE INDEX uq_wit_interface ON wit_interface(
    package_name,
    COALESCE(version, ''),
    COALESCE(oci_layer_id, -1)
);
CREATE UNIQUE INDEX uq_wit_world_import ON wit_world_import(
    wit_world_id,
    declared_package,
    COALESCE(declared_interface, ''),
    COALESCE(declared_version, '')
);
CREATE UNIQUE INDEX uq_wit_world_export ON wit_world_export(
    wit_world_id,
    declared_package,
    COALESCE(declared_interface, ''),
    COALESCE(declared_version, '')
);
CREATE UNIQUE INDEX uq_wit_interface_dependency ON wit_interface_dependency(
    dependent_id,
    declared_package,
    COALESCE(declared_version, '')
);
CREATE UNIQUE INDEX uq_wasm_component ON wasm_component(
    oci_manifest_id,
    COALESCE(oci_layer_id, -1)
);
CREATE UNIQUE INDEX uq_component_target ON component_target(
    wasm_component_id,
    declared_package,
    declared_world,
    COALESCE(declared_version, '')
);
CREATE INDEX idx_oci_manifest_digest ON oci_manifest(digest);
CREATE INDEX idx_oci_manifest_artifact_type ON oci_manifest(artifact_type);
CREATE INDEX idx_oci_tag_digest ON oci_tag(manifest_digest);
CREATE INDEX idx_oci_manifest_annotation_key ON oci_manifest_annotation(`key`);
CREATE INDEX idx_oci_layer_annotation_key ON oci_layer_annotation(`key`);
CREATE INDEX idx_oci_manifest_version ON oci_manifest(oci_version);
CREATE INDEX idx_oci_manifest_vendor ON oci_manifest(oci_vendor);
CREATE INDEX idx_oci_manifest_licenses ON oci_manifest(oci_licenses);
CREATE INDEX idx_oci_referrer_type ON oci_referrer(subject_manifest_id, artifact_type);
CREATE INDEX idx_oci_referrer_referrer ON oci_referrer(referrer_manifest_id);
CREATE INDEX idx_wit_iface_name_version ON wit_interface(package_name, version);
CREATE INDEX idx_wit_iface_provenance ON wit_interface(oci_manifest_id);
CREATE INDEX idx_wit_world_name ON wit_world(name);
CREATE INDEX idx_world_import_declared ON wit_world_import(declared_package, declared_version);
CREATE INDEX idx_world_import_resolved ON wit_world_import(resolved_interface_id);
CREATE INDEX idx_world_export_declared ON wit_world_export(declared_package, declared_version);
CREATE INDEX idx_world_export_resolved ON wit_world_export(resolved_interface_id);
CREATE INDEX idx_wit_dep_declared ON wit_interface_dependency(declared_package, declared_version);
CREATE INDEX idx_wit_dep_resolved ON wit_interface_dependency(resolved_interface_id);
CREATE INDEX idx_wasm_component_name ON wasm_component(name);
CREATE INDEX idx_target_declared ON component_target(declared_package, declared_world, declared_version);
CREATE INDEX idx_target_resolved ON component_target(wit_world_id);
