use rusqlite::Connection;

/// An OCI registry/repository pair.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct OciRepository {
    id: i64,
    /// Registry hostname (e.g. "ghcr.io").
    #[allow(dead_code)]
    pub registry: String,
    /// Repository path (e.g. "user/repo").
    #[allow(dead_code)]
    pub repository: String,
    /// When the row was created.
    #[allow(dead_code)]
    pub created_at: String,
    /// When the row was last updated.
    #[allow(dead_code)]
    pub updated_at: String,
}

impl OciRepository {
    /// Returns the primary key.
    #[must_use]
    pub(crate) fn id(&self) -> i64 {
        self.id
    }

    /// Insert or update a repository, returning its row id.
    pub(crate) fn upsert(
        conn: &Connection,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<i64> {
        Self::upsert_with_wit(conn, registry, repository, None, None)
    }

    /// Insert or update a repository with optional WIT namespace mapping,
    /// returning its row id.
    ///
    /// When `wit_namespace` / `wit_name` are `Some`, they are stored; when
    /// `None`, any existing values are preserved (COALESCE).
    pub(crate) fn upsert_with_wit(
        conn: &Connection,
        registry: &str,
        repository: &str,
        wit_namespace: Option<&str>,
        wit_name: Option<&str>,
    ) -> anyhow::Result<i64> {
        Self::upsert_full(conn, registry, repository, wit_namespace, wit_name, None)
    }

    /// Insert or update a repository with optional WIT namespace and kind,
    /// returning its row id.
    ///
    /// When optional fields are `None`, existing values are preserved
    /// (COALESCE).
    pub(crate) fn upsert_full(
        conn: &Connection,
        registry: &str,
        repository: &str,
        wit_namespace: Option<&str>,
        wit_name: Option<&str>,
        kind: Option<&str>,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO oci_repository (registry, repository, wit_namespace, wit_name, kind)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(registry, repository) DO UPDATE SET
                 updated_at = CURRENT_TIMESTAMP,
                 wit_namespace = COALESCE(?3, oci_repository.wit_namespace),
                 wit_name = COALESCE(?4, oci_repository.wit_name),
                 kind = COALESCE(?5, oci_repository.kind)",
            rusqlite::params![registry, repository, wit_namespace, wit_name, kind],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM oci_repository WHERE registry = ?1 AND repository = ?2",
            (registry, repository),
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// Get a repository by its primary key.
    #[allow(dead_code)]
    pub(crate) fn get_by_id(conn: &Connection, id: i64) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, registry, repository, created_at, updated_at
             FROM oci_repository WHERE id = ?1",
            [id],
            |row| {
                Ok(Self {
                    id: row.get(0)?,
                    registry: row.get(1)?,
                    repository: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        );

        match result {
            Ok(repo) => Ok(Some(repo)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find a repository by registry and repository name.
    pub(crate) fn find(
        conn: &Connection,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, registry, repository, created_at, updated_at
             FROM oci_repository WHERE registry = ?1 AND repository = ?2",
            (registry, repository),
            |row| {
                Ok(Self {
                    id: row.get(0)?,
                    registry: row.get(1)?,
                    repository: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        );

        match result {
            Ok(repo) => Ok(Some(repo)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List every repository.
    #[allow(dead_code)]
    pub(crate) fn list_all(conn: &Connection) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, registry, repository, created_at, updated_at
             FROM oci_repository ORDER BY repository ASC, registry ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Self {
                id: row.get(0)?,
                registry: row.get(1)?,
                repository: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}
