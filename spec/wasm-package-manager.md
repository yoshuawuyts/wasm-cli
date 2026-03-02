# wasm-package-manager Specification

This document defines the requirements for the `wasm-package-manager` library
crate. Requirements are derived from the existing test suite.

## Configuration

The `config` module manages global and local configuration files.

r[config.default]
A default configuration MUST be constructable.

r[config.load-missing]
Loading a nonexistent config file MUST succeed gracefully.

r[config.load-valid]
Loading a valid config file MUST return the correct settings.

r[config.ensure-exists]
`ensure_exists` MUST create the config file if it is missing.

r[config.ensure-idempotent]
`ensure_exists` MUST be idempotent.

r[config.credentials.cache]
Credential caching MUST work correctly.

r[config.credentials.no-helper]
Missing credential helpers MUST be handled gracefully.

r[config.local-overrides]
Local configuration MUST override global configuration.

## Credential Helper

The credential helper subsystem extracts credentials for OCI registries.

r[credential.json]
JSON credential helpers MUST be executed and parsed correctly.

r[credential.split]
Split credential helpers MUST be executed correctly.

r[credential.no-leak-debug]
Debug output MUST never print credentials.

r[credential.no-leak-display]
Display output MUST never leak credentials.

## OCI Storage

The OCI storage layer persists OCI registry data in SQLite.

### Repository and Manifest

r[oci.repository.upsert-and-find]
Upserting an OCI repository MUST allow retrieving it.

r[oci.repository.upsert-idempotent]
Upserting an OCI repository MUST be idempotent.

r[oci.manifest.upsert]
Upserting an OCI manifest MUST store and retrieve correctly.

r[oci.manifest.annotations]
Manifest upsert MUST extract and store annotations.

r[oci.manifest.config-fields]
Manifest upsert MUST store config fields.

r[oci.manifest.placeholder-upgrade]
Upserting a manifest over a placeholder MUST upgrade it with full data.

r[oci.manifest.cascade-delete]
Deleting a manifest MUST cascade to layers, annotations, and referrers.

### Tags

r[oci.tag.upsert]
Upserting an OCI tag MUST be idempotent.

### Layers

r[oci.layer.insert]
Inserting OCI layers MUST allow listing them afterward.

r[oci.layer.annotations]
Layer annotations MUST be insertable and listable.

r[oci.layer.annotation-conflict]
Layer annotation upsert MUST handle conflicts.

r[oci.layer.annotation-cascade]
Deleting a layer MUST cascade to its annotations.

### Referrers

r[oci.referrer.insert]
OCI referrers MUST be insertable and listable.

r[oci.referrer.idempotent]
Referrer insertion MUST be idempotent.

r[oci.referrer.cascade-delete]
Deleting a manifest MUST cascade to its referrer relationships.

### Tag Classification

r[oci.tags.classify-release]
Release tags MUST be classified correctly.

r[oci.tags.classify-signature]
Signature tags MUST be classified correctly.

r[oci.tags.classify-attestation]
Attestation tags MUST be classified correctly.

r[oci.tags.classify-mixed]
Mixed tag lists MUST be classified correctly.

r[oci.tags.classify-empty]
Empty tag lists MUST be classified correctly.

r[oci.tags.classify-all-release]
Tag lists consisting entirely of release tags MUST be classified correctly.

### Layer Filtering

r[oci.layers.filter-mixed]
Filtering MUST separate WASM layers from non-WASM layers.

r[oci.layers.filter-none]
Filtering MUST handle layers with no WASM content.

r[oci.layers.filter-empty]
Filtering MUST handle an empty layer list.

r[oci.layers.cacache-roundtrip]
Data written to cacache with a layer digest key MUST be retrievable using the
digest obtained from `filter_wasm_layers`.

### Orphaned Layers

r[oci.layers.orphaned-disjoint]
Orphaned layer detection MUST work with disjoint layer sets.

r[oci.layers.orphaned-overlap]
Orphaned layer detection MUST work with overlapping layer sets.

r[oci.layers.orphaned-shared]
Orphaned layer detection MUST handle all-shared layers.

## WIT Storage

The WIT metadata storage layer persists WebAssembly Interface Types data.

r[wit.world.insert]
WIT worlds MUST be insertable and queryable.

r[wit.world.imports-exports]
WIT world imports and exports MUST be storable.

