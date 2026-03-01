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
