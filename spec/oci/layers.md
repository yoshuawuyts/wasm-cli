# OCI Layers

## Layer CRUD

r[oci.layer.insert]
Inserting OCI layers MUST allow listing them afterward.

r[oci.layer.annotations]
Layer annotations MUST be insertable and listable.

r[oci.layer.annotation-conflict]
Layer annotation upsert MUST handle conflicts.

r[oci.layer.annotation-cascade]
Deleting a layer MUST cascade to its annotations.

## Layer Filtering

r[oci.layers.filter-mixed]
Filtering MUST separate WASM layers from non-WASM layers.

r[oci.layers.filter-none]
Filtering MUST handle layers with no WASM content.

r[oci.layers.filter-empty]
Filtering MUST handle an empty layer list.

## Layer Validation

r[oci.layers.reject-multi]
OCI bundles with more than one layer MUST be rejected.

r[oci.layers.require-wasm-content-type]
The single layer in an OCI bundle MUST have the `application/wasm` media type.

r[oci.layers.cacache-roundtrip]
Data written to cacache with a layer digest key MUST be retrievable using the
digest obtained from `filter_wasm_layers`.

## Orphaned Layers

r[oci.layers.orphaned-disjoint]
Orphaned layer detection MUST work with disjoint layer sets.

r[oci.layers.orphaned-overlap]
Orphaned layer detection MUST work with overlapping layer sets.

r[oci.layers.orphaned-shared]
Orphaned layer detection MUST handle all-shared layers.
