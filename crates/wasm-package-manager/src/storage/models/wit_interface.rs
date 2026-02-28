use rusqlite::Connection;

/// A WIT interface package stored in the database.
///
/// Each row represents a unique (package_name, version, oci_layer_id) tuple.
/// The record is content-addressable: inserting a duplicate is a no-op.
#[derive(Debug, Clone)]
pub struct WitInterface {
    id: i64,
    /// The WIT package name (e.g. "wasi:http").
    pub package_name: String,
    /// Semver version string, if known.
    pub version: Option<String>,
    /// Human-readable description of the interface.
    pub description: Option<String>,
    /// Full WIT text representation, when available.
    pub wit_text: Option<String>,
    /// OCI manifest this interface was extracted from.
    pub oci_manifest_id: Option<i64>,
    /// OCI layer this interface was extracted from.
    pub oci_layer_id: Option<i64>,
    /// When this row was created.
    pub created_at: String,
}

impl WitInterface {
    /// Returns the primary-key ID of this WIT interface.
    #[must_use]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Insert a new WIT interface and return its ID.
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
            "INSERT INTO wit_interface
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
            "SELECT id FROM wit_interface
             WHERE package_name = ?1
               AND COALESCE(version, '') = COALESCE(?2, '')
               AND COALESCE(oci_layer_id, -1) = COALESCE(?3, -1)",
            rusqlite::params![package_name, version, oci_layer_id],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// Find a WIT interface by package name and optional version.
    #[allow(dead_code)]
    pub(crate) fn find(
        conn: &Connection,
        package_name: &str,
        version: Option<&str>,
    ) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, package_name, version, description, wit_text,
                    oci_manifest_id, oci_layer_id, created_at
             FROM wit_interface
             WHERE package_name = ?1
               AND COALESCE(version, '') = COALESCE(?2, '')",
            rusqlite::params![package_name, version],
            Self::from_row,
        );

        match result {
            Ok(iface) => Ok(Some(iface)),
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
             FROM wit_interface
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

    /// Return every WIT interface, ordered by name then version.
    pub(crate) fn get_all(conn: &Connection) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, package_name, version, description, wit_text,
                    oci_manifest_id, oci_layer_id, created_at
             FROM wit_interface
             ORDER BY package_name ASC, version ASC",
        )?;

        let rows = stmt.query_map([], Self::from_row)?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Return every WIT interface together with its OCI reference string.
    ///
    /// Joins through `oci_manifest` → `oci_repository` to build the reference.
    pub(crate) fn get_all_with_images(conn: &Connection) -> anyhow::Result<Vec<(Self, String)>> {
        let mut stmt = conn.prepare(
            "SELECT w.id, w.package_name, w.version, w.description, w.wit_text,
                    w.oci_manifest_id, w.oci_layer_id, w.created_at,
                    r.registry || '/' || r.repository AS reference
             FROM wit_interface w
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
}
