use rusqlite::Connection;

use super::oci::OciRepository;

/// A known package that persists in the database even after local deletion.
/// This is used to track packages the user has seen or searched for.
///
/// Backed by `oci_repository` in the new schema.  Tags come from `oci_tag`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnownPackage {
    #[allow(dead_code)]
    #[serde(skip)]
    id: i64,
    /// Registry hostname
    pub registry: String,
    /// Repository path
    pub repository: String,
    /// Optional package description
    pub description: Option<String>,
    /// Release tags
    pub tags: Vec<String>,
    /// Signature tags (kept for API compatibility, always empty)
    #[serde(default)]
    pub signature_tags: Vec<String>,
    /// Attestation tags (kept for API compatibility, always empty)
    #[serde(default)]
    pub attestation_tags: Vec<String>,
    /// Timestamp of last seen (maps to `oci_repository.updated_at`)
    pub last_seen_at: String,
    /// Timestamp of creation
    pub created_at: String,
}

impl KnownPackage {
    /// Returns the full reference string for this package (e.g., "ghcr.io/user/repo").
    #[must_use]
    pub fn reference(&self) -> String {
        format!("{}/{}", self.registry, self.repository)
    }

    /// Returns the full reference string with the most recent tag.
    #[must_use]
    pub fn reference_with_tag(&self) -> String {
        if let Some(tag) = self.tags.first() {
            format!("{}:{}", self.reference(), tag)
        } else {
            format!("{}:latest", self.reference())
        }
    }

    /// Inserts or updates a known package.
    ///
    /// Upserts into `oci_repository`.  Tag creation only happens when a
    /// manifest is available (i.e. during pull), so the `tag` parameter is
    /// accepted for API compatibility but only stored when a corresponding
    /// manifest digest can be resolved.
    pub(crate) fn upsert(
        conn: &Connection,
        registry: &str,
        repository: &str,
        tag: Option<&str>,
        description: Option<&str>,
    ) -> anyhow::Result<()> {
        let repo_id = OciRepository::upsert(conn, registry, repository)?;

        // Store description on the most recent manifest that doesn't have one.
        // Uses a subquery instead of LIMIT in UPDATE (SQLITE_ENABLE_UPDATE_DELETE_LIMIT
        // is not enabled in most SQLite builds).
        if let Some(desc) = description
            && let Err(e) = conn.execute(
                "UPDATE oci_manifest SET oci_description = ?1
                 WHERE id = (
                     SELECT id FROM oci_manifest
                     WHERE oci_repository_id = ?2
                       AND oci_description IS NULL
                     ORDER BY created_at DESC
                     LIMIT 1
                 )",
                rusqlite::params![desc, repo_id],
            )
        {
            tracing::warn!("Failed to update description for repo {repo_id}: {e}");
        }

        // If a tag was provided and a manifest exists that it could reference,
        // upsert the tag.  During index/sync there may be no manifest yet, so
        // this is best-effort.
        if let Some(tag) = tag {
            // Try to find the most recent manifest for this repo.
            let digest: Option<String> = conn
                .query_row(
                    "SELECT digest FROM oci_manifest
                     WHERE oci_repository_id = ?1
                     ORDER BY created_at DESC LIMIT 1",
                    [repo_id],
                    |row| row.get(0),
                )
                .ok();

            if let Some(digest) = digest
                && let Err(e) = conn.execute(
                    "INSERT INTO oci_tag (oci_repository_id, tag, manifest_digest)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(oci_repository_id, tag) DO UPDATE SET
                         manifest_digest = ?3,
                         updated_at = CURRENT_TIMESTAMP",
                    rusqlite::params![repo_id, tag, digest],
                )
            {
                tracing::warn!("Failed to upsert tag '{tag}' for repo {repo_id}: {e}");
            }
        }

        Ok(())
    }

    /// Fetch tags for a repository from `oci_tag`, ordered by most recent first.
    fn fetch_tags(conn: &Connection, repo_id: i64) -> Vec<String> {
        let mut stmt = match conn.prepare(
            "SELECT t.tag FROM oci_tag t
             WHERE t.oci_repository_id = ?1
             ORDER BY t.updated_at DESC",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };

        let rows = match stmt.query_map([repo_id], |row| row.get::<_, String>(0)) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };

        rows.flatten().collect()
    }

    /// Fetch the description from the first manifest that has one.
    fn fetch_description(conn: &Connection, repo_id: i64) -> Option<String> {
        conn.query_row(
            "SELECT m.oci_description FROM oci_manifest m
             WHERE m.oci_repository_id = ?1 AND m.oci_description IS NOT NULL
             LIMIT 1",
            [repo_id],
            |row| row.get(0),
        )
        .ok()
    }

