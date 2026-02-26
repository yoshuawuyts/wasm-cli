use rusqlite::Connection;

/// The type of a tag, used to distinguish release tags from signatures and attestations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagType {
    /// A regular release tag (e.g., "1.0.0", "latest")
    Release,
    /// A signature tag (ending in ".sig")
    Signature,
    /// An attestation tag (ending in ".att")
    Attestation,
}

impl TagType {
    /// Determine the tag type from a tag string.
    pub(crate) fn from_tag(tag: &str) -> Self {
        if tag.ends_with(".sig") {
            TagType::Signature
        } else if tag.ends_with(".att") {
            TagType::Attestation
        } else {
            TagType::Release
        }
    }

    /// Convert to the database string representation.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            TagType::Release => "release",
            TagType::Signature => "signature",
            TagType::Attestation => "attestation",
        }
    }
}

/// A known package that persists in the database even after local deletion.
/// This is used to track packages the user has seen or searched for.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct KnownPackage {
    #[allow(dead_code)]
    #[cfg_attr(feature = "serde", serde(skip))]
    id: i64,
    /// Registry hostname
    pub registry: String,
    /// Repository path
    pub repository: String,
    /// Optional package description
    pub description: Option<String>,
    /// Release tags (regular version tags like "1.0.0", "latest")
    pub tags: Vec<String>,
    /// Signature tags (tags ending in ".sig")
    pub signature_tags: Vec<String>,
    /// Attestation tags (tags ending in ".att")
    pub attestation_tags: Vec<String>,
    /// Timestamp of last seen
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

    /// Inserts or updates a known package in the database.
    /// If the package already exists, updates the last_seen_at timestamp.
    /// Also adds the tag if provided, classifying it by type.
    /// Uses a transaction to ensure atomicity of the multi-step operation.
    pub(crate) fn upsert(
        conn: &Connection,
        registry: &str,
        repository: &str,
        tag: Option<&str>,
        description: Option<&str>,
    ) -> anyhow::Result<()> {
        let tx = conn.unchecked_transaction()?;

        tx.execute(
            "INSERT INTO known_package (registry, repository, description) VALUES (?1, ?2, ?3)
             ON CONFLICT(registry, repository) DO UPDATE SET 
                last_seen_at = datetime('now'),
                description = COALESCE(excluded.description, known_package.description)",
            (registry, repository, description),
        )?;

        // If a tag was provided, add it to the tags table with its type
        if let Some(tag) = tag {
            let package_id: i64 = tx.query_row(
                "SELECT id FROM known_package WHERE registry = ?1 AND repository = ?2",
                (registry, repository),
                |row| row.get(0),
            )?;

            let tag_type = TagType::from_tag(tag);
            tx.execute(
                "INSERT INTO known_package_tag (known_package_id, tag, tag_type) VALUES (?1, ?2, ?3)
                 ON CONFLICT(known_package_id, tag) DO UPDATE SET last_seen_at = datetime('now'), tag_type = ?3",
                (package_id, tag, tag_type.as_str()),
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Helper to fetch tags for a package by its ID, separated by type.
    /// Returns (release_tags, signature_tags, attestation_tags).
    /// Logs warnings if database queries fail.
    fn fetch_tags_by_type(
        conn: &Connection,
        package_id: i64,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut release_tags = Vec::new();
        let mut signature_tags = Vec::new();
        let mut attestation_tags = Vec::new();

        let mut stmt = match conn.prepare(
            "SELECT tag, tag_type FROM known_package_tag WHERE known_package_id = ?1 ORDER BY last_seen_at DESC",
        ) {
            Ok(stmt) => stmt,
            Err(e) => {
                eprintln!("Warning: Failed to prepare tag query for package {}: {}", package_id, e);
                return (release_tags, signature_tags, attestation_tags);
            }
        };

        let rows = match stmt.query_map([package_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to query tags for package {}: {}",
                    package_id, e
                );
                return (release_tags, signature_tags, attestation_tags);
            }
        };

        for row in rows.flatten() {
            let (tag, tag_type) = row;
            match tag_type.as_str() {
                "signature" => signature_tags.push(tag),
                "attestation" => attestation_tags.push(tag),
                _ => release_tags.push(tag),
            }
        }

        (release_tags, signature_tags, attestation_tags)
    }

    /// Search for known packages by a query string.
    /// Searches in both registry and repository fields.
    /// Uses pagination with `offset` and `limit` parameters.
    pub(crate) fn search(
        conn: &Connection,
        query: &str,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<KnownPackage>> {
        let search_pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT id, registry, repository, description, last_seen_at, created_at 
             FROM known_package 
             WHERE registry LIKE ?1 OR repository LIKE ?1
             ORDER BY repository ASC, registry ASC
             LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt.query_map((&search_pattern, limit, offset), |row| {
            let id: i64 = row.get(0)?;
            Ok((
                id,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        let mut packages = Vec::new();
        for row in rows {
            let (id, registry, repository, description, last_seen_at, created_at) = row?;
            let (tags, signature_tags, attestation_tags) = Self::fetch_tags_by_type(conn, id);
            packages.push(KnownPackage {
                id,
                registry,
                repository,
                description,
                tags,
                signature_tags,
                attestation_tags,
                last_seen_at,
                created_at,
            });
        }
        Ok(packages)
    }

    /// Get all known packages, ordered alphabetically by repository.
    /// Uses pagination with `offset` and `limit` parameters.
    pub(crate) fn get_all(
        conn: &Connection,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<KnownPackage>> {
        let mut stmt = conn.prepare(
            "SELECT id, registry, repository, description, last_seen_at, created_at 
             FROM known_package 
             ORDER BY repository ASC, registry ASC
             LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map((limit, offset), |row| {
            let id: i64 = row.get(0)?;
            Ok((
                id,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        let mut packages = Vec::new();
        for row in rows {
            let (id, registry, repository, description, last_seen_at, created_at) = row?;
            let (tags, signature_tags, attestation_tags) = Self::fetch_tags_by_type(conn, id);
            packages.push(KnownPackage {
                id,
                registry,
                repository,
                description,
                tags,
                signature_tags,
                attestation_tags,
                last_seen_at,
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
        let mut stmt = conn.prepare(
            "SELECT id, registry, repository, description, last_seen_at, created_at 
             FROM known_package 
             WHERE registry = ?1 AND repository = ?2",
        )?;

        let mut rows = stmt.query_map([registry, repository], |row| {
            let id: i64 = row.get(0)?;
            Ok((
                id,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        match rows.next() {
            Some(row) => {
                let (id, registry, repository, description, last_seen_at, created_at) = row?;
                let (tags, signature_tags, attestation_tags) = Self::fetch_tags_by_type(conn, id);
                Ok(Some(KnownPackage {
                    id,
                    registry,
                    repository,
                    description,
                    tags,
                    signature_tags,
                    attestation_tags,
                    last_seen_at,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Creates a new KnownPackage for testing purposes.
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
            id: 0, // Test ID
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

    // =========================================================================
    // TagType Tests
    // =========================================================================

    #[test]
    fn test_tag_type_from_tag_release() {
        assert_eq!(TagType::from_tag("latest"), TagType::Release);
        assert_eq!(TagType::from_tag("v1.0.0"), TagType::Release);
        assert_eq!(TagType::from_tag("1.2.3"), TagType::Release);
        assert_eq!(TagType::from_tag("main"), TagType::Release);
    }

    #[test]
    fn test_tag_type_from_tag_signature() {
        assert_eq!(TagType::from_tag("v1.0.0.sig"), TagType::Signature);
        assert_eq!(TagType::from_tag("latest.sig"), TagType::Signature);
        assert_eq!(TagType::from_tag(".sig"), TagType::Signature);
    }

    #[test]
    fn test_tag_type_from_tag_attestation() {
        assert_eq!(TagType::from_tag("v1.0.0.att"), TagType::Attestation);
        assert_eq!(TagType::from_tag("latest.att"), TagType::Attestation);
        assert_eq!(TagType::from_tag(".att"), TagType::Attestation);
    }

    // =========================================================================
    // KnownPackage Tests
    // =========================================================================

    #[test]
    fn test_known_package_upsert_new_package() {
        let conn = setup_test_db();

        // Insert a new package
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        // Verify it was inserted
        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].registry, "ghcr.io");
        assert_eq!(packages[0].repository, "user/repo");
    }

    #[test]
    fn test_known_package_upsert_with_tag() {
        let conn = setup_test_db();

        // Insert a package with a tag
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", Some("v1.0.0"), None).unwrap();

        // Verify it was inserted with the tag
        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].tags, vec!["v1.0.0"]);
    }

    #[test]
    fn test_known_package_upsert_multiple_tags() {
        let conn = setup_test_db();

        // Insert a package with multiple tags
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", Some("v1.0.0"), None).unwrap();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", Some("v2.0.0"), None).unwrap();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", Some("latest"), None).unwrap();

        // Verify all tags are present
        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
        // Tags are ordered by last_seen_at DESC
        assert!(packages[0].tags.contains(&"v1.0.0".to_string()));
        assert!(packages[0].tags.contains(&"v2.0.0".to_string()));
        assert!(packages[0].tags.contains(&"latest".to_string()));
    }

    #[test]
    fn test_known_package_upsert_with_description() {
        let conn = setup_test_db();

        // Insert a package with description
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, Some("A test package")).unwrap();

        // Verify description was saved
        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].description, Some("A test package".to_string()));
    }

    #[test]
    fn test_known_package_upsert_updates_existing() {
        let conn = setup_test_db();

        // Insert package
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        // Update with description
        KnownPackage::upsert(
            &conn,
            "ghcr.io",
            "user/repo",
            None,
            Some("Updated description"),
        )
        .unwrap();

        // Verify only one package exists with updated description
        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(
            packages[0].description,
            Some("Updated description".to_string())
        );
    }

    #[test]
    fn test_known_package_tag_types_separated() {
        let conn = setup_test_db();

        // Insert package with different tag types
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", Some("v1.0.0"), None).unwrap();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", Some("v1.0.0.sig"), None).unwrap();
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", Some("v1.0.0.att"), None).unwrap();

        // Verify tags are separated by type
        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
        assert!(packages[0].tags.contains(&"v1.0.0".to_string()));
        assert!(
            packages[0]
                .signature_tags
                .contains(&"v1.0.0.sig".to_string())
        );
        assert!(
            packages[0]
                .attestation_tags
                .contains(&"v1.0.0.att".to_string())
        );
    }

    #[test]
    fn test_known_package_search() {
        let conn = setup_test_db();

        // Insert multiple packages
        KnownPackage::upsert(&conn, "ghcr.io", "bytecode/component", None, None).unwrap();
        KnownPackage::upsert(&conn, "docker.io", "library/nginx", None, None).unwrap();
        KnownPackage::upsert(&conn, "ghcr.io", "user/nginx-app", None, None).unwrap();

        // Search for nginx
        let results = KnownPackage::search(&conn, "nginx", 0, 100).unwrap();
        assert_eq!(results.len(), 2);

        // Search for ghcr.io
        let results = KnownPackage::search(&conn, "ghcr", 0, 100).unwrap();
        assert_eq!(results.len(), 2);

        // Search for bytecode
        let results = KnownPackage::search(&conn, "bytecode", 0, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].repository, "bytecode/component");
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

        // Insert a package
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", Some("v1.0.0"), None).unwrap();

        // Get existing package
        let package = KnownPackage::get(&conn, "ghcr.io", "user/repo").unwrap();
        assert!(package.is_some());
        let package = package.unwrap();
        assert_eq!(package.registry, "ghcr.io");
        assert_eq!(package.repository, "user/repo");

        // Get non-existent package
        let package = KnownPackage::get(&conn, "docker.io", "nonexistent").unwrap();
        assert!(package.is_none());
    }

    #[test]
    fn test_known_package_reference() {
        let conn = setup_test_db();

        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages[0].reference(), "ghcr.io/user/repo");
    }

    #[test]
    fn test_known_package_reference_with_tag() {
        let conn = setup_test_db();

        // Package without tags uses "latest"
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();
        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages[0].reference_with_tag(), "ghcr.io/user/repo:latest");

        // Package with tag uses first tag
        KnownPackage::upsert(&conn, "ghcr.io", "user/repo", Some("v1.0.0"), None).unwrap();
        let packages = KnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages[0].reference_with_tag(), "ghcr.io/user/repo:v1.0.0");
    }
}
