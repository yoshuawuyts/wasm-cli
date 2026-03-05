mod layer;
mod layer_annotation;
mod manifest;
mod referrer;
mod repository;
mod tag;

pub use layer::OciLayer;
pub use layer_annotation::OciLayerAnnotation;
pub use manifest::OciManifest;
pub use referrer::OciReferrer;
pub use repository::OciRepository;
pub use tag::OciTag;

/// Result of an insert operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertResult {
    /// The entry was inserted successfully.
    Inserted,
    /// The entry already existed in the database.
    AlreadyExists,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::storage::Migrations;
    use rusqlite::Connection;

    /// Create an in-memory database with migrations applied for testing.
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();
        conn
    }

    // r[verify oci.repository.upsert-and-find]
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

    // r[verify oci.repository.upsert-idempotent]
    #[test]
    fn test_oci_repository_upsert_idempotent() {
        let conn = setup_test_db();
        let id1 = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();
        let id2 = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();
        assert_eq!(id1, id2);
    }

    // r[verify oci.manifest.upsert]
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

    // r[verify oci.manifest.annotations]
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

    // r[verify oci.tag.upsert]
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

    // r[verify oci.layer.insert]
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

    // r[verify oci.manifest.cascade-delete]
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

    // r[verify oci.manifest.config-fields]
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

    // r[verify oci.layer.annotations]
    #[test]
    fn test_oci_layer_annotation_insert_and_list() {
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

        let layer_id = OciLayer::insert(
            &conn,
            mid,
            "sha256:layer1",
            Some("application/wasm"),
            Some(512),
            0,
        )
        .unwrap();

        // Insert annotations
        let ann_id1 =
            OciLayerAnnotation::insert(&conn, layer_id, "org.example.key1", "value1").unwrap();
        assert!(ann_id1 > 0);

        let ann_id2 =
            OciLayerAnnotation::insert(&conn, layer_id, "org.example.key2", "value2").unwrap();
        assert!(ann_id2 > 0);
        assert_ne!(ann_id1, ann_id2);

        // List and verify
        let annotations = OciLayerAnnotation::list_by_layer(&conn, layer_id).unwrap();
        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].key, "org.example.key1");
        assert_eq!(annotations[0].value, "value1");
        assert_eq!(annotations[1].key, "org.example.key2");
        assert_eq!(annotations[1].value, "value2");
    }

    // r[verify oci.layer.annotation-conflict]
    #[test]
    fn test_oci_layer_annotation_upsert_on_conflict() {
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

        let layer_id = OciLayer::insert(&conn, mid, "sha256:layer1", None, None, 0).unwrap();

        // Insert and then upsert with new value
        let id1 =
            OciLayerAnnotation::insert(&conn, layer_id, "org.example.key", "original").unwrap();
        let id2 =
            OciLayerAnnotation::insert(&conn, layer_id, "org.example.key", "updated").unwrap();
        assert_eq!(id1, id2);

        let annotations = OciLayerAnnotation::list_by_layer(&conn, layer_id).unwrap();
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].value, "updated");
    }

    // r[verify oci.layer.annotation-cascade]
    #[test]
    fn test_oci_layer_annotation_cascade_delete() {
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

        let layer_id = OciLayer::insert(&conn, mid, "sha256:layer1", None, None, 0).unwrap();
        OciLayerAnnotation::insert(&conn, layer_id, "key1", "val1").unwrap();
        OciLayerAnnotation::insert(&conn, layer_id, "key2", "val2").unwrap();

        // Delete the manifest — layers and their annotations should cascade
        OciManifest::delete(&conn, mid).unwrap();

        let annotations = OciLayerAnnotation::list_by_layer(&conn, layer_id).unwrap();
        assert!(
            annotations.is_empty(),
            "layer annotations should be deleted when manifest is deleted"
        );
    }

    // r[verify oci.referrer.insert]
    #[test]
    fn test_oci_referrer_insert_and_list() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        // Create subject manifest
        let (subject_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:subject",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        // Create referrer manifest
        let (referrer_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:referrer",
            None,
            None,
            None,
            Some("application/vnd.dev.cosign.simplesigning.v1+json"),
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        // Insert referrer relationship
        let ref_id = OciReferrer::insert(
            &conn,
            subject_id,
            referrer_id,
            "application/vnd.dev.cosign.simplesigning.v1+json",
        )
        .unwrap();
        assert!(ref_id > 0);

        // List referrers for subject
        let referrers = OciReferrer::list_by_subject(&conn, subject_id).unwrap();
        assert_eq!(referrers.len(), 1);
        assert_eq!(referrers[0].subject_manifest_id, subject_id);
        assert_eq!(referrers[0].referrer_manifest_id, referrer_id);
        assert_eq!(
            referrers[0].artifact_type,
            "application/vnd.dev.cosign.simplesigning.v1+json"
        );
    }

    // r[verify oci.referrer.idempotent]
    #[test]
    fn test_oci_referrer_idempotent() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        let (subject_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:subject",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        let (referrer_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:referrer",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        let id1 = OciReferrer::insert(&conn, subject_id, referrer_id, "type/a").unwrap();
        let id2 = OciReferrer::insert(&conn, subject_id, referrer_id, "type/a").unwrap();
        assert_eq!(id1, id2, "duplicate referrer insert should return same ID");
    }

    // r[verify oci.referrer.cascade-delete]
    #[test]
    fn test_oci_referrer_cascade_delete() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        let (subject_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:subject",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        let (referrer_id, _) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:referrer",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        OciReferrer::insert(&conn, subject_id, referrer_id, "type/a").unwrap();

        // Delete subject manifest — referrer relationship should cascade
        OciManifest::delete(&conn, subject_id).unwrap();

        let referrers = OciReferrer::list_by_subject(&conn, subject_id).unwrap();
        assert!(
            referrers.is_empty(),
            "referrer rows should be deleted when subject manifest is deleted"
        );
    }

    // r[verify oci.repository.get-by-id]
    #[test]
    fn test_oci_repository_get_by_id() {
        let conn = setup_test_db();
        let id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        let repo = OciRepository::get_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(repo.id(), id);
        assert_eq!(repo.registry, "ghcr.io");
        assert_eq!(repo.repository, "user/repo");

        let none = OciRepository::get_by_id(&conn, 9999).unwrap();
        assert!(none.is_none());
    }

    // r[verify oci.repository.list-all]
    #[test]
    fn test_oci_repository_list_all() {
        let conn = setup_test_db();
        let empty = OciRepository::list_all(&conn).unwrap();
        assert!(empty.is_empty());

        OciRepository::upsert(&conn, "ghcr.io", "b/repo").unwrap();
        OciRepository::upsert(&conn, "ghcr.io", "a/repo").unwrap();

        let repos = OciRepository::list_all(&conn).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].repository, "a/repo");
        assert_eq!(repos[1].repository, "b/repo");
    }

    // r[verify oci.repository.find-not-found]
    #[test]
    fn test_oci_repository_find_not_found() {
        let conn = setup_test_db();
        let result = OciRepository::find(&conn, "ghcr.io", "nonexistent").unwrap();
        assert!(result.is_none());
    }

    // r[verify oci.tag.list-by-repository]
    #[test]
    fn test_oci_tag_list_by_repository() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:aaa",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();
        OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:bbb",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        OciTag::upsert(&conn, repo_id, "v2.0", "sha256:bbb").unwrap();
        OciTag::upsert(&conn, repo_id, "v1.0", "sha256:aaa").unwrap();

        let tags = OciTag::list_by_repository(&conn, repo_id).unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].tag, "v1.0");
        assert_eq!(tags[1].tag, "v2.0");
    }

    // r[verify oci.tag.find-not-found]
    #[test]
    fn test_oci_tag_find_not_found() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();
        let result = OciTag::find_by_tag(&conn, repo_id, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    // r[verify oci.layer.get-by-digest]
    #[test]
    fn test_oci_layer_get_by_digest() {
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

        let found = OciLayer::get_by_digest(&conn, mid, "sha256:layer1")
            .unwrap()
            .unwrap();
        assert_eq!(found.digest, "sha256:layer1");
        assert_eq!(found.media_type.as_deref(), Some("application/wasm"));
        assert_eq!(found.size_bytes, Some(512));
        assert_eq!(found.position, 0);

        let not_found = OciLayer::get_by_digest(&conn, mid, "sha256:nonexistent").unwrap();
        assert!(not_found.is_none());
    }

    // r[verify oci.manifest.list-by-repository]
    #[test]
    fn test_oci_manifest_list_by_repository() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:first",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();
        OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:second",
            None,
            None,
            None,
            None,
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();

        let manifests = OciManifest::list_by_repository(&conn, repo_id).unwrap();
        assert_eq!(manifests.len(), 2);
    }

    // r[verify oci.manifest.find-not-found]
    #[test]
    fn test_oci_manifest_find_not_found() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();
        let result = OciManifest::find(&conn, repo_id, "sha256:nonexistent").unwrap();
        assert!(result.is_none());
    }

    // r[verify oci.manifest.placeholder-upgrade]
    #[test]
    fn test_oci_manifest_upsert_upgrades_placeholder() {
        let conn = setup_test_db();
        let repo_id = OciRepository::upsert(&conn, "ghcr.io", "user/repo").unwrap();

        // First insert: placeholder with minimal data (as store_referrer does)
        let (mid1, was_inserted1) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:placeholder",
            None,
            None,
            None,
            Some("application/vnd.dev.cosign.simplesigning.v1+json"),
            None,
            None,
            &HashMap::new(),
        )
        .unwrap();
        assert!(was_inserted1);

        // Verify placeholder has NULL raw_json
        let placeholder = OciManifest::find(&conn, repo_id, "sha256:placeholder")
            .unwrap()
            .unwrap();
        assert!(placeholder.raw_json.is_none());

        // Second insert: full data (as a normal pull would provide)
        let (mid2, was_inserted2) = OciManifest::upsert(
            &conn,
            repo_id,
            "sha256:placeholder",
            Some("application/vnd.oci.image.manifest.v1+json"),
            Some("{\"layers\":[]}"),
            Some(4096),
            None,
            Some("application/vnd.oci.image.config.v1+json"),
            Some("sha256:configabc"),
            &HashMap::new(),
        )
        .unwrap();
        assert!(!was_inserted2, "should report as not newly inserted");
        assert_eq!(mid1, mid2, "should return the same manifest ID");

        // Verify fields were filled in
        let upgraded = OciManifest::find(&conn, repo_id, "sha256:placeholder")
            .unwrap()
            .unwrap();
        assert_eq!(upgraded.raw_json.as_deref(), Some("{\"layers\":[]}"));
        assert_eq!(upgraded.size_bytes, Some(4096));
        assert_eq!(
            upgraded.config_media_type.as_deref(),
            Some("application/vnd.oci.image.config.v1+json")
        );
        assert_eq!(upgraded.config_digest.as_deref(), Some("sha256:configabc"));
        // artifact_type should still be set from the placeholder
        assert_eq!(
            upgraded.artifact_type.as_deref(),
            Some("application/vnd.dev.cosign.simplesigning.v1+json")
        );
    }
}
