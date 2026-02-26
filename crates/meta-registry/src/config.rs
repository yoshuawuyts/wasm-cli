//! Configuration for the meta-registry server.
//!
//! Parses a TOML configuration file that specifies which OCI registries
//! and repositories to index, the sync interval, and the HTTP bind address.

use serde::Deserialize;

/// Top-level configuration for the meta-registry server.
#[derive(Debug, Clone, Deserialize)]
#[must_use]
pub struct Config {
    /// Sync interval in seconds (default: 3600).
    #[serde(default = "default_sync_interval")]
    pub sync_interval: u64,

    /// HTTP server bind address (default: "0.0.0.0:8080").
    #[serde(default = "default_bind")]
    pub bind: String,

    /// List of OCI packages to index.
    #[serde(default)]
    pub packages: Vec<PackageSource>,
}

/// A single OCI package source to index.
#[derive(Debug, Clone, Deserialize)]
#[must_use]
pub struct PackageSource {
    /// OCI registry hostname (e.g., "ghcr.io").
    pub registry: String,
    /// OCI repository path (e.g., "webassembly/wasi/clocks").
    pub repository: String,
}

/// Default sync interval: 1 hour.
fn default_sync_interval() -> u64 {
    3600
}

/// Default bind address.
fn default_bind() -> String {
    "0.0.0.0:8080".to_string()
}

impl Config {
    /// Parse a TOML configuration string.
    ///
    /// # Errors
    ///
    /// Returns an error if the TOML is invalid or missing required fields.
    pub fn from_toml(toml_str: &str) -> anyhow::Result<Self> {
        let config: Config = toml::from_str(toml_str)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
sync_interval = 1800
bind = "127.0.0.1:9090"

[[packages]]
registry = "ghcr.io"
repository = "bytecodealliance/sample-wasi-http-rust"

[[packages]]
registry = "ghcr.io"
repository = "webassembly/wasi/clocks"
"#;

        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.sync_interval, 1800);
        assert_eq!(config.bind, "127.0.0.1:9090");
        assert_eq!(config.packages.len(), 2);
        assert_eq!(config.packages.get(0).unwrap().registry, "ghcr.io");
        assert_eq!(
            config.packages.get(0).unwrap().repository,
            "bytecodealliance/sample-wasi-http-rust"
        );
        assert_eq!(config.packages.get(1).unwrap().registry, "ghcr.io");
        assert_eq!(
            config.packages.get(1).unwrap().repository,
            "webassembly/wasi/clocks"
        );
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml = "";
        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.sync_interval, 3600);
        assert_eq!(config.bind, "0.0.0.0:8080");
        assert!(config.packages.is_empty());
    }

    #[test]
    fn test_parse_defaults() {
        let toml = r#"
[[packages]]
registry = "ghcr.io"
repository = "user/repo"
"#;

        let config = Config::from_toml(toml).unwrap();
        assert_eq!(config.sync_interval, 3600);
        assert_eq!(config.bind, "0.0.0.0:8080");
        assert_eq!(config.packages.len(), 1);
    }

    #[test]
    fn test_parse_invalid_toml() {
        let toml = "this is not valid toml [[[";
        assert!(Config::from_toml(toml).is_err());
    }

    #[test]
    fn test_parse_missing_required_fields() {
        let toml = r#"
[[packages]]
registry = "ghcr.io"
"#;
        // repository is required, so this should fail
        assert!(Config::from_toml(toml).is_err());
    }
}
