//! Dependency resolver using the PubGrub version-solving algorithm.
//!
//! This module exposes [`resolve_from_db`], which computes the complete
//! transitive dependency closure for a given root `(package, version)` pair
//! using metadata stored in the local SQLite database.
//!
//! The resolver is backed by [`DbDependencyProvider`], a concrete
//! implementation of pubgrub's `DependencyProvider` trait that queries the
//! `wit_package` and `wit_package_dependency` tables for available versions
//! and dependency edges.
//!
//! In tests the [`DepGraph`] helper (available inside this module's test
//! block) provides a convenient way to declare an in-memory package universe
//! and assert on the resolved set without going through OCI or the network.

use std::collections::HashMap;
use std::fmt;

use pubgrub::{
    Dependencies, DependencyConstraints, DependencyProvider, PackageResolutionStatistics, Ranges,
    Reporter, SelectedDependencies,
};

use crate::storage::Store;

// ─── Shared error mapping ────────────────────────────────────────────────────

/// Map a [`pubgrub::PubGrubError`] into a [`ResolveError`].
///
/// Shared by both `resolve_from_db` (single-root) and `resolve_all_from_db`
/// (multi-root) so the mapping stays in sync across both code paths.
fn map_pubgrub_error<DP>(e: pubgrub::PubGrubError<DP>) -> ResolveError
where
    DP: DependencyProvider<
            P = String,
            V = WitVersion,
            VS = WitVersionRange,
            M = String,
            Err = ResolveError,
        >,
{
    match e {
        pubgrub::PubGrubError::NoSolution(mut tree) => {
            tree.collapse_no_versions();
            ResolveError::NoSolution(pubgrub::DefaultStringReporter::report(&tree))
        }
        pubgrub::PubGrubError::ErrorRetrievingDependencies {
            package,
            version,
            source,
        } => ResolveError::Db(format!(
            "failed to get deps for {package}@{version}: {source}"
        )),
        pubgrub::PubGrubError::ErrorChoosingVersion { package, source } => {
            ResolveError::Db(format!("failed to choose version for {package}: {source}"))
        }
        pubgrub::PubGrubError::ErrorInShouldCancel(e) => {
            ResolveError::Db(format!("resolution cancelled: {e}"))
        }
    }
}

// ─── Version type ────────────────────────────────────────────────────────────

/// A `major.minor.patch` semantic version, used as the version type throughout
/// the resolver.
///
/// Re-exports [`pubgrub::SemanticVersion`] under a stable public name so that
/// callers do not need to depend on `pubgrub` directly.
pub type WitVersion = pubgrub::SemanticVersion;

// ─── Version set type ────────────────────────────────────────────────────────

/// A set of [`WitVersion`] values, used to express version constraints.
pub type WitVersionRange = Ranges<WitVersion>;

// ─── Error type ──────────────────────────────────────────────────────────────

/// Errors that can occur during dependency resolution.
#[derive(Debug)]
pub enum ResolveError {
    /// No combination of package versions satisfies all constraints.
    NoSolution(String),
    /// A database query failed while looking up dependency information.
    Db(String),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSolution(msg) => write!(f, "no solution: {msg}"),
            Self::Db(msg) => write!(f, "database error: {msg}"),
        }
    }
}

impl std::error::Error for ResolveError {}

// ─── DependencyProvider implementation ───────────────────────────────────────

/// A [`DependencyProvider`] backed by the local SQLite database.
///
/// [`DbDependencyProvider`] translates pubgrub's `choose_version` /
/// `get_dependencies` callbacks into queries against the `wit_package` and
/// `wit_package_dependency` tables.  Each call fetches fresh data from the
/// database; callers should ensure the DB is fully populated (e.g. after a
/// successful sync) before running the solver.
pub(crate) struct DbDependencyProvider<'s> {
    store: &'s Store,
}

impl<'s> DbDependencyProvider<'s> {
    /// Wrap a [`Store`] reference.
    pub(crate) fn new(store: &'s Store) -> Self {
        Self { store }
    }
}