r[wit.world.idempotent]
Import and export operations MUST be idempotent.

r[wit.interface.dependencies]
WIT interface dependencies MUST be storable.

r[wit.component.insert]
WASM components and their targets MUST be storable.

r[wit.component.wit-only]
WIT-only packages MUST NOT create component rows.

### Foreign Key Resolution

r[wit.resolve.import]
Import resolution MUST populate `resolved_interface_id` when the dependency exists.

r[wit.resolve.import-missing]
Import resolution MUST leave the field NULL when the dependency is missing.

r[wit.resolve.dependency]
Dependency interface IDs MUST be resolvable.

r[wit.resolve.export]
Export interface IDs MUST be resolvable.

r[wit.resolve.component-target]
Component targets MUST be resolvable across packages.

## WIT Parsing

The WIT parser extracts interface metadata from WASM binaries.

r[wit.parse.invalid-bytes]
The parser MUST return `None` for invalid bytes.

r[wit.parse.empty-bytes]
The parser MUST return `None` for empty bytes.

r[wit.parse.core-module]
The parser MUST handle core WASM modules.

r[wit.parse.random-bytes]
The parser MUST return `None` for random data.

r[wit.parse.world-key-name]
World key names MUST be converted correctly.

r[wit.parse.world-key-interface]
Interface world keys MUST be converted correctly.

r[wit.parse.wit-text-package]
WIT text generation MUST work for WIT packages.

r[wit.parse.wit-text-component]
WIT text generation MUST work for components.

r[wit.parse.wit-text-imports-exports]
WIT text generation MUST include imports and exports.

r[wit.parse.multiple-worlds]
Extraction MUST handle packages with multiple worlds.

r[wit.parse.single-world]
Components MUST have exactly one world.

r[wit.parse.world-items]
World items with named and interface imports MUST be extracted.

r[wit.parse.exclude-primary]
Dependencies MUST exclude the primary package itself.

r[wit.parse.is-component]
The `is_component` flag MUST correctly distinguish WIT packages from components.

## WIT Detection

r[wit.detect.invalid]
Invalid bytes MUST NOT be detected as a WIT package.

r[wit.detect.empty]
Empty bytes MUST NOT be detected as a WIT package.

r[wit.detect.core-module]
Core modules MUST NOT be detected as WIT packages.

## Package Manager Logic

### Vendor Filenames

r[manager.vendor-filename.basic]
Vendor filenames MUST be generated from registry, repository, tag, and digest.

r[manager.vendor-filename.no-tag]
Vendor filenames MUST handle missing tags.

r[manager.vendor-filename.short-digest]
Vendor filenames MUST handle short digest lengths.

r[manager.vendor-filename.nested]
Vendor filenames MUST handle nested repository paths.

### Sync Scheduling

r[manager.sync.no-previous]
Sync MUST trigger when there is no previous sync time.

r[manager.sync.stale]
Sync MUST trigger when the sync interval has expired.

r[manager.sync.fresh]
Sync MUST NOT trigger when the sync interval has not expired.

### Name Sanitization

r[manager.name.sanitize.valid]
A valid identifier MUST pass through unchanged.

r[manager.name.sanitize.uppercase]
Uppercase characters MUST be lowercased.

r[manager.name.sanitize.underscores]
Underscores MUST be replaced with hyphens.

r[manager.name.sanitize.leading-digits]
Leading digits MUST be stripped.

### Name Derivation

r[manager.name.wit-package]
Name derivation MUST prefer the WIT package name.

r[manager.name.oci-title]
Name derivation MUST fall back to the OCI image title.

r[manager.name.last-segment]
Name derivation MUST fall back to the repository last segment.

r[manager.name.collision]
Name derivation MUST handle collisions.

## Database

### Migrations

r[db.migrations.create-tables]
Running all migrations MUST create the required database tables.

r[db.migrations.idempotent]
Running migrations MUST be idempotent.

r[db.migrations.info]
Migration info MUST be retrievable.

### Known Packages

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

## Formatting

r[format.size.bytes]
The `format_size` function MUST format byte-range sizes.

r[format.size.kilobytes]
The `format_size` function MUST format kilobyte-range sizes.

r[format.size.megabytes]
The `format_size` function MUST format megabyte-range sizes.

r[format.size.gigabytes]
The `format_size` function MUST format gigabyte-range sizes.
