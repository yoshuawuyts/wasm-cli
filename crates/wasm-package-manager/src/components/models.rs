use rusqlite::Connection;

// ---------------------------------------------------------------------------
// WasmComponent
// ---------------------------------------------------------------------------

/// A compiled WebAssembly component stored in the registry.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct WasmComponent {
    id: i64,
    /// Foreign key to `oci_manifest`.
    pub oci_manifest_id: i64,
    /// Foreign key to `oci_layer`, if the component maps to a specific layer.
    pub oci_layer_id: Option<i64>,
    /// Optional human-readable name.
    pub name: Option<String>,
    /// Optional description.
    pub description: Option<String>,
    /// When this row was created.
    pub created_at: String,
}

impl WasmComponent {
    /// Returns the primary-key ID.
    #[must_use]
    #[allow(unreachable_pub)]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Insert a new component, returning its row ID.
    ///
    /// Uses `INSERT … ON CONFLICT DO NOTHING` so duplicate
    /// (oci_manifest_id, oci_layer_id) tuples are idempotent.
    pub(crate) fn insert(
        conn: &Connection,
        oci_manifest_id: i64,
        oci_layer_id: Option<i64>,
        name: Option<&str>,
        description: Option<&str>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO wasm_component
                 (oci_manifest_id, oci_layer_id, name, description)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT DO NOTHING",
            rusqlite::params![oci_manifest_id, oci_layer_id, name, description],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM wasm_component
             WHERE oci_manifest_id = ?1
               AND COALESCE(oci_layer_id, -1) = COALESCE(?2, -1)",
            rusqlite::params![oci_manifest_id, oci_layer_id],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// Find the component associated with a given OCI manifest.
    #[allow(dead_code)]
    pub(crate) fn find_by_manifest(
        conn: &Connection,
        oci_manifest_id: i64,
    ) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, oci_manifest_id, oci_layer_id, name, description, created_at
             FROM wasm_component
             WHERE oci_manifest_id = ?1",
            [oci_manifest_id],
            Self::from_row,
        );

        match result {
            Ok(comp) => Ok(Some(comp)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List every component in the database.
    #[allow(dead_code)]
    pub(crate) fn list_all(conn: &Connection) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, oci_manifest_id, oci_layer_id, name, description, created_at
             FROM wasm_component
             ORDER BY name ASC, created_at ASC",
        )?;

        let rows = stmt.query_map([], Self::from_row)?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Map a `rusqlite::Row` to `Self`.
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            oci_manifest_id: row.get(1)?,
            oci_layer_id: row.get(2)?,
            name: row.get(3)?,
            description: row.get(4)?,
            created_at: row.get(5)?,
        })
    }
}

// ---------------------------------------------------------------------------
// ComponentTarget
// ---------------------------------------------------------------------------

/// A world that a Wasm component targets.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct ComponentTarget {
    id: i64,
    /// Foreign key to `wasm_component`.
    pub wasm_component_id: i64,
    /// Declared package name of the target world.
    pub declared_package: String,
    /// Declared world name.
    pub declared_world: String,
    /// Declared version constraint, if any.
    pub declared_version: Option<String>,
    /// Resolved foreign key to `wit_world`, if matched.
    pub wit_world_id: Option<i64>,
}

impl ComponentTarget {
    /// Returns the primary-key ID.
    #[must_use]
    #[allow(unreachable_pub)]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Insert a new component target, returning its row ID.
    pub(crate) fn insert(
        conn: &Connection,
        wasm_component_id: i64,
        declared_package: &str,
        declared_world: &str,
        declared_version: Option<&str>,
        wit_world_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO component_target
                 (wasm_component_id, declared_package, declared_world,
                  declared_version, wit_world_id)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT DO NOTHING",
            rusqlite::params![
                wasm_component_id,
                declared_package,
                declared_world,
                declared_version,
                wit_world_id,
            ],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM component_target
             WHERE wasm_component_id = ?1
               AND declared_package = ?2
               AND declared_world = ?3
               AND COALESCE(declared_version, '') = COALESCE(?4, '')",
            rusqlite::params![
                wasm_component_id,
                declared_package,
                declared_world,
                declared_version,
            ],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// List all targets for a given component.
    #[allow(dead_code)]
    pub(crate) fn list_by_component(
        conn: &Connection,
        wasm_component_id: i64,
    ) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, wasm_component_id, declared_package, declared_world,
                    declared_version, wit_world_id
             FROM component_target
             WHERE wasm_component_id = ?1
             ORDER BY declared_package ASC, declared_world ASC",
        )?;

