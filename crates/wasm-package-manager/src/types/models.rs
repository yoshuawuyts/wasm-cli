use rusqlite::Connection;

/// A WIT package stored in the database.
///
/// Each row represents a unique (package_name, version, oci_layer_id) tuple.
/// The record is content-addressable: inserting a duplicate is a no-op.
#[derive(Debug, Clone)]
pub struct WitPackage {
    id: i64,
    /// The WIT package name (e.g. "wasi:http").
    pub package_name: String,
    /// Semver version string, if known.
    pub version: Option<String>,
    /// Human-readable description of the type.
    pub description: Option<String>,
    /// Full WIT text representation, when available.
    pub wit_text: Option<String>,
    /// OCI manifest this type was extracted from.
    pub oci_manifest_id: Option<i64>,
    /// OCI layer this type was extracted from.
    pub oci_layer_id: Option<i64>,
    /// When this row was created.
    pub created_at: String,
}

impl WitPackage {
    /// Returns the primary-key ID of this WIT package.
    #[must_use]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Insert a new WIT type and return its ID.
    ///
    /// Uses `INSERT … ON CONFLICT DO NOTHING` followed by a `SELECT` so that
    /// the caller always receives the canonical row ID for the given
    /// (package_name, version, oci_layer_id) triple.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert(
        conn: &Connection,
        package_name: &str,
        version: Option<&str>,
        description: Option<&str>,
        wit_text: Option<&str>,
        oci_manifest_id: Option<i64>,
        oci_layer_id: Option<i64>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO wit_package
                 (package_name, version, description, wit_text, oci_manifest_id, oci_layer_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT DO NOTHING",
            rusqlite::params![
                package_name,
                version,
                description,
                wit_text,
                oci_manifest_id,
                oci_layer_id,
            ],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM wit_package
             WHERE package_name = ?1
               AND COALESCE(version, '') = COALESCE(?2, '')
               AND COALESCE(oci_layer_id, -1) = COALESCE(?3, -1)",
            rusqlite::params![package_name, version, oci_layer_id],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// Find a WIT package by package name and optional version.
    #[allow(dead_code)]
    pub(crate) fn find(
        conn: &Connection,
        package_name: &str,
        version: Option<&str>,
    ) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, package_name, version, description, wit_text,
                    oci_manifest_id, oci_layer_id, created_at
             FROM wit_package
             WHERE package_name = ?1
               AND COALESCE(version, '') = COALESCE(?2, '')",
            rusqlite::params![package_name, version],
            Self::from_row,
        );

