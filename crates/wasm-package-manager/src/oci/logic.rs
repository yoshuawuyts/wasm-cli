//! OCI-specific pure logic extracted from the `Manager` and `Store`
//! implementations.
//!
//! These functions contain no IO and can be unit-tested in isolation.

use oci_client::manifest::OciDescriptor;
use std::collections::HashSet;
use std::ffi::OsStr;

/// Filter manifest layers to only those with `application/wasm` media type.
#[must_use]
pub fn filter_wasm_layers(layers: &[OciDescriptor]) -> Vec<&OciDescriptor> {
    layers
        .iter()
        .filter(|l| l.media_type == "application/wasm")
        .collect()
}

/// Compute which layer digests are orphaned after removing a set of manifests.
///
/// Given the digests belonging to the manifests being deleted and the digests
/// belonging to all other (retained) manifests, returns those that appear only
/// in the deleted set and can safely be purged from the content store.
#[must_use]
pub fn compute_orphaned_layers<S: std::hash::BuildHasher>(
    deleted_digests: &HashSet<String, S>,
    retained_digests: &HashSet<String, S>,
) -> Vec<String> {
    deleted_digests
        .difference(retained_digests)
        .cloned()
        .collect()
}

/// Classify a single tag as release, signature, or attestation.
///
/// OCI cosign conventions use `sha256-<hex>` prefixed tags:
///   - `.sig` suffix → signature tag
///   - `.att` suffix → attestation tag
///   - everything else → release tag
#[must_use]
pub fn classify_tag(tag: &str) -> TagKind {
    if tag.starts_with("sha256-") {
        let ext = std::path::Path::new(tag).extension();
        if ext == Some(OsStr::new("sig")) {
            TagKind::Signature
        } else if ext == Some(OsStr::new("att")) {
            TagKind::Attestation
        } else {
            TagKind::Release
        }
    } else {
        TagKind::Release
    }
}

/// The kind of an OCI tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// A normal release tag (e.g., `v1.0`, `latest`).
    Release,
    /// A cosign signature tag (e.g., `sha256-abc123.sig`).
    Signature,
    /// A cosign attestation tag (e.g., `sha256-abc123.att`).
    Attestation,
}

