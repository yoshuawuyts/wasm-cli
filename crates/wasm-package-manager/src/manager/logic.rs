//! Pure logic extracted from the `Manager` and `Store` implementations.
//!
//! These functions contain no IO and can be unit-tested in isolation.

use std::collections::HashSet;

/// Truncated digest length used in vendor filenames.
const DIGEST_PREFIX_LEN: usize = 12;

/// Compute the vendor filename for a cached layer.
///
/// The filename encodes the registry, repository, optional tag, and a
/// truncated image digest so that vendored files are human-identifiable.
///
/// # Example
///
/// ```
/// use wasm_package_manager::manager::vendor_filename;
///
/// let name = vendor_filename("ghcr.io", "user/repo", Some("v1.0"), "sha256:abcdef1234567890");
/// assert_eq!(name, "ghcr-io-user-repo-v1.0-abcdef123456.wasm");
/// ```
// r[impl manager.vendor-filename.basic]
// r[impl manager.vendor-filename.nested]
// r[impl manager.vendor-filename.no-tag]
// r[impl manager.vendor-filename.short-digest]
#[must_use]
pub fn vendor_filename(
    registry: &str,
    repository: &str,
    tag: Option<&str>,
    digest: &str,
) -> String {
    let registry_part = registry.replace('.', "-");
    let repo_part = repository.replace('/', "-");
    let tag_part = tag.map(|t| format!("-{t}")).unwrap_or_default();
    let sha_part = digest.strip_prefix("sha256:").unwrap_or(digest);
    let short_sha = sha_part.get(..DIGEST_PREFIX_LEN).unwrap_or(sha_part);
    format!("{registry_part}-{repo_part}{tag_part}-{short_sha}.wasm")
}

/// Determine whether a sync from the meta-registry should proceed.
///
/// Returns `true` when enough time has elapsed since `last_synced_epoch`,
/// or when the last-sync timestamp is unknown.
///
/// # Example
///
/// ```
/// use wasm_package_manager::manager::should_sync;
///
/// // No previous sync — always sync.
/// assert!(should_sync(None, 3600, 1000));
///
/// // Last synced long ago — sync again.
/// assert!(should_sync(Some(1000), 3600, 5000));
///
/// // Recently synced — skip.
/// assert!(!should_sync(Some(1000), 3600, 2000));
/// ```
// r[impl manager.sync.fresh]
// r[impl manager.sync.stale]
// r[impl manager.sync.no-previous]
#[must_use]
pub fn should_sync(last_synced_epoch: Option<i64>, sync_interval: u64, now_epoch: i64) -> bool {
    match last_synced_epoch {
        Some(last) => now_epoch - last >= i64::try_from(sync_interval).unwrap_or(i64::MAX),
        None => true,
    }
}

/// Sanitize a string into a valid WIT identifier.
///
/// WIT identifiers must match `[a-z][a-z0-9]*(-[a-z][a-z0-9]*)*`.
/// Returns `None` if the input cannot be sanitized into a valid identifier
/// (e.g. it contains only digits or special characters).
///
/// # Example
///
/// ```
/// use wasm_package_manager::manager::sanitize_to_wit_identifier;
///
/// assert_eq!(sanitize_to_wit_identifier("My_Component"), Some("my-component".to_string()));
/// assert_eq!(sanitize_to_wit_identifier("123fetch"), Some("fetch".to_string()));
/// assert_eq!(sanitize_to_wit_identifier("!!!"), None);
/// ```
// r[impl manager.name.sanitize.valid]
// r[impl manager.name.sanitize.uppercase]
// r[impl manager.name.sanitize.underscores]
// r[impl manager.name.sanitize.leading-digits]
#[must_use]
pub fn sanitize_to_wit_identifier(input: &str) -> Option<String> {
    // Lowercase and replace non-alphanumeric characters with hyphens.
    let sanitized: String = input
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse consecutive hyphens, strip leading/trailing hyphens.
    let collapsed: String = sanitized
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    // Strip leading digits (WIT identifiers must start with [a-z]).
    let trimmed = collapsed.trim_start_matches(|c: char| c.is_ascii_digit());

    // Strip a possible leading hyphen left after digit removal.
    let trimmed = trimmed.strip_prefix('-').unwrap_or(trimmed);

    if trimmed.is_empty() {
        return None;
    }

    Some(trimmed.to_string())
}

