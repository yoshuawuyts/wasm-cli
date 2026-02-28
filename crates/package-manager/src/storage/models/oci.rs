use std::collections::HashMap;

use rusqlite::Connection;

/// Well-known OCI annotation keys that are extracted into dedicated columns.
const WELL_KNOWN_ANNOTATIONS: &[(&str, &str)] = &[
    ("org.opencontainers.image.created", "oci_created"),
    ("org.opencontainers.image.authors", "oci_authors"),
    ("org.opencontainers.image.url", "oci_url"),
    (
        "org.opencontainers.image.documentation",
        "oci_documentation",
    ),
    ("org.opencontainers.image.source", "oci_source"),
    ("org.opencontainers.image.version", "oci_version"),
    ("org.opencontainers.image.revision", "oci_revision"),
    ("org.opencontainers.image.vendor", "oci_vendor"),
    ("org.opencontainers.image.licenses", "oci_licenses"),
    ("org.opencontainers.image.ref.name", "oci_ref_name"),
    ("org.opencontainers.image.title", "oci_title"),
    ("org.opencontainers.image.description", "oci_description"),
    ("org.opencontainers.image.base.digest", "oci_base_digest"),
    ("org.opencontainers.image.base.name", "oci_base_name"),
];

/// Result of an insert operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertResult {
    /// The entry was inserted successfully.
    Inserted,
    /// The entry already existed in the database.
    AlreadyExists,
}