/// Classify a list of tags into `(release, signature, attestation)` buckets.
///
/// This is a convenience wrapper around [`classify_tag`] that partitions
/// a slice of tags into three vectors.
#[must_use]
pub fn classify_tags(tags: &[String]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut release = Vec::new();
    let mut signature = Vec::new();
    let mut attestation = Vec::new();

    for tag in tags {
        match classify_tag(tag) {
            TagKind::Release => release.push(tag.clone()),
            TagKind::Signature => signature.push(tag.clone()),
            TagKind::Attestation => attestation.push(tag.clone()),
        }
    }

    (release, signature, attestation)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── filter_wasm_layers ──────────────────────────────────────────────

    // r[verify oci.layers.filter-mixed]
    #[test]
    fn filter_wasm_layers_mixed() {
        let layers = vec![
            OciDescriptor {
                media_type: "application/wasm".to_string(),
                digest: "sha256:aaa".to_string(),
                size: 100,
                urls: None,
                annotations: None,
            },
            OciDescriptor {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: "sha256:bbb".to_string(),
                size: 50,
                urls: None,
                annotations: None,
            },
            OciDescriptor {
                media_type: "application/wasm".to_string(),
                digest: "sha256:ccc".to_string(),
                size: 200,
                urls: None,
                annotations: None,
            },
        ];
        let wasm = filter_wasm_layers(&layers);
        assert_eq!(wasm.len(), 2);
        assert_eq!(wasm[0].digest, "sha256:aaa");
        assert_eq!(wasm[1].digest, "sha256:ccc");
    }

    // r[verify oci.layers.filter-none]
    #[test]
    fn filter_wasm_layers_none() {
        let layers = vec![OciDescriptor {
            media_type: "application/json".to_string(),
            digest: "sha256:xxx".to_string(),
            size: 10,
            urls: None,
            annotations: None,
        }];
        assert!(filter_wasm_layers(&layers).is_empty());
    }

    // r[verify oci.layers.filter-empty]
    #[test]
    fn filter_wasm_layers_empty() {
        assert!(filter_wasm_layers(&[]).is_empty());
    }

    // ── compute_orphaned_layers ─────────────────────────────────────────

    // r[verify oci.layers.orphaned-disjoint]
    #[test]
    fn orphaned_layers_disjoint() {
        let deleted: HashSet<String> = ["sha256:aaa", "sha256:bbb"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let retained: HashSet<String> = ["sha256:ccc"].iter().map(|s| s.to_string()).collect();
        let mut orphaned = compute_orphaned_layers(&deleted, &retained);
        orphaned.sort();
        assert_eq!(orphaned, vec!["sha256:aaa", "sha256:bbb"]);
    }

    // r[verify oci.layers.orphaned-overlap]
    #[test]
    fn orphaned_layers_overlap() {
        let deleted: HashSet<String> = ["sha256:aaa", "sha256:shared"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let retained: HashSet<String> = ["sha256:shared", "sha256:ccc"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let orphaned = compute_orphaned_layers(&deleted, &retained);
        assert_eq!(orphaned, vec!["sha256:aaa"]);
    }

    // r[verify oci.layers.orphaned-shared]
    #[test]
    fn orphaned_layers_all_shared() {
        let deleted: HashSet<String> = ["sha256:aaa"].iter().map(|s| s.to_string()).collect();
        let retained: HashSet<String> = ["sha256:aaa"].iter().map(|s| s.to_string()).collect();
        let orphaned = compute_orphaned_layers(&deleted, &retained);
        assert!(orphaned.is_empty());
    }

    #[test]
    fn orphaned_layers_empty_retained() {
        let deleted: HashSet<String> = ["sha256:aaa", "sha256:bbb"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let retained: HashSet<String> = HashSet::new();
        let mut orphaned = compute_orphaned_layers(&deleted, &retained);
        orphaned.sort();
        assert_eq!(orphaned, vec!["sha256:aaa", "sha256:bbb"]);
    }

    #[test]
    fn orphaned_layers_empty_deleted() {
        let deleted: HashSet<String> = HashSet::new();
        let retained: HashSet<String> = ["sha256:aaa"].iter().map(|s| s.to_string()).collect();
        let orphaned = compute_orphaned_layers(&deleted, &retained);
        assert!(orphaned.is_empty());
    }

    // ── classify_tag / classify_tags ────────────────────────────────────

    // r[verify oci.tags.classify-release]
    #[test]
    fn classify_tag_release() {
        assert_eq!(classify_tag("v1.0"), TagKind::Release);
        assert_eq!(classify_tag("latest"), TagKind::Release);
    }

    // r[verify oci.tags.classify-signature]
    #[test]
    fn classify_tag_signature() {
        assert_eq!(classify_tag("sha256-abc123def456.sig"), TagKind::Signature);
    }

    // r[verify oci.tags.classify-attestation]
    #[test]
    fn classify_tag_attestation() {
        assert_eq!(
            classify_tag("sha256-abc123def456.att"),
            TagKind::Attestation
        );
    }

    #[test]
    fn classify_tag_sha256_without_suffix() {
        // sha256- prefix but no .sig or .att → release
        assert_eq!(classify_tag("sha256-abc123def456"), TagKind::Release);
    }

    // r[verify oci.tags.classify-mixed]
    #[test]
    fn classify_tags_mixed() {
        let tags: Vec<String> = vec![
            "v1.0".into(),
            "latest".into(),
            "sha256-abc123.sig".into(),
            "sha256-abc123.att".into(),
            "sha256-def456".into(),
        ];
        let (release, signature, attestation) = classify_tags(&tags);
        assert_eq!(release, vec!["v1.0", "latest", "sha256-def456"]);
        assert_eq!(signature, vec!["sha256-abc123.sig"]);
        assert_eq!(attestation, vec!["sha256-abc123.att"]);
    }

    // r[verify oci.tags.classify-empty]
    #[test]
    fn classify_tags_empty() {
        let (release, signature, attestation) = classify_tags(&[]);
        assert!(release.is_empty());
        assert!(signature.is_empty());
        assert!(attestation.is_empty());
    }

    // r[verify oci.tags.classify-all-release]
    #[test]
    fn classify_tags_all_release() {
        let tags: Vec<String> = vec!["v1.0".into(), "latest".into(), "stable".into()];
        let (release, signature, attestation) = classify_tags(&tags);
        assert_eq!(release.len(), 3);
        assert!(signature.is_empty());
        assert!(attestation.is_empty());
    }

    // ── cacache round-trip ──────────────────────────────────────────────

    // r[verify oci.layers.cacache-roundtrip]
    #[tokio::test]
    async fn cacache_roundtrip_via_layer_digest() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path();

        // Simulate what Store::insert does: write layer data keyed by digest.
        let digest = "sha256:aaa111bbb222";
        let data = b"fake wasm component bytes";
        cacache::write(cache, digest, data).await.unwrap();

        // Build a manifest layer list like a real OCI manifest would have.
        let layers = vec![
            OciDescriptor {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: "sha256:cfgcfg".to_string(),
                size: 50,
                urls: None,
                annotations: None,
            },
            OciDescriptor {
                media_type: "application/wasm".to_string(),
                digest: digest.to_string(),
                size: data.len() as i64,
                urls: None,
                annotations: None,
            },
        ];

        // Use filter_wasm_layers to find the correct key — exactly as `run` does.
        let wasm = filter_wasm_layers(&layers);
        assert_eq!(wasm.len(), 1);
        let key = &wasm[0].digest;

        // Read back using the layer digest — this is the pattern the run command uses.
        let read_back = cacache::read(cache, key).await.unwrap();
        assert_eq!(read_back, data);

        // Verify that using an OCI reference string as key does NOT find the data.
        let bad_key = "ghcr.io/example/my-component:1.0.0";
        assert!(cacache::read(cache, bad_key).await.is_err());
    }
}
