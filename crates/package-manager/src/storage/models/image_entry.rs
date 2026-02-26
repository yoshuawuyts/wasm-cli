use oci_client::manifest::OciImageManifest;
#[cfg(any(test, feature = "test-helpers"))]
use oci_client::manifest::{IMAGE_MANIFEST_MEDIA_TYPE, OciDescriptor};
use rusqlite::Connection;

/// Result of an insert operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertResult {
    /// The entry was inserted successfully.
    Inserted,
    /// The entry already existed in the database.
    AlreadyExists,
}

/// Metadata for a stored OCI image.
#[derive(Debug, Clone)]
pub struct ImageEntry {
    #[allow(dead_code)] // Used in database schema
    id: i64,
    /// Registry hostname
    pub ref_registry: String,
    /// Repository path
    pub ref_repository: String,
    /// Optional mirror registry hostname
    pub ref_mirror_registry: Option<String>,
    /// Optional tag
    pub ref_tag: Option<String>,
    /// Optional digest
    pub ref_digest: Option<String>,
    /// OCI image manifest
    pub manifest: OciImageManifest,
    /// Size of the image on disk in bytes
    pub size_on_disk: u64,
}

impl ImageEntry {
    /// Returns the full reference string for this image (e.g., "ghcr.io/user/repo:tag").
    #[must_use]
    pub fn reference(&self) -> String {
        let mut reference = format!("{}/{}", self.ref_registry, self.ref_repository);
        if let Some(tag) = &self.ref_tag {
            reference.push(':');
            reference.push_str(tag);
        } else if let Some(digest) = &self.ref_digest {
            reference.push('@');
            reference.push_str(digest);
        }
        reference
    }

    /// Checks if an image entry with the given reference already exists.
    #[allow(dead_code)]
    pub(crate) fn exists(
        conn: &Connection,
        ref_registry: &str,
        ref_repository: &str,
        ref_tag: Option<&str>,
        ref_digest: Option<&str>,
    ) -> anyhow::Result<bool> {
        let count: i64 = match (ref_tag, ref_digest) {
            (Some(tag), Some(digest)) => conn.query_row(
                "SELECT COUNT(*) FROM image WHERE ref_registry = ?1 AND ref_repository = ?2 AND ref_tag = ?3 AND ref_digest = ?4",
                (ref_registry, ref_repository, tag, digest),
                |row| row.get(0),
            )?,
            (Some(tag), None) => conn.query_row(
                "SELECT COUNT(*) FROM image WHERE ref_registry = ?1 AND ref_repository = ?2 AND ref_tag = ?3 AND ref_digest IS NULL",
                (ref_registry, ref_repository, tag),
                |row| row.get(0),
            )?,
            (None, Some(digest)) => conn.query_row(
                "SELECT COUNT(*) FROM image WHERE ref_registry = ?1 AND ref_repository = ?2 AND ref_tag IS NULL AND ref_digest = ?3",
                (ref_registry, ref_repository, digest),
                |row| row.get(0),
            )?,
            (None, None) => conn.query_row(
                "SELECT COUNT(*) FROM image WHERE ref_registry = ?1 AND ref_repository = ?2 AND ref_tag IS NULL AND ref_digest IS NULL",
                (ref_registry, ref_repository),
                |row| row.get(0),
            )?,
        };
        Ok(count > 0)
    }

