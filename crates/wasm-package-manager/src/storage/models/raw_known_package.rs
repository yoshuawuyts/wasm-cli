use rusqlite::Connection;

use wasm_meta_registry_types::PackageKind;

use crate::oci::OciRepository;
use crate::storage::KnownPackageParams;

/// A raw known package that persists in the database even after local deletion.
/// This is used to track packages the user has seen or searched for.
///
/// Backed by `oci_repository` in the new schema.  Tags come from `oci_tag`.
///
/// This is the internal database-backed type. The public API exposes
/// [`super::super::KnownPackage`] instead, which strips away internal IDs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RawKnownPackage {
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
    /// Optional WIT namespace (e.g. "ba", "wasi").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wit_namespace: Option<String>,
    /// Optional WIT package name within the namespace (e.g. "http").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wit_name: Option<String>,
    /// Package kind: component or interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PackageKind>,
}

impl RawKnownPackage {
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
        Self::upsert_with_params(
            conn,
            &KnownPackageParams {
                registry,
                repository,
                tag,
                description,
                wit_namespace: None,
                wit_name: None,
                kind: None,
            },
        )
    }

    /// Inserts or updates a known package with optional WIT namespace mapping
    /// and package kind.
    pub(crate) fn upsert_with_params(
        conn: &Connection,
        params: &KnownPackageParams<'_>,
    ) -> anyhow::Result<()> {
        let kind_str = params.kind.map(|k| k.to_string());
        let repo_id = OciRepository::upsert_full(
            conn,
            params.registry,
            params.repository,
            params.wit_namespace,
            params.wit_name,
            kind_str.as_deref(),
        )?;

        // Store description on the most recent manifest that doesn't have one.
        // Uses a subquery instead of LIMIT in UPDATE (SQLITE_ENABLE_UPDATE_DELETE_LIMIT
        // is not enabled in most SQLite builds).
        if let Some(desc) = params.description
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
        if let Some(tag) = params.tag {
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
        let Ok(mut stmt) = conn.prepare(
            "SELECT t.tag FROM oci_tag t
             WHERE t.oci_repository_id = ?1
             ORDER BY t.updated_at DESC",
        ) else {
            return Vec::new();
        };

        let Ok(rows) = stmt.query_map([repo_id], |row| row.get::<_, String>(0)) else {
            return Vec::new();
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
    /// Searches in registry, repository, and WIT metadata fields.
    pub(crate) fn search(
        conn: &Connection,
        query: &str,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        let search_pattern = format!("%{query}%");
        let mut stmt = conn.prepare(
            "SELECT id, registry, repository, updated_at, created_at,
                    wit_namespace, wit_name, kind
             FROM oci_repository
             WHERE registry LIKE ?1
                OR repository LIKE ?1
                OR wit_namespace LIKE ?1
                OR wit_name LIKE ?1
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
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;

        let mut packages = Vec::new();
        for row in rows {
            let (id, registry, repository, updated_at, created_at, wit_ns, wit_n, kind_str) = row?;
            let tags = Self::fetch_tags(conn, id);
            let description = Self::fetch_description(conn, id);
            packages.push(RawKnownPackage {
                id,
                registry,
                repository,
                description,
                tags,
                signature_tags: Vec::new(),
                attestation_tags: Vec::new(),
                last_seen_at: updated_at,
                created_at,
                wit_namespace: wit_ns,
                wit_name: wit_n,
                kind: parse_kind(kind_str.as_deref()),
            });
        }
        Ok(packages)
    }

    /// Get all known packages, ordered alphabetically by repository.
    pub(crate) fn get_all(
        conn: &Connection,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        let mut stmt = conn.prepare(
            "SELECT id, registry, repository, updated_at, created_at,
                    wit_namespace, wit_name, kind
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
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;

        let mut packages = Vec::new();
        for row in rows {
            let (id, registry, repository, updated_at, created_at, wit_ns, wit_n, kind_str) = row?;
            let tags = Self::fetch_tags(conn, id);
            let description = Self::fetch_description(conn, id);
            packages.push(RawKnownPackage {
                id,
                registry,
                repository,
                description,
                tags,
                signature_tags: Vec::new(),
                attestation_tags: Vec::new(),
                last_seen_at: updated_at,
                created_at,
                wit_namespace: wit_ns,
                wit_name: wit_n,
                kind: parse_kind(kind_str.as_deref()),
            });
        }
        Ok(packages)
    }

    /// Get known packages ordered by most recent update timestamp.
    pub(crate) fn get_recent(
        conn: &Connection,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        let mut stmt = conn.prepare(
            "SELECT id, registry, repository, updated_at, created_at,
                    wit_namespace, wit_name, kind
             FROM oci_repository
             ORDER BY updated_at DESC
             LIMIT ?2 OFFSET ?1",
        )?;

        let rows = stmt.query_map((offset, limit), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;

        let mut packages = Vec::new();
        for row in rows {
            let (id, registry, repository, updated_at, created_at, wit_ns, wit_n, kind_str) = row?;
            let tags = Self::fetch_tags(conn, id);
            let description = Self::fetch_description(conn, id);
            packages.push(RawKnownPackage {
                id,
                registry,
                repository,
                description,
                tags,
                signature_tags: Vec::new(),
                attestation_tags: Vec::new(),
                last_seen_at: updated_at,
                created_at,
                wit_namespace: wit_ns,
                wit_name: wit_n,
                kind: parse_kind(kind_str.as_deref()),
            });
        }
        Ok(packages)
    }

    /// Get a known package by registry and repository.
    pub(crate) fn get(
        conn: &Connection,
        registry: &str,
        repository: &str,
    ) -> anyhow::Result<Option<RawKnownPackage>> {
        let result = conn.query_row(
            "SELECT id, registry, repository, updated_at, created_at,
                    wit_namespace, wit_name, kind
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
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        );

        match result {
            Ok((id, reg, repo, updated_at, created_at, wit_ns, wit_n, kind_str)) => {
                let tags = Self::fetch_tags(conn, id);
                let description = Self::fetch_description(conn, id);
                Ok(Some(RawKnownPackage {
                    id,
                    registry: reg,
                    repository: repo,
                    description,
                    tags,
                    signature_tags: Vec::new(),
                    attestation_tags: Vec::new(),
                    last_seen_at: updated_at,
                    created_at,
                    wit_namespace: wit_ns,
                    wit_name: wit_n,
                    kind: parse_kind(kind_str.as_deref()),
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Search for known packages that import a given interface.
    ///
    /// Joins through `oci_manifest` → `wit_package` → `wit_world` → `wit_world_import`
    /// and matches on `declared_package` using exact equality.
    pub(crate) fn search_by_import(
        conn: &Connection,
        interface: &str,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT r.id, r.registry, r.repository, r.updated_at, r.created_at,
                    r.wit_namespace, r.wit_name, r.kind
             FROM oci_repository r
             JOIN oci_manifest m ON m.oci_repository_id = r.id
             JOIN wit_package wp ON wp.oci_manifest_id = m.id
             JOIN wit_world ww ON ww.wit_package_id = wp.id
             JOIN wit_world_import wi ON wi.wit_world_id = ww.id
             WHERE wi.declared_package = ?1
             ORDER BY r.repository ASC, r.registry ASC
             LIMIT ?2 OFFSET ?3",
        )?;

        Self::collect_repo_rows(conn, &mut stmt, (interface, limit, offset))
    }

    /// Search for known packages that export a given interface.
    ///
    /// Joins through `oci_manifest` → `wit_package` → `wit_world` → `wit_world_export`
    /// and matches on `declared_package` using exact equality.
    pub(crate) fn search_by_export(
        conn: &Connection,
        interface: &str,
        offset: u32,
        limit: u32,
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT r.id, r.registry, r.repository, r.updated_at, r.created_at,
                    r.wit_namespace, r.wit_name, r.kind
             FROM oci_repository r
             JOIN oci_manifest m ON m.oci_repository_id = r.id
             JOIN wit_package wp ON wp.oci_manifest_id = m.id
             JOIN wit_world ww ON ww.wit_package_id = wp.id
             JOIN wit_world_export we ON we.wit_world_id = ww.id
             WHERE we.declared_package = ?1
             ORDER BY r.repository ASC, r.registry ASC
             LIMIT ?2 OFFSET ?3",
        )?;

        Self::collect_repo_rows(conn, &mut stmt, (interface, limit, offset))
    }

    /// Execute a prepared statement that returns `(id, registry, repository,
    /// updated_at, created_at, wit_namespace, wit_name, kind)` rows and inflate each
    /// into a full `RawKnownPackage` with tags and description.
    fn collect_repo_rows(
        conn: &Connection,
        stmt: &mut rusqlite::Statement<'_>,
        params: (&str, u32, u32),
    ) -> anyhow::Result<Vec<RawKnownPackage>> {
        let rows = stmt.query_map(params, |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;

        let mut packages = Vec::new();
        for row in rows {
            let (id, registry, repository, updated_at, created_at, wit_ns, wit_n, kind_str) = row?;
            let tags = Self::fetch_tags(conn, id);
            let description = Self::fetch_description(conn, id);
            packages.push(RawKnownPackage {
                id,
                registry,
                repository,
                description,
                tags,
                signature_tags: Vec::new(),
                attestation_tags: Vec::new(),
                last_seen_at: updated_at,
                created_at,
                wit_namespace: wit_ns,
                wit_name: wit_n,
                kind: parse_kind(kind_str.as_deref()),
            });
        }
        Ok(packages)
    }

    /// Search for a known package by WIT package name.
    ///
    /// First tries an exact lookup using `wit_namespace` + `wit_name` columns.
    /// Falls back to a fuzzy `LIKE` search on the `repository` column for
    /// backwards compatibility with databases that haven't been re-synced yet.
    pub(crate) fn search_by_wit_name(
        conn: &Connection,
        wit_name: &str,
    ) -> anyhow::Result<Option<RawKnownPackage>> {
        // Split "ba:sample-wasi-http-rust" → ("ba", "sample-wasi-http-rust")
        let Some((namespace, name)) = wit_name.split_once(':') else {
            return Ok(None);
        };

        // 1. Exact lookup via wit_namespace + wit_name columns.
        if let Some(pkg) = Self::search_by_wit_columns(conn, namespace, name)? {
            return Ok(Some(pkg));
        }

        // 2. Fallback: fuzzy LIKE on the repository column.
        Self::search_by_repository_pattern(conn, wit_name)
    }

    /// Exact lookup using `wit_namespace` + `wit_name` columns.
    fn search_by_wit_columns(
        conn: &Connection,
        namespace: &str,
        name: &str,
    ) -> anyhow::Result<Option<RawKnownPackage>> {
        let result = conn.query_row(
            "SELECT id, registry, repository, updated_at, created_at,
                    wit_namespace, wit_name, kind
             FROM oci_repository
             WHERE wit_namespace = ?1 AND wit_name = ?2
             ORDER BY updated_at DESC
             LIMIT 1",
            [namespace, name],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        );

        match result {
            Ok((id, registry, repository, updated_at, created_at, wit_ns, wit_n, kind_str)) => {
                let tags = Self::fetch_tags(conn, id);
                let description = Self::fetch_description(conn, id);
                Ok(Some(RawKnownPackage {
                    id,
                    registry,
                    repository,
                    description,
                    tags,
                    signature_tags: Vec::new(),
                    attestation_tags: Vec::new(),
                    last_seen_at: updated_at,
                    created_at,
                    wit_namespace: wit_ns,
                    wit_name: wit_n,
                    kind: parse_kind(kind_str.as_deref()),
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Fuzzy fallback: convert WIT name to a search pattern on `repository`.
    fn search_by_repository_pattern(
        conn: &Connection,
        wit_name: &str,
    ) -> anyhow::Result<Option<RawKnownPackage>> {
        let search_pattern = wit_name.replace(':', "/");
        let like_pattern = format!("%{search_pattern}%");

        let result = conn.query_row(
            "SELECT id, registry, repository, updated_at, created_at,
                    wit_namespace, wit_name, kind
             FROM oci_repository
             WHERE repository LIKE ?1
             ORDER BY updated_at DESC
             LIMIT 1",
            [&like_pattern],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        );

        match result {
            Ok((id, registry, repository, updated_at, created_at, wit_ns, wit_n, kind_str)) => {
                let tags = Self::fetch_tags(conn, id);
                let description = Self::fetch_description(conn, id);
                Ok(Some(RawKnownPackage {
                    id,
                    registry,
                    repository,
                    description,
                    tags,
                    signature_tags: Vec::new(),
                    attestation_tags: Vec::new(),
                    last_seen_at: updated_at,
                    created_at,
                    wit_namespace: wit_ns,
                    wit_name: wit_n,
                    kind: parse_kind(kind_str.as_deref()),
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Parse a `kind` column value ("component" / "interface") into a
/// [`PackageKind`], returning `None` for `NULL` or unrecognised values.
fn parse_kind(s: Option<&str>) -> Option<PackageKind> {
    match s {
        Some("component") => Some(PackageKind::Component),
        Some("interface") => Some(PackageKind::Interface),
        _ => None,
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

    // r[verify db.known-packages.upsert-new]
    #[test]
    fn test_known_package_upsert_new_package() {
        let conn = setup_test_db();
        RawKnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let packages = RawKnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages.first().unwrap().registry, "ghcr.io");
        assert_eq!(packages.first().unwrap().repository, "user/repo");
    }

    // r[verify db.known-packages.upsert-existing]
    #[test]
    fn test_known_package_upsert_updates_existing() {
        let conn = setup_test_db();
        RawKnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();
        RawKnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let packages = RawKnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
    }

    // r[verify db.known-packages.search]
    #[test]
    fn test_known_package_search() {
        let conn = setup_test_db();
        RawKnownPackage::upsert(&conn, "ghcr.io", "bytecode/component", None, None).unwrap();
        RawKnownPackage::upsert(&conn, "docker.io", "library/nginx", None, None).unwrap();
        RawKnownPackage::upsert(&conn, "ghcr.io", "user/nginx-app", None, None).unwrap();

        let results = RawKnownPackage::search(&conn, "nginx", 0, 100).unwrap();
        assert_eq!(results.len(), 2);

        let results = RawKnownPackage::search(&conn, "ghcr", 0, 100).unwrap();
        assert_eq!(results.len(), 2);

        let results = RawKnownPackage::search(&conn, "bytecode", 0, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results.first().unwrap().repository, "bytecode/component");
    }

    // r[verify db.known-packages.search-empty]
    #[test]
    fn test_known_package_search_no_results() {
        let conn = setup_test_db();
        RawKnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let results = RawKnownPackage::search(&conn, "nonexistent", 0, 100).unwrap();
        assert!(results.is_empty());
    }

    // r[verify db.known-packages.get]
    #[test]
    fn test_known_package_get() {
        let conn = setup_test_db();
        RawKnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let package = RawKnownPackage::get(&conn, "ghcr.io", "user/repo").unwrap();
        assert!(package.is_some());
        let package = package.unwrap();
        assert_eq!(package.registry, "ghcr.io");
        assert_eq!(package.repository, "user/repo");

        let package = RawKnownPackage::get(&conn, "docker.io", "nonexistent").unwrap();
        assert!(package.is_none());
    }

    // r[verify db.known-packages.reference]
    #[test]
    fn test_known_package_reference() {
        let conn = setup_test_db();
        RawKnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();

        let packages = RawKnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.first().unwrap().reference(), "ghcr.io/user/repo");
    }

    // r[verify db.known-packages.reference-default-tag]
    #[test]
    fn test_known_package_reference_with_tag_default() {
        let conn = setup_test_db();
        RawKnownPackage::upsert(&conn, "ghcr.io", "user/repo", None, None).unwrap();
        let packages = RawKnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(
            packages.first().unwrap().reference_with_tag(),
            "ghcr.io/user/repo:latest"
        );
    }

    // r[verify db.known-packages.search-by-wit-name]
    #[test]
    fn test_known_package_search_by_wit_name() {
        let conn = setup_test_db();
        RawKnownPackage::upsert(&conn, "ghcr.io", "webassembly/wasi/http", None, None).unwrap();
        RawKnownPackage::upsert(&conn, "ghcr.io", "webassembly/wasi/clocks", None, None).unwrap();

        // "wasi:http" → search pattern "wasi/http" → should match "webassembly/wasi/http"
        let result = RawKnownPackage::search_by_wit_name(&conn, "wasi:http").unwrap();
        assert!(result.is_some());
        let pkg = result.unwrap();
        assert_eq!(pkg.repository, "webassembly/wasi/http");
    }

    // r[verify db.known-packages.search-by-wit-name-not-found]
    #[test]
    fn test_known_package_search_by_wit_name_not_found() {
        let conn = setup_test_db();
        RawKnownPackage::upsert(&conn, "ghcr.io", "webassembly/wasi/http", None, None).unwrap();

        let result = RawKnownPackage::search_by_wit_name(&conn, "wasi:nonexistent").unwrap();
        assert!(result.is_none());
    }

    /// Helper: create a repo + manifest + wit_package + wit_world chain in the test DB.
    /// Returns the world ID for attaching imports/exports.
    fn setup_wit_chain(
        conn: &Connection,
        registry: &str,
        repository: &str,
        wit_name: &str,
        world_name: &str,
    ) -> i64 {
        use crate::oci::{OciManifest, OciRepository as OciRepo};
        use crate::types::RawWitPackage;
        use crate::types::WitWorld;
        use std::collections::HashMap;

        let repo_id = OciRepo::upsert(conn, registry, repository).unwrap();
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
            &HashMap::new(),
        )
        .unwrap();
        let pkg_id = RawWitPackage::insert(
            conn,
            wit_name,
            Some("0.2.0"),
            None,
            None,
            Some(manifest_id),
            None,
        )
        .unwrap();
        WitWorld::insert(conn, pkg_id, world_name, None).unwrap()
    }

    #[test]
    fn test_search_by_import_returns_matching_packages() {
        use crate::types::WitWorldImport;

        let conn = setup_test_db();
        let world_id = setup_wit_chain(&conn, "ghcr.io", "example/my-app", "my:app", "main");

        WitWorldImport::insert(&conn, world_id, "wasi:http", Some("handler"), None, None).unwrap();
        WitWorldImport::insert(&conn, world_id, "wasi:io", Some("streams"), None, None).unwrap();

        let results = RawKnownPackage::search_by_import(&conn, "wasi:http", 0, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].repository, "example/my-app");

        let results = RawKnownPackage::search_by_import(&conn, "wasi:io", 0, 100).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_by_import_no_results() {
        let conn = setup_test_db();
        let results = RawKnownPackage::search_by_import(&conn, "wasi:nonexistent", 0, 100).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_by_export_returns_matching_packages() {
        use crate::types::WitWorldExport;

        let conn = setup_test_db();
        let world_id =
            setup_wit_chain(&conn, "ghcr.io", "example/http-server", "my:server", "main");

        WitWorldExport::insert(&conn, world_id, "wasi:http", Some("handler"), None, None).unwrap();

        let results = RawKnownPackage::search_by_export(&conn, "wasi:http", 0, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].repository, "example/http-server");
    }

    #[test]
    fn test_search_by_export_no_results() {
        let conn = setup_test_db();
        let results = RawKnownPackage::search_by_export(&conn, "wasi:nonexistent", 0, 100).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_by_import_deduplicates_repos() {
        use crate::oci::{OciManifest, OciRepository as OciRepo};
        use crate::types::RawWitPackage;
        use crate::types::{WitWorld, WitWorldImport};
        use std::collections::HashMap;

        let conn = setup_test_db();

        // One repo with two manifests that both import wasi:http
        let repo_id = OciRepo::upsert(&conn, "ghcr.io", "example/multi").unwrap();
        let (m1, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:aaa",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(1024),
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();
        let (m2, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:bbb",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{}"),
            Some(2048),
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        let p1 = RawWitPackage::insert(&conn, "my:app", Some("0.1.0"), None, None, Some(m1), None)
            .unwrap();
        let w1 = WitWorld::insert(&conn, p1, "main", None).unwrap();
        WitWorldImport::insert(&conn, w1, "wasi:http", None, None, None).unwrap();

        let p2 = RawWitPackage::insert(&conn, "my:app", Some("0.2.0"), None, None, Some(m2), None)
            .unwrap();
        let w2 = WitWorld::insert(&conn, p2, "main", None).unwrap();
        WitWorldImport::insert(&conn, w2, "wasi:http", None, None, None).unwrap();

        // Should return only 1 row (DISTINCT on oci_repository)
        let results = RawKnownPackage::search_by_import(&conn, "wasi:http", 0, 100).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].repository, "example/multi");
    }

    // r[verify db.known-packages.search-by-wit-name-exact]
    /// When wit_namespace and wit_name are stored, the exact lookup should
    /// succeed even when the repository path does not contain the namespace.
    #[test]
    fn test_known_package_search_by_wit_name_exact_lookup() {
        let conn = setup_test_db();
        // Insert a package whose repository path does NOT contain "ba/"
        RawKnownPackage::upsert_with_params(
            &conn,
            &KnownPackageParams {
                registry: "ghcr.io",
                repository: "bytecodealliance/sample-wasi-http-rust/sample-wasi-http-rust",
                tag: None,
                description: None,
                wit_namespace: Some("ba"),
                wit_name: Some("sample-wasi-http-rust"),
                kind: None,
            },
        )
        .unwrap();

        // The fuzzy fallback would fail ("ba/sample-wasi-http-rust" is not a
        // substring of "bytecodealliance/sample-wasi-http-rust/..."), but the
        // exact wit_namespace + wit_name lookup should succeed.
        let result =
            RawKnownPackage::search_by_wit_name(&conn, "ba:sample-wasi-http-rust").unwrap();
        assert!(result.is_some());
        let pkg = result.unwrap();
        assert_eq!(
            pkg.repository,
            "bytecodealliance/sample-wasi-http-rust/sample-wasi-http-rust"
        );
        assert_eq!(pkg.wit_namespace.as_deref(), Some("ba"));
        assert_eq!(pkg.wit_name.as_deref(), Some("sample-wasi-http-rust"));
    }

    // r[verify db.known-packages.kind-round-trip]
    /// Storing a `PackageKind` via `upsert_with_params` must round-trip
    /// correctly when reading back via `get_all`.
    #[test]
    fn test_known_package_kind_round_trip() {
        let conn = setup_test_db();

        RawKnownPackage::upsert_with_params(
            &conn,
            &KnownPackageParams {
                registry: "ghcr.io",
                repository: "example/my-component",
                tag: None,
                description: None,
                wit_namespace: None,
                wit_name: None,
                kind: Some(PackageKind::Component),
            },
        )
        .unwrap();

        RawKnownPackage::upsert_with_params(
            &conn,
            &KnownPackageParams {
                registry: "ghcr.io",
                repository: "example/my-interface",
                tag: None,
                description: None,
                wit_namespace: None,
                wit_name: None,
                kind: Some(PackageKind::Interface),
            },
        )
        .unwrap();

        let packages = RawKnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 2);

        let component = packages
            .iter()
            .find(|p| p.repository == "example/my-component")
            .unwrap();
        assert_eq!(component.kind, Some(PackageKind::Component));

        let interface = packages
            .iter()
            .find(|p| p.repository == "example/my-interface")
            .unwrap();
        assert_eq!(interface.kind, Some(PackageKind::Interface));
    }

    // r[verify db.known-packages.kind-none]
    /// Packages without a `kind` should round-trip as `None`.
    #[test]
    fn test_known_package_kind_none_round_trip() {
        let conn = setup_test_db();
        RawKnownPackage::upsert(&conn, "ghcr.io", "example/unknown", None, None).unwrap();

        let packages = RawKnownPackage::get_all(&conn, 0, 100).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].kind, None);
    }
}