// ---------------------------------------------------------------------------
// OciRepository
// ---------------------------------------------------------------------------

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
        conn.execute(
            "INSERT INTO oci_repository (registry, repository)
             VALUES (?1, ?2)
             ON CONFLICT(registry, repository) DO UPDATE SET
                 updated_at = CURRENT_TIMESTAMP",
            (registry, repository),
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

    /// Creates a new `OciRepository` for testing purposes.
    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn new_for_testing(
        registry: String,
        repository: String,
        created_at: String,
        updated_at: String,
    ) -> Self {
        Self {
            id: 0,
            registry,
            repository,
            created_at,
            updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// OciManifest
// ---------------------------------------------------------------------------

/// An OCI image manifest stored in the database.
///
/// Many annotation fields are not yet consumed by the current code paths
/// but are populated from OCI manifests for completeness.
#[derive(Debug, Clone)]
#[allow(dead_code, unreachable_pub)]
pub struct OciManifest {
    id: i64,
    /// Foreign key to `oci_repository`.
    pub oci_repository_id: i64,
    /// Content-addressable digest.
    pub digest: String,
    /// MIME type of the manifest.
    pub media_type: Option<String>,
    /// Raw JSON body.
    pub raw_json: Option<String>,
    /// Total size in bytes.
    pub size_bytes: Option<i64>,
    /// When the row was created.
    pub created_at: String,
    /// Artifact type, if present.
    pub artifact_type: Option<String>,
    /// Config descriptor media type.
    pub config_media_type: Option<String>,
    /// Config descriptor digest.
    pub config_digest: Option<String>,
    /// `org.opencontainers.image.created`
    pub oci_created: Option<String>,
    /// `org.opencontainers.image.authors`
    pub oci_authors: Option<String>,
    /// `org.opencontainers.image.url`
    pub oci_url: Option<String>,
    /// `org.opencontainers.image.documentation`
    pub oci_documentation: Option<String>,
    /// `org.opencontainers.image.source`
    pub oci_source: Option<String>,
    /// `org.opencontainers.image.version`
    pub oci_version: Option<String>,
    /// `org.opencontainers.image.revision`
    pub oci_revision: Option<String>,
    /// `org.opencontainers.image.vendor`
    pub oci_vendor: Option<String>,
    /// `org.opencontainers.image.licenses`
    pub oci_licenses: Option<String>,
    /// `org.opencontainers.image.ref.name`
    pub oci_ref_name: Option<String>,
    /// `org.opencontainers.image.title`
    pub oci_title: Option<String>,
    /// `org.opencontainers.image.description`
    pub oci_description: Option<String>,
    /// `org.opencontainers.image.base.digest`
    pub oci_base_digest: Option<String>,
    /// `org.opencontainers.image.base.name`
    pub oci_base_name: Option<String>,
}

impl OciManifest {
    /// Returns the primary key.
    #[must_use]
    #[allow(unreachable_pub)]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Insert a manifest and its annotations.
    ///
    /// Well-known OCI annotation keys are extracted into dedicated columns;
    /// remaining annotations are stored in `oci_manifest_annotation`.
    ///
    /// Uses `INSERT … ON CONFLICT DO NOTHING` so duplicate digests within the
    /// same repository are silently skipped. Returns `(manifest_id, was_inserted)`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn upsert(
        conn: &Connection,
        oci_repository_id: i64,
        digest: &str,
        media_type: Option<&str>,
        raw_json: Option<&str>,
        size_bytes: Option<i64>,
        artifact_type: Option<&str>,
        config_media_type: Option<&str>,
        config_digest: Option<&str>,
        annotations: &HashMap<String, String>,
    ) -> anyhow::Result<(i64, bool)> {
        // Partition annotations into well-known and extra.
        let ann_key_to_col: HashMap<&str, &str> = WELL_KNOWN_ANNOTATIONS.iter().copied().collect();
        let mut well_known: HashMap<&str, &str> = HashMap::new();
        let mut extra: Vec<(&str, &str)> = Vec::new();

        for (k, v) in annotations {
            if let Some(&col) = ann_key_to_col.get(k.as_str()) {
                well_known.insert(col, v.as_str());
            } else {
                extra.push((k.as_str(), v.as_str()));
            }
        }

        let rows_inserted = conn.execute(
            "INSERT INTO oci_manifest (
                oci_repository_id, digest, media_type, raw_json, size_bytes,
                artifact_type, config_media_type, config_digest,
                oci_created, oci_authors, oci_url, oci_documentation, oci_source,
                oci_version, oci_revision, oci_vendor, oci_licenses, oci_ref_name,
                oci_title, oci_description, oci_base_digest, oci_base_name
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18,
                ?19, ?20, ?21, ?22
             )
             ON CONFLICT(oci_repository_id, digest) DO NOTHING",
            rusqlite::params![
                oci_repository_id,
                digest,
                media_type,
                raw_json,
                size_bytes,
                artifact_type,
                config_media_type,
                config_digest,
                well_known.get("oci_created").copied(),
                well_known.get("oci_authors").copied(),
                well_known.get("oci_url").copied(),
                well_known.get("oci_documentation").copied(),
                well_known.get("oci_source").copied(),
                well_known.get("oci_version").copied(),
                well_known.get("oci_revision").copied(),
                well_known.get("oci_vendor").copied(),
                well_known.get("oci_licenses").copied(),
                well_known.get("oci_ref_name").copied(),
                well_known.get("oci_title").copied(),
                well_known.get("oci_description").copied(),
                well_known.get("oci_base_digest").copied(),
                well_known.get("oci_base_name").copied(),
            ],
        )?;

        let was_inserted = rows_inserted > 0;

        // Retrieve the canonical row id.
        let manifest_id: i64 = conn.query_row(
            "SELECT id FROM oci_manifest WHERE oci_repository_id = ?1 AND digest = ?2",
            (oci_repository_id, digest),
            |row| row.get(0),
        )?;

        // Store extra (non-well-known) annotations (only on insert).
        if was_inserted {
            for (key, value) in &extra {
                conn.execute(
                    "INSERT INTO oci_manifest_annotation (oci_manifest_id, `key`, `value`)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(oci_manifest_id, `key`) DO UPDATE SET `value` = ?3",
                    rusqlite::params![manifest_id, key, value],
                )?;
            }
        }

        Ok((manifest_id, was_inserted))
    }

    /// Get a manifest by primary key.
    #[allow(dead_code)]
    pub(crate) fn get_by_id(conn: &Connection, id: i64) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, oci_repository_id, digest, media_type, raw_json, size_bytes,
                    created_at, artifact_type, config_media_type, config_digest,
                    oci_created, oci_authors, oci_url, oci_documentation, oci_source,
                    oci_version, oci_revision, oci_vendor, oci_licenses, oci_ref_name,
                    oci_title, oci_description, oci_base_digest, oci_base_name
             FROM oci_manifest WHERE id = ?1",
            [id],
            Self::from_row,
        );

        match result {
            Ok(m) => Ok(Some(m)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find a manifest by repository id and digest.
    pub(crate) fn find(
        conn: &Connection,
        oci_repository_id: i64,
        digest: &str,
    ) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, oci_repository_id, digest, media_type, raw_json, size_bytes,
                    created_at, artifact_type, config_media_type, config_digest,
                    oci_created, oci_authors, oci_url, oci_documentation, oci_source,
                    oci_version, oci_revision, oci_vendor, oci_licenses, oci_ref_name,
                    oci_title, oci_description, oci_base_digest, oci_base_name
             FROM oci_manifest WHERE oci_repository_id = ?1 AND digest = ?2",
            (oci_repository_id, digest),
            Self::from_row,
        );

        match result {
            Ok(m) => Ok(Some(m)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all manifests belonging to a repository.
    pub(crate) fn list_by_repository(
        conn: &Connection,
        oci_repository_id: i64,
    ) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, oci_repository_id, digest, media_type, raw_json, size_bytes,
                    created_at, artifact_type, config_media_type, config_digest,
                    oci_created, oci_authors, oci_url, oci_documentation, oci_source,
                    oci_version, oci_revision, oci_vendor, oci_licenses, oci_ref_name,
                    oci_title, oci_description, oci_base_digest, oci_base_name
             FROM oci_manifest WHERE oci_repository_id = ?1 ORDER BY created_at ASC",
        )?;

        let rows = stmt.query_map([oci_repository_id], Self::from_row)?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Delete a manifest by primary key. Returns `true` if a row was removed.
    pub(crate) fn delete(conn: &Connection, id: i64) -> anyhow::Result<bool> {
        let rows = conn.execute("DELETE FROM oci_manifest WHERE id = ?1", [id])?;
        Ok(rows > 0)
    }

    /// Map a `rusqlite::Row` to `Self`.
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            oci_repository_id: row.get(1)?,
            digest: row.get(2)?,
            media_type: row.get(3)?,
            raw_json: row.get(4)?,
            size_bytes: row.get(5)?,
            created_at: row.get(6)?,
            artifact_type: row.get(7)?,
            config_media_type: row.get(8)?,
            config_digest: row.get(9)?,
            oci_created: row.get(10)?,
            oci_authors: row.get(11)?,
            oci_url: row.get(12)?,
            oci_documentation: row.get(13)?,
            oci_source: row.get(14)?,
            oci_version: row.get(15)?,
            oci_revision: row.get(16)?,
            oci_vendor: row.get(17)?,
            oci_licenses: row.get(18)?,
            oci_ref_name: row.get(19)?,
            oci_title: row.get(20)?,
            oci_description: row.get(21)?,
            oci_base_digest: row.get(22)?,
            oci_base_name: row.get(23)?,
        })
    }

    /// Creates a new `OciManifest` for testing purposes.
    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn new_for_testing(oci_repository_id: i64, digest: String) -> Self {
        Self {
            id: 0,
            oci_repository_id,
            digest,
            media_type: None,
            raw_json: None,
            size_bytes: None,
            created_at: String::new(),
            artifact_type: None,
            config_media_type: None,
            config_digest: None,
            oci_created: None,
            oci_authors: None,
            oci_url: None,
            oci_documentation: None,
            oci_source: None,
            oci_version: None,
            oci_revision: None,
            oci_vendor: None,
            oci_licenses: None,
            oci_ref_name: None,
            oci_title: None,
            oci_description: None,
            oci_base_digest: None,
            oci_base_name: None,
        }
    }
}