    /// Inserts a new image entry into the database if it doesn't already exist.
    /// Uses atomic `INSERT ... ON CONFLICT DO NOTHING` to prevent race conditions.
    /// Returns `(InsertResult::AlreadyExists, None)` if the entry already exists,
    /// or `(InsertResult::Inserted, Some(id))` if it was successfully inserted.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert(
        conn: &Connection,
        ref_registry: &str,
        ref_repository: &str,
        ref_tag: Option<&str>,
        ref_digest: Option<&str>,
        manifest: &str,
        size_on_disk: u64,
        package_type: &str,
    ) -> anyhow::Result<(InsertResult, Option<i64>)> {
        // Use atomic upsert to prevent race conditions
        // The unique index uses COALESCE for NULL handling, so we match that pattern
        let rows_affected = conn.execute(
            "INSERT INTO image (ref_registry, ref_repository, ref_tag, ref_digest, manifest, size_on_disk, package_type) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(ref_registry, ref_repository, COALESCE(ref_tag, ''), COALESCE(ref_digest, '')) DO NOTHING",
            (ref_registry, ref_repository, ref_tag, ref_digest, manifest, size_on_disk as i64, package_type),
        )?;

        if rows_affected == 0 {
            Ok((InsertResult::AlreadyExists, None))
        } else {
            Ok((InsertResult::Inserted, Some(conn.last_insert_rowid())))
        }
    }

    /// Returns all currently stored images and their metadata, ordered alphabetically by repository.
    pub(crate) fn get_all(conn: &Connection) -> anyhow::Result<Vec<ImageEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, ref_registry, ref_repository, ref_mirror_registry, ref_tag, ref_digest, manifest, size_on_disk FROM image ORDER BY ref_repository ASC, ref_registry ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            let manifest_json: String = row.get(6)?;
            let manifest: OciImageManifest = serde_json::from_str(&manifest_json).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            let size_on_disk: i64 = row.get(7)?;

            Ok(ImageEntry {
                id: row.get(0)?,
                ref_registry: row.get(1)?,
                ref_repository: row.get(2)?,
                ref_mirror_registry: row.get(3)?,
                ref_tag: row.get(4)?,
                ref_digest: row.get(5)?,
                manifest,
                size_on_disk: size_on_disk as u64,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Deletes an image entry by its full reference string.
    pub(crate) fn delete_by_reference(
        conn: &Connection,
        registry: &str,
        repository: &str,
        tag: Option<&str>,
        digest: Option<&str>,
    ) -> anyhow::Result<bool> {
        let rows_affected = match (tag, digest) {
            (Some(tag), Some(digest)) => conn.execute(
                "DELETE FROM image WHERE ref_registry = ?1 AND ref_repository = ?2 AND ref_tag = ?3 AND ref_digest = ?4",
                (registry, repository, tag, digest),
            )?,
            (Some(tag), None) => conn.execute(
                "DELETE FROM image WHERE ref_registry = ?1 AND ref_repository = ?2 AND ref_tag = ?3",
                (registry, repository, tag),
            )?,
            (None, Some(digest)) => conn.execute(
                "DELETE FROM image WHERE ref_registry = ?1 AND ref_repository = ?2 AND ref_digest = ?3",
                (registry, repository, digest),
            )?,
            (None, None) => conn.execute(
                "DELETE FROM image WHERE ref_registry = ?1 AND ref_repository = ?2",
                (registry, repository),
            )?,
        };
        Ok(rows_affected > 0)
    }

    /// Creates a new ImageEntry for testing purposes.
    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn new_for_testing(
        ref_registry: String,
        ref_repository: String,
        ref_tag: Option<String>,
        ref_digest: Option<String>,
        size_on_disk: u64,
    ) -> Self {
        Self {
            id: 0,
            ref_registry,
            ref_repository,
            ref_mirror_registry: None,
            ref_tag,
            ref_digest,
            manifest: Self::test_manifest(),
            size_on_disk,
        }
    }

    /// Creates a minimal OCI image manifest with a single WASM layer for testing.
    ///
    /// The manifest uses placeholder digests and sizes that are valid but not
    /// representative of real content.
    #[cfg(any(test, feature = "test-helpers"))]
    fn test_manifest() -> OciImageManifest {
        OciImageManifest {
            schema_version: 2,
            media_type: Some(IMAGE_MANIFEST_MEDIA_TYPE.to_string()),
            config: OciDescriptor {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: "sha256:abc123".to_string(),
                size: 100,
                urls: None,
                annotations: None,
            },
            layers: vec![OciDescriptor {
                media_type: "application/wasm".to_string(),
                digest: "sha256:def456".to_string(),
                size: 1024,
                urls: None,
                annotations: None,
            }],
            artifact_type: None,
            annotations: None,
            subject: None,
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

    /// Create a minimal valid manifest JSON string for testing.
    fn test_manifest() -> String {
        r#"{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"mediaType":"application/vnd.oci.image.config.v1+json","digest":"sha256:abc123","size":100},"layers":[]}"#.to_string()
    }

    // =========================================================================
    // ImageEntry Tests
    // =========================================================================

    #[test]
    fn test_image_entry_insert_new() {
        let conn = setup_test_db();

        let result = ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v1.0.0"),
            None,
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();

        assert_eq!(result.0, InsertResult::Inserted);
        assert!(result.1.is_some());
    }

    #[test]
    fn test_image_entry_insert_duplicate() {
        let conn = setup_test_db();

        // Insert first time
        let result1 = ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v1.0.0"),
            None,
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();
        assert_eq!(result1.0, InsertResult::Inserted);
        assert!(result1.1.is_some());

        // Insert duplicate
        let result2 = ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v1.0.0"),
            None,
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();
        assert_eq!(result2.0, InsertResult::AlreadyExists);
        assert!(result2.1.is_none());
    }

    #[test]
    fn test_image_entry_insert_different_tags() {
        let conn = setup_test_db();

        // Insert with tag v1
        let result1 = ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v1.0.0"),
            None,
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();
        assert_eq!(result1.0, InsertResult::Inserted);
        assert!(result1.1.is_some());

        // Insert with tag v2 - should succeed (different tag)
        let result2 = ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v2.0.0"),
            None,
            &test_manifest(),
            2048,
            "component",
        )
        .unwrap();
        assert_eq!(result2.0, InsertResult::Inserted);
        assert!(result2.1.is_some());

        // Verify both exist
        let entries = ImageEntry::get_all(&conn).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_image_entry_exists() {
        let conn = setup_test_db();

        // Initially doesn't exist
        assert!(!ImageEntry::exists(&conn, "ghcr.io", "user/repo", Some("v1.0.0"), None).unwrap());

        // Insert
        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v1.0.0"),
            None,
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();

        // Now exists
        assert!(ImageEntry::exists(&conn, "ghcr.io", "user/repo", Some("v1.0.0"), None).unwrap());

        // Different tag doesn't exist
        assert!(!ImageEntry::exists(&conn, "ghcr.io", "user/repo", Some("v2.0.0"), None).unwrap());
    }

    #[test]
    fn test_image_entry_exists_with_digest() {
        let conn = setup_test_db();

        // Insert with digest only
        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            None,
            Some("sha256:abc123"),
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();

        // Exists with digest
        assert!(
            ImageEntry::exists(&conn, "ghcr.io", "user/repo", None, Some("sha256:abc123")).unwrap()
        );

        // Different digest doesn't exist
        assert!(
            !ImageEntry::exists(&conn, "ghcr.io", "user/repo", None, Some("sha256:def456"))
                .unwrap()
        );
    }

    #[test]
    fn test_image_entry_exists_with_tag_and_digest() {
        let conn = setup_test_db();

        // Insert with both tag and digest
        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v1.0.0"),
            Some("sha256:abc123"),
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();

        // Exists with both
        assert!(
            ImageEntry::exists(
                &conn,
                "ghcr.io",
                "user/repo",
                Some("v1.0.0"),
                Some("sha256:abc123")
            )
            .unwrap()
        );

        // Wrong digest doesn't match
        assert!(
            !ImageEntry::exists(
                &conn,
                "ghcr.io",
                "user/repo",
                Some("v1.0.0"),
                Some("sha256:wrong")
            )
            .unwrap()
        );
    }

    #[test]
    fn test_image_entry_get_all_empty() {
        let conn = setup_test_db();

        let entries = ImageEntry::get_all(&conn).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_image_entry_get_all_ordered() {
        let conn = setup_test_db();

        // Insert in non-alphabetical order
        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "zebra/repo",
            Some("v1.0.0"),
            None,
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();
        ImageEntry::insert(
            &conn,
            "docker.io",
            "apple/repo",
            Some("latest"),
            None,
            &test_manifest(),
            2048,
            "component",
        )
        .unwrap();

        let entries = ImageEntry::get_all(&conn).unwrap();
        assert_eq!(entries.len(), 2);
        // Should be ordered by repository ASC
        assert_eq!(entries[0].ref_repository, "apple/repo");
        assert_eq!(entries[1].ref_repository, "zebra/repo");
    }

    #[test]
    fn test_image_entry_delete_by_reference_with_tag() {
        let conn = setup_test_db();

        // Insert
        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v1.0.0"),
            None,
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();

        // Delete
        let deleted =
            ImageEntry::delete_by_reference(&conn, "ghcr.io", "user/repo", Some("v1.0.0"), None)
                .unwrap();
        assert!(deleted);

        // Verify gone
        assert!(ImageEntry::get_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn test_image_entry_delete_by_reference_not_found() {
        let conn = setup_test_db();

        // Try to delete non-existent
        let deleted = ImageEntry::delete_by_reference(
            &conn,
            "ghcr.io",
            "nonexistent/repo",
            Some("v1.0.0"),
            None,
        )
        .unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_image_entry_delete_by_reference_with_digest() {
        let conn = setup_test_db();

        // Insert with digest
        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            None,
            Some("sha256:abc123"),
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();

        // Delete by digest
        let deleted = ImageEntry::delete_by_reference(
            &conn,
            "ghcr.io",
            "user/repo",
            None,
            Some("sha256:abc123"),
        )
        .unwrap();
        assert!(deleted);

        assert!(ImageEntry::get_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn test_image_entry_delete_by_registry_repository_only() {
        let conn = setup_test_db();

        // Insert multiple entries for same repo
        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v1.0.0"),
            None,
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();
        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v2.0.0"),
            None,
            &test_manifest(),
            2048,
            "component",
        )
        .unwrap();

        // Delete all by registry/repository only
        let deleted =
            ImageEntry::delete_by_reference(&conn, "ghcr.io", "user/repo", None, None).unwrap();
        assert!(deleted);

        assert!(ImageEntry::get_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn test_image_entry_reference_with_tag() {
        let conn = setup_test_db();

        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v1.0.0"),
            None,
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();

        let entries = ImageEntry::get_all(&conn).unwrap();
        assert_eq!(entries[0].reference(), "ghcr.io/user/repo:v1.0.0");
    }

    #[test]
    fn test_image_entry_reference_with_digest() {
        let conn = setup_test_db();

        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            None,
            Some("sha256:abc123"),
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();

        let entries = ImageEntry::get_all(&conn).unwrap();
        assert_eq!(entries[0].reference(), "ghcr.io/user/repo@sha256:abc123");
    }

    #[test]
    fn test_image_entry_reference_plain() {
        let conn = setup_test_db();

        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            None,
            None,
            &test_manifest(),
            1024,
            "component",
        )
        .unwrap();

        let entries = ImageEntry::get_all(&conn).unwrap();
        assert_eq!(entries[0].reference(), "ghcr.io/user/repo");
    }

    #[test]
    fn test_image_entry_size_on_disk() {
        let conn = setup_test_db();

        ImageEntry::insert(
            &conn,
            "ghcr.io",
            "user/repo",
            Some("v1.0.0"),
            None,
            &test_manifest(),
            12345678,
            "component",
        )
        .unwrap();

        let entries = ImageEntry::get_all(&conn).unwrap();
        assert_eq!(entries[0].size_on_disk, 12345678);
    }
}
