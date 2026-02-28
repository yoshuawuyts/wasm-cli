use oci_client::manifest::OciImageManifest;
#[cfg(any(test, feature = "test-helpers"))]
use oci_client::manifest::{IMAGE_MANIFEST_MEDIA_TYPE, OciDescriptor};
use rusqlite::Connection;

/// Metadata for a stored OCI image.
///
/// This is a view type constructed by joining `oci_manifest`, `oci_repository`,
/// and optionally `oci_tag`. It is not backed by its own table.
#[derive(Debug, Clone)]
pub struct ImageEntry {
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

    /// Returns all stored images by joining `oci_manifest` with `oci_repository`
    /// and optionally `oci_tag`, ordered alphabetically by repository.
    pub(crate) fn get_all(conn: &Connection) -> anyhow::Result<Vec<ImageEntry>> {
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
            entries.push(ImageEntry {
                id,
                ref_registry: registry,
                ref_repository: repository,
                ref_mirror_registry: None,
                ref_tag: tag,
                ref_digest: Some(digest),
                manifest,
                size_on_disk: size_bytes.unwrap_or(0) as u64,
            });
        }
        Ok(entries)
    }

    /// Creates a new `ImageEntry` for testing purposes.
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

    #[test]
    fn test_image_entry_get_all_empty() {
        let conn = setup_test_db();
        let entries = ImageEntry::get_all(&conn).unwrap();
        assert!(entries.is_empty());
    }
}