// ---------------------------------------------------------------------------
// OciTag
// ---------------------------------------------------------------------------

/// A tag pointing at a manifest within a repository.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct OciTag {
    #[allow(dead_code)]
    id: i64,
    /// Foreign key to `oci_repository`.
    #[allow(dead_code)]
    pub oci_repository_id: i64,
    /// Digest of the manifest this tag references.
    pub manifest_digest: String,
    /// The tag string (e.g. "latest", "v1.0.0").
    #[allow(dead_code)]
    pub tag: String,
    /// When the row was created.
    #[allow(dead_code)]
    pub created_at: String,
    /// When the row was last updated.
    #[allow(dead_code)]
    pub updated_at: String,
}

impl OciTag {
    /// Insert or update a tag, returning its row id.
    pub(crate) fn upsert(
        conn: &Connection,
        oci_repository_id: i64,
        tag: &str,
        manifest_digest: &str,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO oci_tag (oci_repository_id, tag, manifest_digest)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(oci_repository_id, tag) DO UPDATE SET
                 manifest_digest = ?3,
                 updated_at = CURRENT_TIMESTAMP",
            (oci_repository_id, tag, manifest_digest),
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM oci_tag WHERE oci_repository_id = ?1 AND tag = ?2",
            (oci_repository_id, tag),
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// List all tags for a repository.
    #[allow(dead_code)]
    pub(crate) fn list_by_repository(
        conn: &Connection,
        oci_repository_id: i64,
    ) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, oci_repository_id, manifest_digest, tag, created_at, updated_at
             FROM oci_tag WHERE oci_repository_id = ?1 ORDER BY tag ASC",
        )?;