impl DependencyProvider for DbDependencyProvider<'_> {
    type P = String;
    type V = WitVersion;
    type VS = WitVersionRange;
    type M = String;
    type Priority = u32;
    type Err = ResolveError;

    // r[impl resolution.per-version-deps]
    fn get_dependencies(
        &self,
        package: &String,
        version: &WitVersion,
    ) -> Result<Dependencies<String, WitVersionRange, String>, ResolveError> {
        let ver_str = version.to_string();
        let raw_deps = self
            .store
            .get_package_dependencies_by_name(package, Some(&ver_str))
            .map_err(|e| ResolveError::Db(e.to_string()))?;

        let mut constraints: HashMap<String, WitVersionRange> = HashMap::new();
        for dep in raw_deps {
            let range = match dep.version.as_deref() {
                Some(v) => {
                    // Strip a leading 'v' that some registries include (e.g. "v0.2.0").
                    let normalized = v.strip_prefix('v').unwrap_or(v);
                    match normalized.parse::<WitVersion>() {
                        Ok(sv) => Ranges::higher_than(sv),
                        Err(e) => {
                            return Err(ResolveError::Db(format!(
                                "unparseable version {v:?} for dependency `{}` of `{package}@{version}`: {e}",
                                dep.package
                            )));
                        }
                    }
                }
                None => Ranges::full(),
            };
            // Merge duplicate constraints for the same dependency by intersection.
            // This handles the (rare) case of multiple declared edges to the same
            // package; the resolver must satisfy *all* of them, not just the last one.
            if let Some(existing) = constraints.get_mut(&dep.package) {
                let merged = existing.intersection(&range);
                if merged.is_empty() {
                    return Err(ResolveError::NoSolution(format!(
                        "conflicting version constraints for dependency `{}` of `{package}@{version}`",
                        dep.package
                    )));
                }
                *existing = merged;
            } else {
                constraints.insert(dep.package, range);
            }
        }
        let constraints: DependencyConstraints<String, WitVersionRange> =
            constraints.into_iter().collect();
        Ok(Dependencies::Available(constraints))
    }

    fn choose_version(
        &self,
        package: &String,
        range: &WitVersionRange,
    ) -> Result<Option<WitVersion>, ResolveError> {
        let version_strings = self
            .store
            .list_wit_package_versions(package)
            .map_err(|e| ResolveError::Db(e.to_string()))?;

        // Parse each version string, collect valid ones, sort newest-first.
        let mut candidates: Vec<WitVersion> = version_strings
            .iter()
            .filter_map(|s| s.parse::<WitVersion>().ok())
            .collect();
        candidates.sort_unstable_by(|a, b| b.cmp(a)); // descending

        Ok(candidates.into_iter().find(|v| range.contains(v)))
    }

    fn prioritize(
        &self,
        _package: &String,
        _range: &WitVersionRange,
        stats: &PackageResolutionStatistics,
    ) -> u32 {
        stats.conflict_count()
    }
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Resolve the complete transitive dependency graph for a root package+version.
///
/// Returns a map from WIT package name to the single selected version for each
/// package in the resolved set (including the root package itself).
///
/// # Errors
///
/// Returns [`ResolveError::NoSolution`] when no conflict-free assignment
/// exists.  Returns [`ResolveError::Db`] when a database query fails.
// r[impl resolution.pubgrub]
pub(crate) fn resolve_from_db(
    store: &Store,
    package: impl Into<String>,
    version: WitVersion,
) -> Result<HashMap<String, WitVersion>, ResolveError> {
    let provider = DbDependencyProvider::new(store);
    let selected: SelectedDependencies<String, WitVersion> =
        pubgrub::resolve(&provider, package.into(), version).map_err(map_pubgrub_error)?;

    Ok(selected.into_iter().collect())
}

// ─── Multi-root resolution ────────────────────────────────────────────────

/// Sentinel package name used for the virtual root when resolving multiple
/// top-level packages in a single PubGrub pass.
const VIRTUAL_ROOT: &str = "<virtual-root>";

