use rusqlite::Connection;

/// A raw WIT package stored in the database.
///
/// Each row represents a unique (package_name, version, oci_layer_id) tuple.
/// The record is content-addressable: inserting a duplicate is a no-op.
///
/// This is the internal database-backed type. The public API exposes
/// [`super::WitPackage`] instead, which strips away internal IDs.
#[derive(Debug, Clone)]
pub struct RawWitPackage {
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

impl RawWitPackage {
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

        RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let result = RawWitPackage::find_oci_reference(&conn, "wasi:http", Some("0.2.0")).unwrap();
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
            RawWitPackage::find_oci_reference(&conn, "wasi:nonexistent", Some("1.0.0")).unwrap();
        assert!(result.is_none());
    }

    // r[verify db.wit-package.find-oci-reference-no-version]
    #[test]
    fn find_oci_reference_without_version() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn, "ghcr.io", "webassembly/wasi/clocks");

        RawWitPackage::insert(
            &conn,
            "wasi:clocks",
            None,
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let result = RawWitPackage::find_oci_reference(&conn, "wasi:clocks", None).unwrap();
        assert!(result.is_some());
        let (registry, repository) = result.unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(repository, "webassembly/wasi/clocks");
    }

    // r[verify db.wit-package.insert]
    #[test]
    fn insert_returns_id_and_is_idempotent() {
        let conn = setup_test_db();

        let id1 = RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
            .unwrap();
        assert!(id1 > 0);

        // Same insert returns the same ID (idempotent)
        let id2 = RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None)
            .unwrap();
        assert_eq!(id1, id2);

        // Different version gets a different ID
        let id3 = RawWitPackage::insert(&conn, "wasi:http", Some("0.3.0"), None, None, None, None)
            .unwrap();
        assert_ne!(id1, id3);
    }

    // r[verify db.wit-package.insert-with-metadata]
    #[test]
    fn insert_with_description_and_wit_text() {
        let conn = setup_test_db();
        let manifest_id = insert_test_manifest(&conn, "ghcr.io", "webassembly/wasi/http");

        let id = RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            Some("HTTP types and handlers"),
            Some("package wasi:http@0.2.0;"),
            Some(manifest_id),
            None,
        )
        .unwrap();

        let pkg = RawWitPackage::find(&conn, "wasi:http", Some("0.2.0"))
            .unwrap()
            .unwrap();
        assert_eq!(pkg.id(), id);
        assert_eq!(pkg.description.as_deref(), Some("HTTP types and handlers"));
        assert_eq!(pkg.wit_text.as_deref(), Some("package wasi:http@0.2.0;"));
    }

    // r[verify db.wit-package.find]
    #[test]
    fn find_returns_package_or_none() {
        let conn = setup_test_db();

        let not_found = RawWitPackage::find(&conn, "wasi:http", Some("0.2.0")).unwrap();
        assert!(not_found.is_none());

        RawWitPackage::insert(&conn, "wasi:http", Some("0.2.0"), None, None, None, None).unwrap();

        let found = RawWitPackage::find(&conn, "wasi:http", Some("0.2.0"))
            .unwrap()
            .unwrap();
        assert_eq!(found.package_name, "wasi:http");
        assert_eq!(found.version.as_deref(), Some("0.2.0"));
    }

    // r[verify db.wit-package.find-none-version]
    #[test]
    fn find_with_none_version() {
        let conn = setup_test_db();

        RawWitPackage::insert(&conn, "wasi:clocks", None, None, None, None, None).unwrap();

        let found = RawWitPackage::find(&conn, "wasi:clocks", None)
            .unwrap()
            .unwrap();
        assert_eq!(found.package_name, "wasi:clocks");
        assert!(found.version.is_none());
    }

    // r[verify db.wit-package.search]
    #[test]
    fn search_matches_name_and_description() {
        let conn = setup_test_db();

        RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            Some("HTTP types"),
            None,
            None,
            None,
        )
        .unwrap();
        RawWitPackage::insert(
            &conn,
            "wasi:clocks",
            Some("0.2.0"),
            Some("Clock functions"),
            None,
            None,
            None,
        )
        .unwrap();
        RawWitPackage::insert(
            &conn,
            "wasi:io",
            Some("0.2.0"),
            Some("I/O streams"),
            None,
            None,
            None,
        )
        .unwrap();

        // Search by name
        let results = RawWitPackage::search(&conn, "http", 0, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].package_name, "wasi:http");

        // Search by description
        let results = RawWitPackage::search(&conn, "Clock", 0, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].package_name, "wasi:clocks");

        // Search with limit and offset
        let results = RawWitPackage::search(&conn, "wasi", 0, 2).unwrap();
        assert_eq!(results.len(), 2);

        let results = RawWitPackage::search(&conn, "wasi", 2, 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    // r[verify db.wit-package.get-all]
    #[test]
    fn get_all_returns_ordered_results() {
        let conn = setup_test_db();

        let empty = RawWitPackage::get_all(&conn).unwrap();
        assert!(empty.is_empty());

        RawWitPackage::insert(&conn, "wasi:io", Some("0.2.0"), None, None, None, None).unwrap();
        RawWitPackage::insert(&conn, "wasi:clocks", Some("0.1.0"), None, None, None, None).unwrap();
        RawWitPackage::insert(&conn, "wasi:clocks", Some("0.2.0"), None, None, None, None).unwrap();

        let all = RawWitPackage::get_all(&conn).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].package_name, "wasi:clocks");
        assert_eq!(all[0].version.as_deref(), Some("0.1.0"));
        assert_eq!(all[1].package_name, "wasi:clocks");
        assert_eq!(all[1].version.as_deref(), Some("0.2.0"));
        assert_eq!(all[2].package_name, "wasi:io");
    }

    // r[verify db.wit-package.get-all-with-images]
    #[test]
    fn get_all_with_images_returns_reference() {
        let conn = setup_test_db();

        let empty = RawWitPackage::get_all_with_images(&conn).unwrap();
        assert!(empty.is_empty());

        let manifest_id = insert_test_manifest(&conn, "ghcr.io", "webassembly/wasi/http");
        RawWitPackage::insert(
            &conn,
            "wasi:http",
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();

        let results = RawWitPackage::get_all_with_images(&conn).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.package_name, "wasi:http");
        assert_eq!(results[0].1, "ghcr.io/webassembly/wasi/http");
    }
}