        let rows = stmt.query_map([oci_repository_id], |row| {
            Ok(Self {
                id: row.get(0)?,
                oci_repository_id: row.get(1)?,
                manifest_digest: row.get(2)?,
                tag: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Find a tag by repository id and tag name.
    pub(crate) fn find_by_tag(
        conn: &Connection,
        oci_repository_id: i64,
        tag: &str,
    ) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, oci_repository_id, manifest_digest, tag, created_at, updated_at
             FROM oci_tag WHERE oci_repository_id = ?1 AND tag = ?2",
            (oci_repository_id, tag),
            |row| {
                Ok(Self {
                    id: row.get(0)?,
                    oci_repository_id: row.get(1)?,
                    manifest_digest: row.get(2)?,
                    tag: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Creates a new `OciTag` for testing purposes.
    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn new_for_testing(
        oci_repository_id: i64,
        tag: String,
        manifest_digest: String,
    ) -> Self {
        Self {
            id: 0,
            oci_repository_id,
            manifest_digest,
            tag,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// OciLayer
// ---------------------------------------------------------------------------

/// A layer (blob) within an OCI manifest.
#[derive(Debug, Clone)]
#[allow(unreachable_pub)]
pub struct OciLayer {
    #[allow(dead_code)]
    id: i64,
    /// Foreign key to `oci_manifest`.
    #[allow(dead_code)]
    pub oci_manifest_id: i64,
    /// Content-addressable digest.
    pub digest: String,
    /// MIME type of the layer.
    #[allow(dead_code)]
    pub media_type: Option<String>,
    /// Size in bytes.
    #[allow(dead_code)]
    pub size_bytes: Option<i64>,
    /// Ordinal position within the manifest.
    #[allow(dead_code)]
    pub position: i32,
}

impl OciLayer {
    /// Insert a new layer, returning its row id.
    pub(crate) fn insert(
        conn: &Connection,
        oci_manifest_id: i64,
        digest: &str,
        media_type: Option<&str>,
        size_bytes: Option<i64>,
        position: i32,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO oci_layer (oci_manifest_id, digest, media_type, size_bytes, position)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![oci_manifest_id, digest, media_type, size_bytes, position],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// List all layers for a manifest, ordered by position.
    pub(crate) fn list_by_manifest(
        conn: &Connection,
        oci_manifest_id: i64,
    ) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, oci_manifest_id, digest, media_type, size_bytes, position
             FROM oci_layer WHERE oci_manifest_id = ?1 ORDER BY position ASC",
        )?;

        let rows = stmt.query_map([oci_manifest_id], |row| {
            Ok(Self {
                id: row.get(0)?,
                oci_manifest_id: row.get(1)?,
                digest: row.get(2)?,
                media_type: row.get(3)?,
                size_bytes: row.get(4)?,
                position: row.get(5)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Find a layer by manifest id and digest.
    #[allow(dead_code)]
    pub(crate) fn get_by_digest(
        conn: &Connection,
        oci_manifest_id: i64,
        digest: &str,
    ) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT id, oci_manifest_id, digest, media_type, size_bytes, position
             FROM oci_layer WHERE oci_manifest_id = ?1 AND digest = ?2",
            (oci_manifest_id, digest),
            |row| {
                Ok(Self {
                    id: row.get(0)?,
                    oci_manifest_id: row.get(1)?,
                    digest: row.get(2)?,
                    media_type: row.get(3)?,
                    size_bytes: row.get(4)?,
                    position: row.get(5)?,
                })
            },
        );

        match result {
            Ok(l) => Ok(Some(l)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Creates a new `OciLayer` for testing purposes.
    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn new_for_testing(
        oci_manifest_id: i64,
        digest: String,
        media_type: Option<String>,
        size_bytes: Option<i64>,
        position: i32,
    ) -> Self {
        Self {
            id: 0,
            oci_manifest_id,
            digest,
            media_type,
            size_bytes,
            position,
        }
    }
}

// ---------------------------------------------------------------------------
// OciReferrer
// ---------------------------------------------------------------------------

/// A referrer relationship between two manifests.
#[derive(Debug, Clone)]
#[allow(dead_code, unreachable_pub)]
pub struct OciReferrer {
    #[allow(dead_code)]
    id: i64,
    /// The manifest that is being referred to.
    pub subject_manifest_id: i64,
    /// The manifest doing the referring.
    pub referrer_manifest_id: i64,
    /// The artifact type of the referrer.
    pub artifact_type: String,
    /// When the row was created.
    pub created_at: String,
}

#[allow(dead_code)]
impl OciReferrer {
    /// Insert a new referrer relationship, returning its row id.
    pub(crate) fn insert(
        conn: &Connection,
        subject_manifest_id: i64,
        referrer_manifest_id: i64,
        artifact_type: &str,
    ) -> anyhow::Result<i64> {
        conn.execute(
            "INSERT INTO oci_referrer (subject_manifest_id, referrer_manifest_id, artifact_type)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(subject_manifest_id, referrer_manifest_id) DO NOTHING",
            (subject_manifest_id, referrer_manifest_id, artifact_type),
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM oci_referrer
             WHERE subject_manifest_id = ?1 AND referrer_manifest_id = ?2",
            (subject_manifest_id, referrer_manifest_id),
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// List all referrers for a given subject manifest.
    pub(crate) fn list_by_subject(
        conn: &Connection,
        subject_manifest_id: i64,
    ) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, subject_manifest_id, referrer_manifest_id, artifact_type, created_at
             FROM oci_referrer WHERE subject_manifest_id = ?1 ORDER BY created_at ASC",
        )?;

        let rows = stmt.query_map([subject_manifest_id], |row| {
            Ok(Self {
                id: row.get(0)?,
                subject_manifest_id: row.get(1)?,
                referrer_manifest_id: row.get(2)?,
                artifact_type: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Creates a new `OciReferrer` for testing purposes.
    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn new_for_testing(
        subject_manifest_id: i64,
        referrer_manifest_id: i64,
        artifact_type: String,
    ) -> Self {
        Self {
            id: 0,
            subject_manifest_id,
            referrer_manifest_id,
            artifact_type,
            created_at: String::new(),
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
    fn test_oci_repository_upsert_and_find() {
        let conn = setup_test_db();
        let id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();
        assert!(id > 0);

        let repo = OciRepository::find(&conn, "ghcr.io", "user/repo")
            .unwrap()
            .unwrap();
        assert_eq!(repo.id(), id);
    }

    #[test]
    fn test_oci_repository_upsert_idempotent() {
        let conn = setup_test_db();
        let id1 = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();
        let id2 = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_oci_manifest_upsert_and_find() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        let annotations = HashMap::new();
        let (mid, was_inserted) = OciManifest::upsert(
            &conn,
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
        assert!(was_inserted);
        assert!(mid > 0);

        // Re-inserting same digest should not insert
        let (mid2, was_inserted2) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:abc123",
            None,
            None,
            None,
            None,
            None,
            None,
            &annotations,
        )
        .unwrap();
        assert!(!was_inserted2);
        assert_eq!(mid, mid2);

        let manifest = OciManifest::find(&conn, repo_id, "sha256:abc123")
            .unwrap()
            .unwrap();
        assert_eq!(manifest.id(), mid);
    }

    #[test]
    fn test_oci_manifest_upsert_extracts_annotations() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        let mut annotations = HashMap::new();
        annotations.insert(
            "org.opencontainers.image.description".to_string(),
            "A test image".to_string(),
        );
        annotations.insert("custom.key".to_string(), "custom-value".to_string());

        let (mid, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:desc123",
            None,
            None,
            None,
            None,
            None,
            None,
            &annotations,
        )
        .unwrap();

        let manifest = OciManifest::find(&conn, repo_id, "sha256:desc123")
            .unwrap()
            .unwrap();
        assert_eq!(manifest.oci_description.as_deref(), Some("A test image"));

        // Check extra annotation was stored
        let custom: String = conn
            .query_row(
                "SELECT `value` FROM oci_manifest_annotation
                 WHERE oci_manifest_id = ?1 AND `key` = 'custom.key'",
                [mid],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(custom, "custom-value");
    }

    #[test]
    fn test_oci_tag_upsert_and_find() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:abc123",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        let tag_id = OciTag::upsert(&conn, repo_id, "latest", "sha256:abc123").unwrap();
        assert!(tag_id > 0);

        let tag = OciTag::find_by_tag(&conn, repo_id, "latest")
            .unwrap()
            .unwrap();
        assert_eq!(tag.manifest_digest, "sha256:abc123");

        // Update tag to point at a new digest
        OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:def456",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();
        OciTag::upsert(&conn, repo_id, "latest", "sha256:def456").unwrap();
        let tag = OciTag::find_by_tag(&conn, repo_id, "latest")
            .unwrap()
            .unwrap();
        assert_eq!(tag.manifest_digest, "sha256:def456");
    }

    #[test]
    fn test_oci_layer_insert_and_list() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();
        let (mid, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:abc",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        OciLayer::insert(
            &conn,
            mid,
            "sha256:layer1",
            Some("application/wasm"),
            Some(512),
            0,
        )
        .unwrap();
        OciLayer::insert(
            &conn,
            mid,
            "sha256:layer2",
            Some("application/octet-stream"),
            Some(256),
            1,
        )
        .unwrap();

        let layers = OciLayer::list_by_manifest(&conn, mid).unwrap();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers.first().unwrap().digest, "sha256:layer1");
        assert_eq!(layers.get(1).unwrap().digest, "sha256:layer2");
    }

    #[test]
    fn test_oci_manifest_delete_cascades() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();
        let (mid, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:abc",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        OciTag::upsert(&conn, repo_id, "v1", "sha256:abc").unwrap();
        OciLayer::insert(&conn, mid, "sha256:layer1", None, None, 0).unwrap();

        // Delete the manifest — tags and layers should cascade
        OciManifest::delete(&conn, mid).unwrap();

        let manifests = OciManifest::list_by_repository(&conn, repo_id).unwrap();
        assert!(manifests.is_empty());

        let layers = OciLayer::list_by_manifest(&conn, mid).unwrap();
        assert!(layers.is_empty());

        // Tag should also be gone (ON DELETE CASCADE)
        let tag = OciTag::find_by_tag(&conn, repo_id, "v1").unwrap();
        assert!(tag.is_none());
    }

    #[test]
    fn test_oci_manifest_upsert_stores_config_fields() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        let (mid, was_inserted) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:config123",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(2048),
            Some("application/vnd.example+type"),
            Some("application/vnd.oci.image.config.v1+json"),
            Some("sha256:configdigest"),
            &HashMap::new(),
        )
        .unwrap();
        assert!(was_inserted);

        let manifest = OciManifest::find(&conn, repo_id, "sha256:config123")
            .unwrap()
            .unwrap();
        assert_eq!(manifest.id(), mid);
        assert_eq!(
            manifest.artifact_type.as_deref(),
            Some("application/vnd.example+type")
        );
        assert_eq!(
            manifest.config_media_type.as_deref(),
            Some("application/vnd.oci.image.config.v1+json")
        );
        assert_eq!(
            manifest.config_digest.as_deref(),
            Some("sha256:configdigest")
        );
    }
}