/// A [`DependencyProvider`] that wraps [`DbDependencyProvider`] with a
/// virtual root package whose dependencies are the set of top-level packages
/// to resolve together.
///
/// Root packages are special-cased in `choose_version`: the exact requested
/// version is returned directly (it does not need to be present in the DB).
/// This avoids spurious `NoSolution` errors for root packages that haven't
/// been indexed yet, letting the fallback installer handle their transitive
/// deps sequentially.
struct VirtualRootProvider<'s> {
    inner: DbDependencyProvider<'s>,
    root_deps: DependencyConstraints<String, WitVersionRange>,
    /// Exact versions requested for each root package.  Used by
    /// `choose_version` so roots don't need to be in the DB.
    root_versions: HashMap<String, WitVersion>,
}

impl DependencyProvider for VirtualRootProvider<'_> {
    type P = String;
    type V = WitVersion;
    type VS = WitVersionRange;
    type M = String;
    type Priority = u32;
    type Err = ResolveError;

    fn get_dependencies(
        &self,
        package: &String,
        version: &WitVersion,
    ) -> Result<Dependencies<String, WitVersionRange, String>, ResolveError> {
        if package == VIRTUAL_ROOT {
            Ok(Dependencies::Available(self.root_deps.clone()))
        } else {
            self.inner.get_dependencies(package, version)
        }
    }

    fn choose_version(
        &self,
        package: &String,
        range: &WitVersionRange,
    ) -> Result<Option<WitVersion>, ResolveError> {
        if package == VIRTUAL_ROOT {
            let v = WitVersion::new(0, 0, 0);
            return Ok(if range.contains(&v) { Some(v) } else { None });
        }
        // For root packages, return the exact requested version if it
        // satisfies the range — this avoids requiring the package to be
        // present in the DB (unindexed roots fall back to sequential
        // transitive dep discovery).
        if let Some(root_ver) = self.root_versions.get(package)
            && range.contains(root_ver)
        {
            return Ok(Some(*root_ver));
        }
        self.inner.choose_version(package, range)
    }

    fn prioritize(
        &self,
        package: &String,
        range: &WitVersionRange,
        stats: &PackageResolutionStatistics,
    ) -> u32 {
        if package == VIRTUAL_ROOT {
            0
        } else {
            self.inner.prioritize(package, range, stats)
        }
    }
}

