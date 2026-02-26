//! CLI configuration loaded from `~/.config/wasm/config.toml`.
//!
//! If the file is missing, all values fall back to built-in defaults.

use serde::Deserialize;

/// Top-level CLI configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[must_use]
pub(crate) struct CliConfig {
    /// Meta-registry settings.
    #[serde(default)]
    pub(crate) registry: RegistryConfig,
}

/// Settings for syncing packages from a meta-registry.
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct RegistryConfig {
    /// Base URL of the meta-registry (e.g., `http://localhost:8081`).
    /// When `None`, HTTP sync is disabled.
    pub(crate) url: Option<String>,

    /// Minimum interval between automatic syncs, in seconds.
    /// Defaults to 3600 (1 hour).
    pub(crate) sync_interval: Option<u64>,
}

impl RegistryConfig {
    /// Returns the sync interval, defaulting to 3600 seconds.
    #[must_use]
    pub(crate) fn sync_interval(&self) -> u64 {
        self.sync_interval.unwrap_or(3600)
    }
}

impl CliConfig {
    /// Load configuration from `~/.config/wasm/config.toml`.
    ///
    /// Returns the default config if the file does not exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but contains invalid TOML.
    pub(crate) fn load() -> anyhow::Result<Self> {
        let Some(config_dir) = dirs::config_local_dir() else {
            return Ok(Self::default());
        };

        let path = config_dir.join("wasm").join("config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&path)?;
        let config: CliConfig = toml::from_str(&contents)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CliConfig::default();
        assert!(config.registry.url.is_none());
        assert_eq!(config.registry.sync_interval(), 3600);
    }

    #[test]
    fn test_parse_full_config() {
        let toml_str = r#"
[registry]
url = "http://localhost:8081"
sync_interval = 1800
"#;
        let config: CliConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.registry.url,
            Some("http://localhost:8081".to_string())
        );
        assert_eq!(config.registry.sync_interval(), 1800);
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml_str = "";
        let config: CliConfig = toml::from_str(toml_str).unwrap();
        assert!(config.registry.url.is_none());
        assert_eq!(config.registry.sync_interval(), 3600);
    }

    #[test]
    fn test_parse_partial_registry_config() {
        let toml_str = r#"
[registry]
url = "https://example.com"
"#;
        let config: CliConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.registry.url, Some("https://example.com".to_string()));
        assert_eq!(config.registry.sync_interval(), 3600);
    }
}
