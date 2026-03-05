use oci_client::manifest::OciImageManifest;
use rusqlite::Connection;

/// Metadata for a stored OCI image.
///
/// This is an internal type constructed by joining `oci_manifest`, `oci_repository`,
/// and optionally `oci_tag`. It is not backed by its own table.
///
/// The public API exposes [`super::ImageEntry`] instead, which strips away
/// internal IDs.
#[derive(Debug, Clone)]
pub struct RawImageEntry {
    #[allow(dead_code)]
    id: i64,
    /// Registry hostname
    pub ref_registry: String,
    /// Repository path
    pub ref_repository: String,
    /// Optional mirror registry hostname (always `None` in the new schema)
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

impl RawImageEntry {
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

    /// Returns all stored images by joining `oci_manifest` with `oci_repository`
    /// and optionally `oci_tag`, ordered alphabetically by repository.
    pub(crate) fn get_all(conn: &Connection) -> anyhow::Result<Vec<RawImageEntry>> {
        let mut stmt = conn.prepare(
            "SELECT m.id, r.registry, r.repository, m.digest, m.raw_json, m.size_bytes,
                    (SELECT t.tag FROM oci_tag t
                     WHERE t.oci_repository_id = r.id AND t.manifest_digest = m.digest
                     ORDER BY t.updated_at DESC LIMIT 1) as tag
             FROM oci_manifest m
             JOIN oci_repository r ON m.oci_repository_id = r.id
             WHERE m.raw_json IS NOT NULL
             ORDER BY r.repository ASC, r.registry ASC",
        )?;

        let mut entries = Vec::new();
        let rows = stmt.query_map([], |row| {
            let raw_json: Option<String> = row.get(4)?;
            let size_bytes: Option<i64> = row.get(5)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                raw_json,
                size_bytes,
                row.get::<_, Option<String>>(6)?,
            ))
        })?;

        for row in rows {
            let (id, registry, repository, digest, raw_json, size_bytes, tag) = row?;
            let Some(json) = raw_json else {
                continue;
            };
            let manifest = match serde_json::from_str::<OciImageManifest>(&json) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("Skipping manifest {digest} in {registry}/{repository}: {e}");
                    continue;
                }
            };
            entries.push(RawImageEntry {
                id,
                ref_registry: registry,
                ref_repository: repository,
                ref_mirror_registry: None,
                ref_tag: tag,
                ref_digest: Some(digest),
                manifest,
                size_on_disk: u64::try_from(size_bytes.unwrap_or(0)).unwrap_or(0),
            });
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Migrations;

    /// Create an in-memory database with migrations applied for testing.
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();
        conn
    }

    #[test]
    fn test_image_entry_get_all_empty() {
        let conn = setup_test_db();
        let entries = RawImageEntry::get_all(&conn).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_image_entry_reference_with_tag() {
        let entry = RawImageEntry {
            id: 1,
            ref_registry: "ghcr.io".to_string(),
            ref_repository: "user/repo".to_string(),
            ref_mirror_registry: None,
            ref_tag: Some("latest".to_string()),
            ref_digest: Some("sha256:abc".to_string()),
            manifest: OciImageManifest::default(),
            size_on_disk: 1024,
        };
        assert_eq!(entry.reference(), "ghcr.io/user/repo:latest");
    }

    #[test]
    fn test_image_entry_reference_with_digest_only() {
        let entry = RawImageEntry {
            id: 1,
            ref_registry: "ghcr.io".to_string(),
            ref_repository: "user/repo".to_string(),
            ref_mirror_registry: None,
            ref_tag: None,
            ref_digest: Some("sha256:abc123".to_string()),
            manifest: OciImageManifest::default(),
            size_on_disk: 512,
        };
        assert_eq!(entry.reference(), "ghcr.io/user/repo@sha256:abc123");
    }

    #[test]
    fn test_image_entry_reference_bare() {
        let entry = RawImageEntry {
            id: 1,
            ref_registry: "ghcr.io".to_string(),
            ref_repository: "user/repo".to_string(),
            ref_mirror_registry: None,
            ref_tag: None,
            ref_digest: None,
            manifest: OciImageManifest::default(),
            size_on_disk: 0,
        };
        assert_eq!(entry.reference(), "ghcr.io/user/repo");
    }

    #[test]
    fn test_image_entry_get_all_with_valid_manifest() {
        use crate::oci::{OciManifest, OciRepository, OciTag};
        use std::collections::HashMap;

        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        let manifest_json = serde_json::to_string(&OciImageManifest::default()).unwrap();
        let (_mid, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:valid",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some(&manifest_json),
            Some(2048),
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        OciTag::upsert(&conn, repo_id, "v1.0", "sha256:valid").unwrap();

        let entries = RawImageEntry::get_all(&conn).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ref_registry, "ghcr.io");
        assert_eq!(entries[0].ref_repository, "user/repo");
        assert_eq!(entries[0].ref_tag.as_deref(), Some("v1.0"));
        assert_eq!(entries[0].ref_digest.as_deref(), Some("sha256:valid"));
        assert_eq!(entries[0].size_on_disk, 2048);
    }

    #[test]
    fn test_image_entry_get_all_skips_invalid_json() {
        use crate::oci::{OciManifest, OciRepository};
        use std::collections::HashMap;

        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        // Insert manifest with invalid JSON
        OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:badjson",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{not valid json}"),
            Some(100),
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        let entries = RawImageEntry::get_all(&conn).unwrap();
        assert!(
            entries.is_empty(),
            "invalid JSON manifests should be skipped"
        );
    }
}