/// All roots are fed into a single PubGrub pass via a virtual root package.
/// This ensures that shared transitive dependencies are resolved consistently
/// across all roots—the solver sees every constraint simultaneously rather
/// than producing independent (potentially conflicting) per-root results.
///
/// Returns a map from WIT package name to the selected version for every
/// package in the resolved set. The virtual root is stripped from the output.
///
/// # Errors
///
/// Returns [`ResolveError::NoSolution`] when no conflict-free assignment
/// exists.  Returns [`ResolveError::Db`] when a database query fails.
pub(crate) fn resolve_all_from_db(
    store: &Store,
    roots: &[(String, WitVersion)],
) -> Result<HashMap<String, WitVersion>, ResolveError> {
    if roots.is_empty() {
        return Ok(HashMap::new());
    }

    let mut root_constraints: HashMap<String, WitVersionRange> = HashMap::new();
    let mut root_versions: HashMap<String, WitVersion> = HashMap::new();
    for (name, version) in roots {
        let new_range = Ranges::singleton(*version);
        match root_constraints.get(name) {
            None => {
                root_constraints.insert(name.clone(), new_range);
                root_versions.insert(name.clone(), *version);
            }
            Some(existing_range) => {
                let intersection = existing_range.intersection(&new_range);
                if intersection.is_empty() {
                    return Err(ResolveError::NoSolution(format!(
                        "root package `{name}` has incompatible version requirements",
                    )));
                }
                root_constraints.insert(name.clone(), intersection);
                // For same-version duplicates the value is identical;
                // `root_versions` retains the first insertion unchanged.
            }
        }
    }
    let root_deps: DependencyConstraints<String, WitVersionRange> =
        root_constraints.into_iter().collect();

    let provider = VirtualRootProvider {
        inner: DbDependencyProvider::new(store),
        root_deps,
        root_versions,
    };

    let selected: SelectedDependencies<String, WitVersion> = pubgrub::resolve(
        &provider,
        VIRTUAL_ROOT.to_string(),
        WitVersion::new(0, 0, 0),
    )
    .map_err(map_pubgrub_error)?;

    Ok(selected
        .into_iter()
        .filter(|(name, _)| name != VIRTUAL_ROOT)
        .collect())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::{
        storage::Migrations,
        types::{RawWitPackage, WitPackageDependency},
    };

    use super::*;

    // ── Helper: DepGraph ──────────────────────────────────────────────────────

    /// One entry in [`DepGraph`]'s package list:
    /// `(package_name, version, [(dep_name, dep_version)])`.
    type PackageEntry = (String, String, Vec<(String, String)>);

    /// A small in-memory dependency-graph builder used to author resolver tests.
    ///
    /// `DepGraph` lets tests declare a universe of `(package, version, deps)`
    /// triples and then ask the resolver to pick a conflict-free assignment for
    /// a given root package.  Data is stored in a fresh in-memory SQLite DB
    /// so the full `Store` → `DbDependencyProvider` → pubgrub pipeline is
    /// exercised on every call to [`resolve`](DepGraph::resolve).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut g = DepGraph::new();
    /// g.add("wasi:http", "0.2.0", &[("wasi:io", "0.2.0")]);
    /// g.add("wasi:io",   "0.2.0", &[]);
    /// let plan = g.resolve("wasi:http", "0.2.0").unwrap();
    /// assert_eq!(*plan.get("wasi:io").unwrap(), WitVersion::new(0, 2, 0));
    /// ```
    struct DepGraph {
        /// Accumulated `(name, version, [(dep_name, dep_version)])` entries.
        packages: Vec<PackageEntry>,
    }

    impl DepGraph {
        fn new() -> Self {
            Self { packages: vec![] }
        }

        /// Register a package version with its exact-version dependencies.
        ///
        /// `deps` is a slice of `(dep_name, dep_version)` pairs.  Each
        /// dependency will be stored as a singleton version constraint
        /// (i.e. "exactly this version").
        fn add(&mut self, name: &str, version: &str, deps: &[(&str, &str)]) -> &mut Self {
            self.packages.push((
                name.into(),
                version.into(),
                deps.iter()
                    .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
                    .collect(),
            ));
            self
        }

        /// Build a fresh in-memory DB, populate it with all registered
        /// packages, and run the resolver for the given root.
        fn resolve(
            &self,
            name: &str,
            version: &str,
        ) -> Result<HashMap<String, WitVersion>, ResolveError> {
            let store = self.build_store()?;
            let root_version = version
                .parse::<WitVersion>()
                .map_err(|e| ResolveError::Db(format!("invalid version {version:?}: {e}")))?;
            resolve_from_db(&store, name, root_version)
        }

        /// Build a fresh in-memory DB, populate it with all registered
        /// packages, and run the unified multi-root resolver.
        fn resolve_all(
            &self,
            roots: &[(&str, &str)],
        ) -> Result<HashMap<String, WitVersion>, ResolveError> {
            let store = self.build_store()?;
            let parsed_roots: Vec<(String, WitVersion)> = roots
                .iter()
                .map(|(name, version)| {
                    let v = version.parse::<WitVersion>().map_err(|e| {
                        ResolveError::Db(format!("invalid version {version:?}: {e}"))
                    })?;
                    Ok((name.to_string(), v))
                })
                .collect::<Result<Vec<_>, ResolveError>>()?;
            resolve_all_from_db(&store, &parsed_roots)
        }

        /// Set up a fresh in-memory SQLite database and populate it with
        /// all registered packages.
        fn build_store(&self) -> Result<Store, ResolveError> {
            let conn = Connection::open_in_memory().map_err(|e| ResolveError::Db(e.to_string()))?;
            Migrations::run_all(&conn)
                .map_err(|e: anyhow::Error| ResolveError::Db(e.to_string()))?;

            for (pkg_name, pkg_ver, pkg_deps) in &self.packages {
                let pkg_id = RawWitPackage::insert(
                    &conn,
                    pkg_name,
                    Some(pkg_ver.as_str()),
                    None,
                    None,
                    None,
                    None,
                )
                .map_err(|e| ResolveError::Db(e.to_string()))?;

                for (dep_name, dep_ver) in pkg_deps {
                    WitPackageDependency::insert(
                        &conn,
                        pkg_id,
                        dep_name.as_str(),
                        Some(dep_ver.as_str()),
                        None,
                    )
                    .map_err(|e| ResolveError::Db(e.to_string()))?;
                }
            }

            Ok(Store::from_conn(conn))
        }
    }

    // ── r[verify resolution.per-version-deps] ─────────────────────────────────

    /// Two versions of the same package declare *different* dependency sets.
    /// The resolver MUST use only the deps for the selected version.
    ///
    /// With lower-bound (`>=`) version semantics the exact chosen version is
    /// not pinned, so we verify the dependency *set* (which packages appear)
    /// rather than the exact version picked.
    #[test]
    // r[verify resolution.per-version-deps]
    fn per_version_deps_are_tracked_independently() {
        let mut g = DepGraph::new();
        // v0.1.0 pulls in lib-old; v0.2.0 pulls in lib-new (entirely different pkg).
        g.add("wasi:http", "0.1.0", &[("lib-old", "0.1.0")]);
        g.add("wasi:http", "0.2.0", &[("lib-new", "0.2.0")]);
        g.add("lib-old", "0.1.0", &[]);
        g.add("lib-new", "0.2.0", &[]);

        // Resolve v0.1.0 — must include lib-old, NOT lib-new.
        let plan = g.resolve("wasi:http", "0.1.0").unwrap();
        assert!(
            plan.contains_key("lib-old"),
            "expected lib-old in plan, got {plan:?}"
        );
        assert!(
            !plan.contains_key("lib-new"),
            "lib-new must NOT appear when resolving v0.1.0, got {plan:?}"
        );
        assert_eq!(
            *plan.get("wasi:http").expect("wasi:http"),
            WitVersion::new(0, 1, 0)
        );

        // Resolve v0.2.0 — must include lib-new, NOT lib-old.
        let plan = g.resolve("wasi:http", "0.2.0").unwrap();
        assert!(
            plan.contains_key("lib-new"),
            "expected lib-new in plan, got {plan:?}"
        );
        assert!(
            !plan.contains_key("lib-old"),
            "lib-old must NOT appear when resolving v0.2.0, got {plan:?}"
        );
        assert_eq!(
            *plan.get("wasi:http").expect("wasi:http"),
            WitVersion::new(0, 2, 0)
        );
    }

    // ── r[verify resolution.transitive] ───────────────────────────────────────

    /// A → B → C: resolving A must include C.
    #[test]
    // r[verify resolution.transitive]
    fn transitive_deps_are_included() {
        let mut g = DepGraph::new();
        g.add("wasi:http", "0.2.0", &[("wasi:io", "0.2.0")]);
        g.add("wasi:io", "0.2.0", &[("wasi:clocks", "0.2.0")]);
        g.add("wasi:clocks", "0.2.0", &[]);

        let plan = g.resolve("wasi:http", "0.2.0").unwrap();
        assert!(
            plan.contains_key("wasi:clocks"),
            "expected wasi:clocks in plan, got {plan:?}"
        );
        assert_eq!(
            *plan.get("wasi:clocks").expect("wasi:clocks"),
            WitVersion::new(0, 2, 0)
        );
        assert_eq!(
            *plan.get("wasi:io").expect("wasi:io"),
            WitVersion::new(0, 2, 0)
        );
        assert_eq!(
            *plan.get("wasi:http").expect("wasi:http"),
            WitVersion::new(0, 2, 0)
        );
    }

    // ── r[verify resolution.diamond] ──────────────────────────────────────────

    /// A depends on B and C; both B and C depend on D@0.2.0.
    /// D MUST appear exactly once in the resolved set.
    #[test]
    // r[verify resolution.diamond]
    fn diamond_dep_appears_once() {
        let mut g = DepGraph::new();
        g.add(
            "app",
            "1.0.0",
            &[("wasi:http", "0.2.0"), ("wasi:io", "0.2.0")],
        );
        g.add("wasi:http", "0.2.0", &[("wasi:clocks", "0.2.0")]);
        g.add("wasi:io", "0.2.0", &[("wasi:clocks", "0.2.0")]);
        g.add("wasi:clocks", "0.2.0", &[]);

        let plan = g.resolve("app", "1.0.0").unwrap();
        // Plan is a map so duplicates are impossible by construction; verify
        // the single entry has the right version.
        assert_eq!(
            *plan.get("wasi:clocks").expect("wasi:clocks"),
            WitVersion::new(0, 2, 0)
        );
        assert_eq!(plan.len(), 4);
    }

    // ── r[verify resolution.conflict-detection] ───────────────────────────────

    /// Resolution MUST fail with `NoSolution` when the intersection of all
    /// lower-bound constraints for a package is non-empty but no version in
    /// the DB satisfies it.
    ///
    /// Here app depends on pkg-b (which needs shared >=0.1.0) *and* pkg-c
    /// (which needs shared >=0.3.0).  Combined lower bound = >=0.3.0, but
    /// the DB only has shared@0.1.0 and shared@0.2.0 — no solution exists.
    #[test]
    // r[verify resolution.conflict-detection]
    fn conflicting_constraints_produce_error() {
        let mut g = DepGraph::new();
        g.add("app", "1.0.0", &[("pkg-b", "1.0.0"), ("pkg-c", "1.0.0")]);
        g.add("pkg-b", "1.0.0", &[("shared", "0.1.0")]);
        g.add("pkg-c", "1.0.0", &[("shared", "0.3.0")]); // combined lower-bound = >=0.3.0
        g.add("shared", "0.1.0", &[]);
        g.add("shared", "0.2.0", &[]); // 0.3.0 intentionally absent from DB

        let result = g.resolve("app", "1.0.0");
        assert!(
            matches!(result, Err(ResolveError::NoSolution(_))),
            "expected NoSolution, got: {result:?}"
        );
    }

    // ── Additional: package with no deps resolves to just itself ──────────────

    #[test]
    fn root_with_no_deps_resolves_to_self() {
        let mut g = DepGraph::new();
        g.add("wasi:clocks", "0.2.0", &[]);

        let plan = g.resolve("wasi:clocks", "0.2.0").unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(
            *plan.get("wasi:clocks").expect("wasi:clocks"),
            WitVersion::new(0, 2, 0)
        );
    }

    // ── Additional: lower-bound semantics pick the newest satisfying version ──

    /// When two packages express different lower-bound requirements for the
    /// same dependency, the resolver must satisfy *both* and pick the newest
    /// available version that meets the combined (higher) lower bound.
    ///
    /// `app` requires `wasi:io >= 0.2.0`.
    /// `wasi:http` (a dep of `app`) requires `wasi:io >= 0.2.3`.
    /// Combined: `wasi:io >= 0.2.3`.
    /// DB provides both 0.2.0 and 0.2.3 → chosen version must be 0.2.3.
    #[test]
    fn lower_bound_constraints_pick_newest_satisfying() {
        let mut g = DepGraph::new();
        g.add(
            "app",
            "1.0.0",
            &[("wasi:io", "0.2.0"), ("wasi:http", "0.2.0")],
        );
        g.add("wasi:http", "0.2.0", &[("wasi:io", "0.2.3")]);
        g.add("wasi:io", "0.2.0", &[]);
        g.add("wasi:io", "0.2.3", &[]);

        let plan = g.resolve("app", "1.0.0").unwrap();
        assert_eq!(
            *plan.get("wasi:io").expect("wasi:io"),
            WitVersion::new(0, 2, 3),
            "expected wasi:io 0.2.3 (the newest satisfying >=0.2.3), got {plan:?}"
        );
    }

    // ── Multi-root resolution ────────────────────────────────────────────────

    /// Two independent roots with a shared transitive dependency.  The
    /// unified resolver sees both constraints simultaneously and picks a
    /// single consistent version.
    #[test]
    fn multi_root_resolves_shared_dep_consistently() {
        let mut g = DepGraph::new();
        g.add("wasi:http", "0.2.0", &[("wasi:io", "0.2.0")]);
        g.add("wasi:cli", "0.3.0", &[("wasi:io", "0.2.3")]);
        g.add("wasi:io", "0.2.0", &[]);
        g.add("wasi:io", "0.2.3", &[]);

        let plan = g
            .resolve_all(&[("wasi:http", "0.2.0"), ("wasi:cli", "0.3.0")])
            .unwrap();

        // The solver must pick a single version of wasi:io that satisfies
        // both >=0.2.0 (from wasi:http) and >=0.2.3 (from wasi:cli).
        assert_eq!(
            *plan.get("wasi:io").expect("wasi:io"),
            WitVersion::new(0, 2, 3),
            "expected unified wasi:io 0.2.3, got {plan:?}"
        );
        // Both roots must appear in the result.
        assert!(plan.contains_key("wasi:http"));
        assert!(plan.contains_key("wasi:cli"));
    }

    /// Multi-root with a single root falls back to `resolve_from_db`.
    #[test]
    fn multi_root_single_root_matches_single_resolve() {
        let mut g = DepGraph::new();
        g.add("wasi:http", "0.2.0", &[("wasi:io", "0.2.0")]);
        g.add("wasi:io", "0.2.0", &[]);

        let single = g.resolve("wasi:http", "0.2.0").unwrap();
        let multi = g.resolve_all(&[("wasi:http", "0.2.0")]).unwrap();
        assert_eq!(single, multi);
    }

    /// Multi-root with no roots returns an empty map.
    #[test]
    fn multi_root_empty_returns_empty() {
        let g = DepGraph::new();
        let plan = g.resolve_all(&[]).unwrap();
        assert!(plan.is_empty());
    }

    /// Multi-root detects real conflicts (no version satisfies combined
    /// constraints).
    #[test]
    fn multi_root_detects_real_conflict() {
        let mut g = DepGraph::new();
        // wasi:http@0.2.0 needs shared >= 0.1.0
        // wasi:cli@0.3.0 needs shared >= 0.5.0
        // But only shared@0.1.0 and shared@0.2.0 exist — no version >= 0.5.0.
        g.add("wasi:http", "0.2.0", &[("shared", "0.1.0")]);
        g.add("wasi:cli", "0.3.0", &[("shared", "0.5.0")]);
        g.add("shared", "0.1.0", &[]);
        g.add("shared", "0.2.0", &[]);

        let result = g.resolve_all(&[("wasi:http", "0.2.0"), ("wasi:cli", "0.3.0")]);
        assert!(
            matches!(result, Err(ResolveError::NoSolution(_))),
            "expected NoSolution, got: {result:?}"
        );
    }

    /// Duplicate root names with the same version are merged (no error).
    #[test]
    fn multi_root_duplicate_same_version_succeeds() {
        let mut g = DepGraph::new();
        g.add("wasi:http", "0.2.0", &[("wasi:io", "0.2.0")]);
        g.add("wasi:io", "0.2.0", &[]);

        // Same package+version listed twice — should not error.
        let plan = g
            .resolve_all(&[("wasi:http", "0.2.0"), ("wasi:http", "0.2.0")])
            .unwrap();
        assert!(plan.contains_key("wasi:http"));
        assert_eq!(
            *plan.get("wasi:http").expect("wasi:http"),
            WitVersion::new(0, 2, 0)
        );
    }

    /// Duplicate root names with different versions produce `NoSolution`
    /// (the singleton ranges don't intersect).
    #[test]
    fn multi_root_duplicate_different_version_errors() {
        let mut g = DepGraph::new();
        g.add("wasi:http", "0.2.0", &[]);
        g.add("wasi:http", "0.3.0", &[]);

        let result = g.resolve_all(&[("wasi:http", "0.2.0"), ("wasi:http", "0.3.0")]);
        assert!(
            matches!(result, Err(ResolveError::NoSolution(_))),
            "expected NoSolution for incompatible root versions, got: {result:?}"
        );
    }
}