        let rows = stmt.query_map([wasm_component_id], |row| {
            Ok(Self {
                id: row.get(0)?,
                wasm_component_id: row.get(1)?,
                declared_package: row.get(2)?,
                declared_world: row.get(3)?,
                declared_version: row.get(4)?,
                wit_world_id: row.get(5)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oci::{OciManifest, OciRepository};
    use crate::storage::Migrations;
    use std::collections::HashMap;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();
        conn
    }

    fn insert_test_manifest(conn: &Connection) -> i64 {
        let repo_id = OciRepository::upsert(conn, "ghcr.io", "user/repo").unwrap();
        let (mid, _) = OciManifest::upsert(
            conn,
            repo_id,
            "sha256:abc",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();
        mid
    }

    // r[verify component.insert]
    #[test]
    fn test_wasm_component_insert_and_find() {
        let conn = setup_test_db();
        let mid = insert_test_manifest(&conn);

        let id = WasmComponent::insert(&conn, mid, None, Some("my-comp"), Some("A test component"))
            .unwrap();
        assert!(id > 0);

        let comp = WasmComponent::find_by_manifest(&conn, mid)
            .unwrap()
            .unwrap();
        assert_eq!(comp.id(), id);
        assert_eq!(comp.name.as_deref(), Some("my-comp"));
        assert_eq!(comp.description.as_deref(), Some("A test component"));
    }

    // r[verify component.insert-idempotent]
    #[test]
    fn test_wasm_component_insert_idempotent() {
        let conn = setup_test_db();
        let mid = insert_test_manifest(&conn);

        let id1 = WasmComponent::insert(&conn, mid, None, Some("comp"), None).unwrap();
        let id2 = WasmComponent::insert(&conn, mid, None, Some("comp"), None).unwrap();
        assert_eq!(id1, id2);
    }

    // r[verify component.find-not-found]
    #[test]
    fn test_wasm_component_find_not_found() {
        let conn = setup_test_db();
        let result = WasmComponent::find_by_manifest(&conn, 9999).unwrap();
        assert!(result.is_none());
    }

    // r[verify component.list-all]
    #[test]
    fn test_wasm_component_list_all() {
        let conn = setup_test_db();
        let empty = WasmComponent::list_all(&conn).unwrap();
        assert!(empty.is_empty());

        let mid = insert_test_manifest(&conn);
        WasmComponent::insert(&conn, mid, None, Some("beta"), None).unwrap();

        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "other/repo").unwrap();
        let (mid2, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:def",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();
        WasmComponent::insert(&conn, mid2, None, Some("alpha"), None).unwrap();

        let all = WasmComponent::list_all(&conn).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name.as_deref(), Some("alpha"));
        assert_eq!(all[1].name.as_deref(), Some("beta"));
    }

    // r[verify component-target.insert]
    #[test]
    fn test_component_target_insert_and_list() {
        let conn = setup_test_db();
        let mid = insert_test_manifest(&conn);
        let comp_id = WasmComponent::insert(&conn, mid, None, Some("comp"), None).unwrap();

        let tid =
            ComponentTarget::insert(&conn, comp_id, "wasi:http", "proxy", Some("0.2.0"), None)
                .unwrap();
        assert!(tid > 0);

        ComponentTarget::insert(&conn, comp_id, "wasi:cli", "command", None, None).unwrap();

        let targets = ComponentTarget::list_by_component(&conn, comp_id).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].declared_package, "wasi:cli");
        assert_eq!(targets[0].declared_world, "command");
        assert_eq!(targets[1].declared_package, "wasi:http");
        assert_eq!(targets[1].declared_world, "proxy");
        assert_eq!(targets[1].declared_version.as_deref(), Some("0.2.0"));
    }

    // r[verify component-target.insert-idempotent]
    #[test]
    fn test_component_target_insert_idempotent() {
        let conn = setup_test_db();
        let mid = insert_test_manifest(&conn);
        let comp_id = WasmComponent::insert(&conn, mid, None, Some("comp"), None).unwrap();

        let id1 =
            ComponentTarget::insert(&conn, comp_id, "wasi:http", "proxy", Some("0.2.0"), None)
                .unwrap();
        let id2 =
            ComponentTarget::insert(&conn, comp_id, "wasi:http", "proxy", Some("0.2.0"), None)
                .unwrap();
        assert_eq!(id1, id2);
    }

    // r[verify component-target.list-empty]
    #[test]
    fn test_component_target_list_empty() {
        let conn = setup_test_db();
        let targets = ComponentTarget::list_by_component(&conn, 9999).unwrap();
        assert!(targets.is_empty());
    }
}
