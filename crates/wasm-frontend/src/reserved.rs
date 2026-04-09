//! Reserved namespace identifiers.
//!
//! These paths are reserved for application routes and MUST NOT be interpreted
//! as package namespace lookups.

// r[impl frontend.routing.reserved-namespaces]

/// Namespaces reserved for application routes.
const RESERVED_NAMESPACES: &[&str] = &[
    "about", "admin", "all", "api", "assets", "docs", "engines", "explore", "health", "login",
    "logout", "new", "register", "search", "settings", "signup", "static",
];

/// Returns `true` if the given string is a reserved namespace.
#[must_use]
pub(crate) fn is_reserved(namespace: &str) -> bool {
    RESERVED_NAMESPACES.contains(&namespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify frontend.routing.reserved-namespaces]
    #[test]
    fn login_is_reserved() {
        assert!(is_reserved("login"));
    }

    // r[verify frontend.routing.reserved-namespaces]
    #[test]
    fn all_is_reserved() {
        assert!(is_reserved("all"));
    }

    // r[verify frontend.routing.reserved-namespaces]
    #[test]
    fn health_is_reserved() {
        assert!(is_reserved("health"));
    }

    // r[verify frontend.routing.reserved-namespaces]
    #[test]
    fn engines_is_reserved() {
        assert!(is_reserved("engines"));
    }

    // r[verify frontend.routing.reserved-namespaces]
    #[test]
    fn regular_namespace_is_not_reserved() {
        assert!(!is_reserved("wasi"));
        assert!(!is_reserved("ba"));
    }
}