        match result {
            Ok(wt) => Ok(Some(wt)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Full-text search on package_name and description.
    #[allow(dead_code)]
    pub(crate) fn search(
        conn: &Connection,
        query: &str,
        offset: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<Self>> {
        let pattern = format!("%{query}%");
        let mut stmt = conn.prepare(
            "SELECT id, package_name, version, description, wit_text,
                    oci_manifest_id, oci_layer_id, created_at
             FROM wit_package
             WHERE package_name LIKE ?1 OR description LIKE ?1
             ORDER BY package_name ASC, version ASC
             LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt.query_map(rusqlite::params![pattern, limit, offset], Self::from_row)?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Return every WIT package, ordered by name then version.
    pub(crate) fn get_all(conn: &Connection) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, package_name, version, description, wit_text,
                    oci_manifest_id, oci_layer_id, created_at
             FROM wit_package
             ORDER BY package_name ASC, version ASC",
        )?;

        let rows = stmt.query_map([], Self::from_row)?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Return every WIT package together with its OCI reference string.
    ///
    /// Joins through `oci_manifest` → `oci_repository` to build the reference.
    pub(crate) fn get_all_with_images(conn: &Connection) -> anyhow::Result<Vec<(Self, String)>> {
        let mut stmt = conn.prepare(
            "SELECT w.id, w.package_name, w.version, w.description, w.wit_text,
                    w.oci_manifest_id, w.oci_layer_id, w.created_at,
                    r.registry || '/' || r.repository AS reference
             FROM wit_package w
             JOIN oci_manifest m ON w.oci_manifest_id = m.id
             JOIN oci_repository r ON m.oci_repository_id = r.id
             ORDER BY w.package_name ASC, w.version ASC, r.repository ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                Self {
                    id: row.get(0)?,
                    package_name: row.get(1)?,
                    version: row.get(2)?,
                    description: row.get(3)?,
                    wit_text: row.get(4)?,
                    oci_manifest_id: row.get(5)?,
                    oci_layer_id: row.get(6)?,
                    created_at: row.get(7)?,
                },
                row.get::<_, String>(8)?,
            ))
        })?;

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
            package_name: row.get(1)?,
            version: row.get(2)?,
            description: row.get(3)?,
            wit_text: row.get(4)?,
            oci_manifest_id: row.get(5)?,
            oci_layer_id: row.get(6)?,
            created_at: row.get(7)?,
        })
    }

    /// Find the OCI reference (registry, repository) for a WIT package by name and version.
    ///
    /// JOINs through `oci_manifest` → `oci_repository` to resolve
    /// the registry location of a previously-pulled WIT package.
    pub(crate) fn find_oci_reference(
        conn: &Connection,
        package_name: &str,
        version: Option<&str>,
    ) -> anyhow::Result<Option<(String, String)>> {
        let result = conn.query_row(
            "SELECT r.registry, r.repository
             FROM wit_package w
             JOIN oci_manifest m ON w.oci_manifest_id = m.id
             JOIN oci_repository r ON m.oci_repository_id = r.id
             WHERE w.package_name = ?1
               AND COALESCE(w.version, '') = COALESCE(?2, '')
             LIMIT 1",
            rusqlite::params![package_name, version],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        );

        match result {
            Ok(pair) => Ok(Some(pair)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oci::{OciManifest, OciRepository};
    use crate::storage::Migrations;
    use std::collections::HashMap;

    /// Create an in-memory database with migrations applied for testing.
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();
        conn
    }

    /// Helper: create a repo + manifest in the test DB, returning the manifest ID.
    fn insert_test_manifest(conn: &Connection, registry: &str, repository: &str) -> i64 {
        let repo_id = OciRepository::upsert(conn, registry, repository).unwrap();
        let annotations = HashMap::new();
        let (manifest_id, _) = OciManifest::upsert(
            conn,
            repo_id,
            "sha256:abc123",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(1024),
            None,
            None,
            None,
            &annotations,
        )
        .unwrap();
        manifest_id
    }

    // r[verify db.wit-package.find-oci-reference]
    #[test]
    fn find_oci_reference_returns_registry_and_repository() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn, "ghcr.io", "webassembly/wasi/http");

        WitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let result = WitPackage::find_oci_reference(&conn, "wasi:http", Some("0.2.0")).unwrap();
        assert!(result.is_some());
        let (registry, repository) = result.unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(repository, "webassembly/wasi/http");
    }

    // r[verify db.wit-package.find-oci-reference-not-found]
    #[test]
    fn find_oci_reference_returns_none_when_not_found() {
        let conn = setup_test_db();

        let result =
            WitPackage::find_oci_reference(&conn, "wasi:nonexistent", Some("1.0.0")).unwrap();
        assert!(result.is_none());
    }

    // r[verify db.wit-package.find-oci-reference-no-version]
    #[test]
    fn find_oci_reference_without_version() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn, "ghcr.io", "webassembly/wasi/clocks");

        WitPackage::insert(
            &conn,
            "wasi:clocks",
            None,
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let result = WitPackage::find_oci_reference(&conn, "wasi:clocks", None).unwrap();
        assert!(result.is_some());
        let (registry, repository) = result.unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(repository, "webassembly/wasi/clocks");
    }
}