/// Derive a human-friendly component name for use in `wasm.toml`.
///
/// Follows this priority chain:
/// 1. **WIT package name** — strip the `@version` suffix.
/// 2. **OCI `image.title`** — sanitized to a WIT-legal identifier.
/// 3. **Last segment of the repository path** — sanitized; used when no
///    collision with `existing_names`.
/// 4. **Full repository path** — with `/` replaced by `-`; used on collision.
///
/// # Example
///
/// ```
/// use std::collections::HashSet;
/// use wasm_package_manager::manager::derive_component_name;
///
/// let existing = HashSet::new();
/// let name = derive_component_name(
///     Some("wasi:http@0.2.10"),
///     None,
///     "webassembly/wasi-http",
///     &existing,
/// );
/// assert_eq!(name, "wasi:http");
/// ```
// r[impl manager.name.last-segment]
// r[impl manager.name.wit-package]
// r[impl manager.name.oci-title]
// r[impl manager.name.collision]
#[must_use]
pub fn derive_component_name<S: std::hash::BuildHasher>(
    package_name: Option<&str>,
    oci_title: Option<&str>,
    repository: &str,
    existing_names: &HashSet<String, S>,
) -> String {
    // 1. WIT package name (strip @version).
    if let Some(name) = package_name {
        return name.split('@').next().unwrap_or(name).to_string();
    }

    // 2. OCI image.title annotation.
    if let Some(title) = oci_title
        && let Some(sanitized) = sanitize_to_wit_identifier(title)
    {
        return sanitized;
    }

    // 3. Last segment of the repository path.
    let last_segment = repository.rsplit('/').next().unwrap_or(repository);
    if let Some(sanitized) = sanitize_to_wit_identifier(last_segment)
        && !existing_names.contains(&sanitized)
    {
        return sanitized;
    }

    // 4. Full repository path (on collision or if last segment fails).
    sanitize_to_wit_identifier(&repository.replace('/', "-"))
        .unwrap_or_else(|| repository.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── vendor_filename ─────────────────────────────────────────────────

    // r[verify manager.vendor-filename.basic]
    #[test]
    fn vendor_filename_basic() {
        let name = vendor_filename(
            "ghcr.io",
            "user/repo",
            Some("v1.0"),
            "sha256:abcdef1234567890",
        );
        assert_eq!(name, "ghcr-io-user-repo-v1.0-abcdef123456.wasm");
    }

    // r[verify manager.vendor-filename.no-tag]
    #[test]
    fn vendor_filename_no_tag() {
        let name = vendor_filename("ghcr.io", "user/repo", None, "sha256:abcdef1234567890");
        assert_eq!(name, "ghcr-io-user-repo-abcdef123456.wasm");
    }

    // r[verify manager.vendor-filename.short-digest]
    #[test]
    fn vendor_filename_short_digest() {
        let name = vendor_filename("ghcr.io", "user/repo", Some("latest"), "sha256:abc");
        assert_eq!(name, "ghcr-io-user-repo-latest-abc.wasm");
    }

    #[test]
    fn vendor_filename_no_sha256_prefix() {
        let name = vendor_filename("docker.io", "lib/hello", Some("1.0"), "rawdigest123456");
        assert_eq!(name, "docker-io-lib-hello-1.0-rawdigest123.wasm");
    }

    // r[verify manager.vendor-filename.nested]
    #[test]
    fn vendor_filename_nested_repository() {
        let name = vendor_filename(
            "ghcr.io",
            "org/team/component",
            Some("v2"),
            "sha256:0123456789abcdef",
        );
        assert_eq!(name, "ghcr-io-org-team-component-v2-0123456789ab.wasm");
    }

    #[test]
    fn vendor_filename_unknown_digest() {
        let name = vendor_filename("ghcr.io", "user/repo", None, "unknown");
        assert_eq!(name, "ghcr-io-user-repo-unknown.wasm");
    }

    // ── should_sync ─────────────────────────────────────────────────────

    // r[verify manager.sync.no-previous]
    #[test]
    fn should_sync_no_previous() {
        assert!(should_sync(None, 3600, 1000));
    }

    // r[verify manager.sync.stale]
    #[test]
    fn should_sync_stale() {
        assert!(should_sync(Some(1000), 3600, 5000));
    }

    // r[verify manager.sync.fresh]
    #[test]
    fn should_sync_fresh() {
        assert!(!should_sync(Some(1000), 3600, 2000));
    }

    #[test]
    fn should_sync_exact_boundary() {
        // Exactly at the interval boundary should trigger sync.
        assert!(should_sync(Some(0), 3600, 3600));
    }

    // ── sanitize_to_wit_identifier ──────────────────────────────────────

    // r[verify manager.name.sanitize.valid]
    #[test]
    fn sanitize_already_valid() {
        assert_eq!(
            sanitize_to_wit_identifier("fetch"),
            Some("fetch".to_string())
        );
    }

    // r[verify manager.name.sanitize.uppercase]
    #[test]
    fn sanitize_uppercase() {
        assert_eq!(
            sanitize_to_wit_identifier("Fetch"),
            Some("fetch".to_string())
        );
    }

    // r[verify manager.name.sanitize.underscores]
    #[test]
    fn sanitize_underscores() {
        assert_eq!(
            sanitize_to_wit_identifier("my_component"),
            Some("my-component".to_string())
        );
    }

    // r[verify manager.name.sanitize.leading-digits]
    #[test]
    fn sanitize_leading_digits() {
        assert_eq!(
            sanitize_to_wit_identifier("123fetch"),
            Some("fetch".to_string())
        );
    }

    #[test]
    fn sanitize_all_digits() {
        assert_eq!(sanitize_to_wit_identifier("12345"), None);
    }

    #[test]
    fn sanitize_empty_after_sanitization() {
        assert_eq!(sanitize_to_wit_identifier("!!!"), None);
    }

    #[test]
    fn sanitize_complex() {
        assert_eq!(
            sanitize_to_wit_identifier("My Cool_Fetch.Component"),
            Some("my-cool-fetch-component".to_string())
        );
    }

    // ── derive_component_name ───────────────────────────────────────────

    // r[verify manager.name.wit-package]
    #[test]
    fn derive_name_wit_package_name() {
        let existing = HashSet::new();
        let name = derive_component_name(
            Some("wasi:http@0.2.10"),
            None,
            "webassembly/wasi-http",
            &existing,
        );
        assert_eq!(name, "wasi:http");
    }

    // r[verify manager.name.oci-title]
    #[test]
    fn derive_name_oci_title() {
        let existing = HashSet::new();
        let name = derive_component_name(
            None,
            Some("My Fetch Component"),
            "yoshuawuyts/fetch",
            &existing,
        );
        assert_eq!(name, "my-fetch-component");
    }

    // r[verify manager.name.last-segment]
    #[test]
    fn derive_name_last_segment() {
        let existing = HashSet::new();
        let name = derive_component_name(None, None, "yoshuawuyts/fetch", &existing);
        assert_eq!(name, "fetch");
    }

    // r[verify manager.name.collision]
    #[test]
    fn derive_name_collision() {
        let mut existing = HashSet::new();
        existing.insert("fetch".to_string());
        let name = derive_component_name(None, None, "yoshuawuyts/fetch", &existing);
        assert_eq!(name, "yoshuawuyts-fetch");
    }

    #[test]
    fn derive_name_repo_with_underscores_dots() {
        let existing = HashSet::new();
        let name = derive_component_name(None, None, "my_org/my.component", &existing);
        assert_eq!(name, "my-component");
    }

    #[test]
    fn derive_name_repo_with_underscores_dots_collision() {
        let mut existing = HashSet::new();
        existing.insert("my-component".to_string());
        let name = derive_component_name(None, None, "my_org/my.component", &existing);
        assert_eq!(name, "my-org-my-component");
    }

    #[test]
    fn derive_name_oci_title_invalid_chars() {
        let existing = HashSet::new();
        let name = derive_component_name(None, Some("!!!"), "yoshuawuyts/fetch", &existing);
        // Title sanitizes to empty → falls through to last segment
        assert_eq!(name, "fetch");
    }

    #[test]
    fn derive_name_oci_title_sanitizes_to_empty() {
        let existing = HashSet::new();
        let name = derive_component_name(None, Some("12345"), "yoshuawuyts/fetch", &existing);
        // Title is all digits → sanitizes to None → falls through
        assert_eq!(name, "fetch");
    }
}