    /// Search for known packages by a query string.
    /// Searches in both registry and repository fields.
    pub(crate) fn search(
        conn: &Connection,
        query: &str,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<KnownPackage>> {
        let search_pattern = format!("%{query}%");
        let mut stmt = conn.prepare(
            "SELECT id, registry, repository, updated_at, created_at
             FROM oci_repository
             WHERE registry LIKE ?1 OR repository LIKE ?1
             ORDER BY repository ASC, registry ASC
             LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt.query_map((&search_pattern, limit, offset), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;

        let mut packages = Vec::new();
        for row in rows {
            let (id, registry, repository, updated_at, created_at) = row?;
            let tags = Self::fetch_tags(conn, id);
            let description = Self::fetch_description(conn, id);
            packages.push(KnownPackage {
                id,
                registry,
                repository,
                description,
                tags,
                signature_tags: Vec::new(),
                attestation_tags: Vec::new(),
                last_seen_at: updated_at,
                created_at,
            });
        }
        Ok(packages)
    }

    /// Get all known packages, ordered alphabetically by repository.
    pub(crate) fn get_all(
        conn: &Connection,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<KnownPackage>> {
        let mut stmt = conn.prepare(
            "SELECT id, registry, repository, updated_at, created_at
             FROM oci_repository
             ORDER BY repository ASC, registry ASC
             LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map((limit, offset), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;

        let mut packages = Vec::new();
        for row in rows {
            let (id, registry, repository, updated_at, created_at) = row?;
            let tags = Self::fetch_tags(conn, id);
            let description = Self::fetch_description(conn, id);
            packages.push(KnownPackage {
                id,
                registry,
                repository,
                description,
                tags,
                signature_tags: Vec::new(),
                attestation_tags: Vec::new(),
                last_seen_at: updated_at,
                created_at,
            });
        }
        Ok(packages)
    }

    /// Get a known package by registry and repository.
    pub(crate) fn get(
        conn: &Connection,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<Option<KnownPackage>> {
        let result = conn.query_row(
            "SELECT id, registry, repository, updated_at, created_at
             FROM oci_repository
             WHERE registry = ?1 AND repository = ?2",
            [registry, repository],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        );

        match result {
            Ok((id, reg, repo, updated_at, created_at)) => {
                let tags = Self::fetch_tags(conn, id);
                let description = Self::fetch_description(conn, id);
                Ok(Some(KnownPackage {
                    id,
                    registry: reg,
                    repository: repo,
                    description,
                    tags,
                    signature_tags: Vec::new(),
                    attestation_tags: Vec::new(),
                    last_seen_at: updated_at,
                    created_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Creates a new `KnownPackage` for testing purposes.
    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn new_for_testing(
        registry: String,
        repository: String,
        description: Option<String>,
        tags: Vec<String>,
        signature_tags: Vec<String>,
        attestation_tags: Vec<String>,
        last_seen_at: String,
        created_at: String,
    ) -> Self {
        Self {
            id: 0,
            registry,
            repository,
            description,
            tags,
            signature_tags,
            attestation_tags,
            last_seen_at,
            created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::models::Migrations;

    /// Create an in-memory database with migrations applied for testing.
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();
        conn
    }

    #[test]
    fn test_known_package_upsert_new_package() {
        let conn = setup_test_db();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages.first().unwrap().registry, "ghcr.io");
        assert_eq!(packages.first().unwrap().repository, "user/repo");
    }

    #[test]
    fn test_known_package_upsert_updates_existing() {
        let conn = setup_test_db();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
    }

    #[test]
    fn test_known_package_search() {
        let conn = setup_test_db();
        KnownPackage::upsert(&conn, "ghcr.io", "bytecode/component", None, None).unwrap();
        KnownPackage::upsert(&conn, "docker.io", "library/nginx", None, None).unwrap();
        KnownPackage::upsert(&conn, "ghcr.io", "user/nginx-app", None, None).unwrap();

        let results = KnownPackage::search(&conn, "nginx", 0, 100).unwrap();
        assert_eq!(results.len(), 2);

        let results = KnownPackage::search(&conn, "ghcr", 0, 100).unwrap();
        assert_eq!(results.len(), 2);

        let results = KnownPackage::search(&conn, "bytecode", 0, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results.first().unwrap().repository, "bytecode/component");
    }

    #[test]
    fn test_known_package_search_no_results() {
        let conn = setup_test_db();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let results = KnownPackage::search(&conn, "nonexistent", 0, 100).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_known_package_get() {
        let conn = setup_test_db();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let package = KnownPackage::get(&conn, "ghcr.io", "user/repo").unwrap();
        assert!(package.is_some());
        let package = package.unwrap();
        assert_eq!(package.registry, "ghcr.io");
        assert_eq!(package.repository, "user/repo");

        let package = KnownPackage::get(&conn, "docker.io", "nonexistent").unwrap();
        assert!(package.is_none());
    }

    #[test]
    fn test_known_package_reference() {
        let conn = setup_test_db();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.first().unwrap().reference(), "ghcr.io/user/repo");
    }

    #[test]
    fn test_known_package_reference_with_tag_default() {
        let conn = setup_test_db();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();
        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(
            packages.first().unwrap().reference_with_tag(),
            "ghcr.io/user/repo:latest"
        );
    }
}
