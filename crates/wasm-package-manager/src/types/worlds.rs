use rusqlite::Connection;

// ---------------------------------------------------------------------------
// WitWorld
// ---------------------------------------------------------------------------

/// A world declared inside a WIT package.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct WitWorld {
    id: i64,
    /// Foreign key to `wit_package`.
    pub wit_package_id: i64,
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
        wit_package_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO wit_world (wit_package_id, name, description)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(wit_package_id, name) DO NOTHING",
            rusqlite::params![wit_package_id, name, description],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM wit_world WHERE wit_package_id = ?1 AND name = ?2",
            rusqlite::params![wit_package_id, name],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// List all worlds belonging to a given package.
    #[allow(dead_code)]
    pub(crate) fn list_by_type(
        conn: &Connection,
        wit_package_id: i64,
    ) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, wit_package_id, name, description, created_at
             FROM wit_world
             WHERE wit_package_id = ?1
             ORDER BY name ASC",
        )?;

        let rows = stmt.query_map([wit_package_id], |row| {
            Ok(Self {
                id: row.get(0)?,
                wit_package_id: row.get(1)?,
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

    /// Find a single world by package ID and name.
    #[allow(dead_code)]
    pub(crate) fn find_by_name(
        conn: &Connection,
        wit_package_id: i64,
        name: &str,
    ) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, wit_package_id, name, description, created_at
             FROM wit_world
             WHERE wit_package_id = ?1 AND name = ?2",
            rusqlite::params![wit_package_id, name],
            |row| {
                Ok(Self {
                    id: row.get(0)?,
                    wit_package_id: row.get(1)?,
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
    /// Resolved foreign key to `wit_package`, if matched.
    pub resolved_package_id: Option<i64>,
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
        resolved_package_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO wit_world_import
                 (wit_world_id, declared_package, declared_interface,
                  declared_version, resolved_package_id)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT DO NOTHING",
            rusqlite::params![
                wit_world_id,
                declared_package,
                declared_interface,
                declared_version,
                resolved_package_id,
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
    /// Resolved foreign key to `wit_package`, if matched.
    pub resolved_package_id: Option<i64>,
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
        resolved_package_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO wit_world_export
                 (wit_world_id, declared_package, declared_interface,
                  declared_version, resolved_package_id)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT DO NOTHING",
            rusqlite::params![
                wit_world_id,
                declared_package,
                declared_interface,
                declared_version,
                resolved_package_id,
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
// WitPackageDependency
// ---------------------------------------------------------------------------

/// A dependency edge between two WIT packages.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct WitPackageDependency {
    id: i64,
    /// The package that *has* the dependency (foreign key to `wit_package`).
    pub dependent_id: i64,
    /// Declared package name of the dependency.
    pub declared_package: String,
    /// Declared version constraint, if any.
    pub declared_version: Option<String>,
    /// Resolved foreign key to `wit_package`, if matched.
    pub resolved_package_id: Option<i64>,
}

impl WitPackageDependency {
    /// Returns the primary-key ID.
    #[must_use]
    #[allow(unreachable_pub)]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Insert a new package dependency, returning its row ID.
    pub(crate) fn insert(
        conn: &Connection,
        dependent_id: i64,
        declared_package: &str,
        declared_version: Option<&str>,
        resolved_package_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO wit_package_dependency
                 (dependent_id, declared_package, declared_version, resolved_package_id)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT DO NOTHING",
            rusqlite::params![
                dependent_id,
                declared_package,
                declared_version,
                resolved_package_id,
            ],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM wit_package_dependency
             WHERE dependent_id = ?1
               AND declared_package = ?2
               AND COALESCE(declared_version, '') = COALESCE(?3, '')",
            rusqlite::params![dependent_id, declared_package, declared_version],
            |row| row.get(0),
        )?;

        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Migrations;
    use crate::types::RawWitPackage;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();
        conn
    }

    // r[verify wit-world.insert]
    #[test]
    fn test_wit_world_insert_and_find() {
        let conn = setup_test_db();
        let pkg_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();

        let world_id =
            WitWorld::insert(&conn, pkg_id, "proxy", Some("An HTTP proxy world")).unwrap();
        assert!(world_id > 0);

        let found = WitWorld::find_by_name(&conn, pkg_id, "proxy")
            .unwrap()
            .unwrap();
        assert_eq!(found.id(), world_id);
        assert_eq!(found.name, "proxy");
        assert_eq!(found.description.as_deref(), Some("An HTTP proxy world"));
    }

    // r[verify wit-world.insert-idempotent]
    #[test]
    fn test_wit_world_insert_idempotent() {
        let conn = setup_test_db();
        let pkg_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();

        let id1 = WitWorld::insert(&conn, pkg_id, "proxy", None).unwrap();
        let id2 = WitWorld::insert(&conn, pkg_id, "proxy", None).unwrap();
        assert_eq!(id1, id2);
    }

    // r[verify wit-world.find-not-found]
    #[test]
    fn test_wit_world_find_not_found() {
        let conn = setup_test_db();
        let result = WitWorld::find_by_name(&conn, 9999, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    // r[verify wit-world.list-by-type]
    #[test]
    fn test_wit_world_list_by_type() {
        let conn = setup_test_db();
        let pkg_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();

        WitWorld::insert(&conn, pkg_id, "proxy", None).unwrap();
        WitWorld::insert(&conn, pkg_id, "handler", None).unwrap();

        let worlds = WitWorld::list_by_type(&conn, pkg_id).unwrap();
        assert_eq!(worlds.len(), 2);
        assert_eq!(worlds[0].name, "handler");
        assert_eq!(worlds[1].name, "proxy");
    }

    // r[verify wit-world.list-by-type-empty]
    #[test]
    fn test_wit_world_list_by_type_empty() {
        let conn = setup_test_db();
        let worlds = WitWorld::list_by_type(&conn, 9999).unwrap();
        assert!(worlds.is_empty());
    }

    // r[verify wit-world-import.insert]
    #[test]
    fn test_wit_world_import_insert() {
        let conn = setup_test_db();
        let pkg_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();
        let world_id = WitWorld::insert(&conn, pkg_id, "proxy", None).unwrap();

        let import_id = WitWorldImport::insert(
            &conn,
            world_id,
            "wasi:io",
            Some("streams"),
            Some("0.2.0"),
            None,
        )
        .unwrap();
        assert!(import_id > 0);

        // Without optional fields
        let import_id2 =
            WitWorldImport::insert(&conn, world_id, "wasi:clocks", None, None, None).unwrap();
        assert!(import_id2 > 0);
        assert_ne!(import_id, import_id2);
    }

    // r[verify wit-world-import.insert-idempotent]
    #[test]
    fn test_wit_world_import_insert_idempotent() {
        let conn = setup_test_db();
        let pkg_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();
        let world_id = WitWorld::insert(&conn, pkg_id, "proxy", None).unwrap();

        let id1 = WitWorldImport::insert(
            &conn,
            world_id,
            "wasi:io",
            Some("streams"),
            Some("0.2.0"),
            None,
        )
        .unwrap();
        let id2 = WitWorldImport::insert(
            &conn,
            world_id,
            "wasi:io",
            Some("streams"),
            Some("0.2.0"),
            None,
        )
        .unwrap();
        assert_eq!(id1, id2);
    }

    // r[verify wit-world-export.insert]
    #[test]
    fn test_wit_world_export_insert() {
        let conn = setup_test_db();
        let pkg_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();
        let world_id = WitWorld::insert(&conn, pkg_id, "proxy", None).unwrap();

        let export_id = WitWorldExport::insert(
            &conn,
            world_id,
            "wasi:http",
            Some("handler"),
            Some("0.2.0"),
            None,
        )
        .unwrap();
        assert!(export_id > 0);

        // Without optional fields
        let export_id2 =
            WitWorldExport::insert(&conn, world_id, "wasi:cli", None, None, None).unwrap();
        assert!(export_id2 > 0);
        assert_ne!(export_id, export_id2);
    }

    // r[verify wit-world-export.insert-idempotent]
    #[test]
    fn test_wit_world_export_insert_idempotent() {
        let conn = setup_test_db();
        let pkg_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();
        let world_id = WitWorld::insert(&conn, pkg_id, "proxy", None).unwrap();

        let id1 = WitWorldExport::insert(
            &conn,
            world_id,
            "wasi:http",
            Some("handler"),
            Some("0.2.0"),
            None,
        )
        .unwrap();
        let id2 = WitWorldExport::insert(
            &conn,
            world_id,
            "wasi:http",
            Some("handler"),
            Some("0.2.0"),
            None,
        )
        .unwrap();
        assert_eq!(id1, id2);
    }

    // r[verify wit-package-dependency.insert]
    #[test]
    fn test_wit_package_dependency_insert() {
        let conn = setup_test_db();
        let pkg_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();
        let dep_pkg_id =
            RawWitPackage::insert(&conn, "wasi:io", Some("0.2.0"), None, None, None, None).unwrap();

        let dep_id =
            WitPackageDependency::insert(&conn, pkg_id, "wasi:io", Some("0.2.0"), Some(dep_pkg_id))
                .unwrap();
        assert!(dep_id > 0);

        // Without version
        let dep_id2 =
            WitPackageDependency::insert(&conn, pkg_id, "wasi:clocks", None, None).unwrap();
        assert!(dep_id2 > 0);
        assert_ne!(dep_id, dep_id2);
    }

    // r[verify wit-package-dependency.insert-idempotent]
    #[test]
    fn test_wit_package_dependency_insert_idempotent() {
        let conn = setup_test_db();
        let pkg_id =
            RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
                .unwrap();

        let id1 =
            WitPackageDependency::insert(&conn, pkg_id, "wasi:io", Some("0.2.0"), None).unwrap();
        let id2 =
            WitPackageDependency::insert(&conn, pkg_id, "wasi:io", Some("0.2.0"), None).unwrap();
        assert_eq!(id1, id2);
    }
}
