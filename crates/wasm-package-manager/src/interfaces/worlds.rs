use rusqlite::Connection;

// ---------------------------------------------------------------------------
// WitWorld
// ---------------------------------------------------------------------------

/// A world declared inside a WIT interface package.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct WitWorld {
    id: i64,
    /// Foreign key to `wit_interface`.
    pub wit_interface_id: i64,
    /// World name (e.g. "proxy", "command").
    pub name: String,
    /// Optional human-readable description.
    pub description: Option<String>,
    /// When this row was created.
    pub created_at: String,
}

impl WitWorld {
    /// Returns the primary-key ID.
    #[must_use]
    #[allow(unreachable_pub)]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Insert a new world, returning its row ID.
    pub(crate) fn insert(
        conn: &Connection,
        wit_interface_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO wit_world (wit_interface_id, name, description)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(wit_interface_id, name) DO NOTHING",
            rusqlite::params![wit_interface_id, name, description],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM wit_world WHERE wit_interface_id = ?1 AND name = ?2",
            rusqlite::params![wit_interface_id, name],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// List all worlds belonging to a given interface.
    #[allow(dead_code)]
    pub(crate) fn list_by_interface(
        conn: &Connection,
        wit_interface_id: i64,
    ) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, wit_interface_id, name, description, created_at
             FROM wit_world
             WHERE wit_interface_id = ?1
             ORDER BY name ASC",
        )?;

        let rows = stmt.query_map([wit_interface_id], |row| {
            Ok(Self {
                id: row.get(0)?,
                wit_interface_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Find a single world by interface ID and name.
    pub(crate) fn find_by_name(
        conn: &Connection,
        wit_interface_id: i64,
        name: &str,
    ) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, wit_interface_id, name, description, created_at
             FROM wit_world
             WHERE wit_interface_id = ?1 AND name = ?2",
            rusqlite::params![wit_interface_id, name],
            |row| {
                Ok(Self {
                    id: row.get(0)?,
                    wit_interface_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        );

        match result {
            Ok(world) => Ok(Some(world)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

// ---------------------------------------------------------------------------
// WitWorldImport
// ---------------------------------------------------------------------------

/// An import declaration inside a WIT world.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct WitWorldImport {
    id: i64,
    /// Foreign key to `wit_world`.
    pub wit_world_id: i64,
    /// Declared package name of the import.
    pub declared_package: String,
    /// Declared interface name, if any.
    pub declared_interface: Option<String>,
    /// Declared version constraint, if any.
    pub declared_version: Option<String>,
    /// Resolved foreign key to `wit_interface`, if matched.
    pub resolved_interface_id: Option<i64>,
}

impl WitWorldImport {
    /// Returns the primary-key ID.
    #[must_use]
    #[allow(unreachable_pub)]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Insert a new world import, returning its row ID.
    pub(crate) fn insert(
        conn: &Connection,
        wit_world_id: i64,
        declared_package: &str,
        declared_interface: Option<&str>,
        declared_version: Option<&str>,
        resolved_interface_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO wit_world_import
                 (wit_world_id, declared_package, declared_interface,
                  declared_version, resolved_interface_id)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT DO NOTHING",
            rusqlite::params![
                wit_world_id,
                declared_package,
                declared_interface,
                declared_version,
                resolved_interface_id,
            ],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM wit_world_import
             WHERE wit_world_id = ?1
               AND declared_package = ?2
               AND COALESCE(declared_interface, '') = COALESCE(?3, '')
               AND COALESCE(declared_version, '') = COALESCE(?4, '')",
            rusqlite::params![
                wit_world_id,
                declared_package,
                declared_interface,
                declared_version,
            ],
            |row| row.get(0),
        )?;

        Ok(id)
    }
}

// ---------------------------------------------------------------------------
// WitWorldExport
// ---------------------------------------------------------------------------

/// An export declaration inside a WIT world.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct WitWorldExport {
    id: i64,
    /// Foreign key to `wit_world`.
    pub wit_world_id: i64,
    /// Declared package name of the export.
    pub declared_package: String,
    /// Declared interface name, if any.
    pub declared_interface: Option<String>,
    /// Declared version constraint, if any.
    pub declared_version: Option<String>,
    /// Resolved foreign key to `wit_interface`, if matched.
    pub resolved_interface_id: Option<i64>,
}

impl WitWorldExport {
    /// Returns the primary-key ID.
    #[must_use]
    #[allow(unreachable_pub)]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Insert a new world export, returning its row ID.
    pub(crate) fn insert(
        conn: &Connection,
        wit_world_id: i64,
        declared_package: &str,
        declared_interface: Option<&str>,
        declared_version: Option<&str>,
        resolved_interface_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO wit_world_export
                 (wit_world_id, declared_package, declared_interface,
                  declared_version, resolved_interface_id)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT DO NOTHING",
            rusqlite::params![
                wit_world_id,
                declared_package,
                declared_interface,
                declared_version,
                resolved_interface_id,
            ],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM wit_world_export
             WHERE wit_world_id = ?1
               AND declared_package = ?2
               AND COALESCE(declared_interface, '') = COALESCE(?3, '')
               AND COALESCE(declared_version, '') = COALESCE(?4, '')",
            rusqlite::params![
                wit_world_id,
                declared_package,
                declared_interface,
                declared_version,
            ],
            |row| row.get(0),
        )?;

        Ok(id)
    }
}

// ---------------------------------------------------------------------------
// WitInterfaceDependency
// ---------------------------------------------------------------------------

/// A dependency edge between two WIT interface packages.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct WitInterfaceDependency {
    id: i64,
    /// The interface that *has* the dependency (foreign key to `wit_interface`).
    pub dependent_id: i64,
    /// Declared package name of the dependency.
    pub declared_package: String,
    /// Declared version constraint, if any.
    pub declared_version: Option<String>,
    /// Resolved foreign key to `wit_interface`, if matched.
    pub resolved_interface_id: Option<i64>,
}

impl WitInterfaceDependency {
    /// Returns the primary-key ID.
    #[must_use]
    #[allow(unreachable_pub)]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Insert a new interface dependency, returning its row ID.
    pub(crate) fn insert(
        conn: &Connection,
        dependent_id: i64,
        declared_package: &str,
        declared_version: Option<&str>,
        resolved_interface_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO wit_interface_dependency
                 (dependent_id, declared_package, declared_version, resolved_interface_id)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT DO NOTHING",
            rusqlite::params![
                dependent_id,
                declared_package,
                declared_version,
                resolved_interface_id,
            ],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM wit_interface_dependency
             WHERE dependent_id = ?1
               AND declared_package = ?2
               AND COALESCE(declared_version, '') = COALESCE(?3, '')",
            rusqlite::params![dependent_id, declared_package, declared_version],
            |row| row.get(0),
        )?;

        Ok(id)
    }
}
