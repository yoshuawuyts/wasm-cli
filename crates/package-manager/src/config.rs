//! Global configuration module for the package manager.
//!
//! This module provides support for reading and managing a global TOML configuration
//! file at `$XDG_CONFIG_HOME/wasm/config.toml`. The configuration file supports
//! per-registry credential helpers for secure authentication.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::credential_helper::CredentialHelper;

/// Default configuration file content with commented examples.
const DEFAULT_CONFIG: &str = r#"# wasm(1) configuration file
# https://github.com/yoshuawuyts/wasm

# Per-registry credential helpers allow secure authentication with container registries.
# Credentials are fetched on-demand and never stored to disk.

# Example configurations (uncomment and modify as needed):

# Option 1: Single JSON command (recommended for 1Password)
# The command should output JSON with username and password fields:
# [{"id": "username", "value": "..."}, {"id": "password", "value": "..."}]
#
# [registries."ghcr.io"]
# credential-helper = "op item get ghcr --format json --fields username,password"

# Option 2: Two separate commands (for simple scripts)
#
# [registries."my-registry.example.com"]
# credential-helper.username = "/path/to/get-user.sh"
# credential-helper.password = "/path/to/get-pass.sh"
"#;

/// The main configuration struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Per-registry configuration.
    #[serde(default)]
    pub registries: HashMap<String, RegistryConfig>,

    /// Runtime credential cache (not serialized).
    #[serde(skip)]
    credential_cache: CredentialCache,
}

/// Thread-safe credential cache.
#[derive(Debug, Default)]
struct CredentialCache {
    cache: RwLock<HashMap<String, (String, String)>>,
}

impl Clone for CredentialCache {
    fn clone(&self) -> Self {
        // Use unwrap_or_default if the lock is poisoned - we'll just start with empty cache
        let cache = self
            .cache
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        Self {
            cache: RwLock::new(cache),
        }
    }
}

/// Configuration for a specific registry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RegistryConfig {
    /// Credential helper configuration for this registry.
    #[serde(rename = "credential-helper")]
    pub credential_helper: Option<CredentialHelper>,
}

impl Config {
    /// Load configuration from the default config directory.
    ///
    /// The configuration file is expected at `$XDG_CONFIG_HOME/wasm/config.toml`.
    /// If the file doesn't exist, returns a default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration file exists but cannot be read or parsed.
    pub fn load() -> Result<Self> {
        Self::load_from(None)
    }

    /// Load configuration from a specified directory (for testing).
    ///
    /// If `config_dir` is `None`, uses the default XDG config directory.
    /// If the file doesn't exist, returns a default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration file exists but cannot be read or parsed.
    pub fn load_from(config_dir: Option<PathBuf>) -> Result<Self> {
        let config_path = Self::config_path_from(config_dir);
        Self::load_from_path(&config_path)
    }

    /// Load configuration from a specific file path.
    ///
    /// If the file doesn't exist, returns a default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration file exists but cannot be read or parsed.
    pub fn load_from_path(config_path: &Path) -> Result<Self> {
        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;

        Ok(config)
    }

    /// Returns the path to the configuration file.
    #[must_use]
    pub fn config_path() -> PathBuf {
        Self::config_path_from(None)
    }

    /// Returns the path to the configuration file from a specified directory.
    #[must_use]
    pub fn config_path_from(config_dir: Option<PathBuf>) -> PathBuf {
        let base = config_dir
            .unwrap_or_else(|| dirs::config_dir().unwrap_or_else(|| PathBuf::from(".config")));
        base.join("wasm").join("config.toml")
    }

    /// Ensures the configuration file exists, creating a default one if not.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory or file cannot be created.
    pub fn ensure_exists() -> Result<PathBuf> {
        Self::ensure_exists_at(None)
    }

    /// Ensures the configuration file exists at a specified directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory or file cannot be created.
    pub fn ensure_exists_at(config_dir: Option<PathBuf>) -> Result<PathBuf> {
        let config_path = Self::config_path_from(config_dir);

        if config_path.exists() {
            return Ok(config_path);
        }

        // Create parent directory if needed
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        // Write default configuration
        fs::write(&config_path, DEFAULT_CONFIG).with_context(|| {
            format!(
                "Failed to write default config file: {}",
                config_path.display()
            )
        })?;

        Ok(config_path)
    }

    /// Get credentials for a registry using the configured credential helper.
    ///
    /// Returns `None` if no credential helper is configured for the registry.
    /// Results are cached in memory for subsequent calls.
    ///
    /// # Errors
    ///
    /// Returns an error if the credential helper command fails or returns invalid output.
    pub fn get_credentials(&self, registry: &str) -> Result<Option<(String, String)>> {
        // Check cache first - if lock is poisoned, skip cache and fetch fresh credentials
        if let Ok(cache) = self.credential_cache.cache.read()
            && let Some(creds) = cache.get(registry)
        {
            return Ok(Some(creds.clone()));
        }

        // Look up registry config
        let registry_config = match self.registries.get(registry) {
            Some(config) => config,
            None => return Ok(None),
        };

        // Check if credential helper is configured
        let helper = match &registry_config.credential_helper {
            Some(h) => h,
            None => return Ok(None),
        };

        // Execute credential helper
        let credentials = helper.execute()?;

        // Cache the result - if lock is poisoned, skip caching but still return credentials
        if let Ok(mut cache) = self.credential_cache.cache.write() {
            cache.insert(registry.to_string(), credentials.clone());
        }

        Ok(Some(credentials))
    }

    /// Clear the credential cache.
    pub fn clear_credential_cache(&self) {
        // If lock is poisoned, the cache is already in an undefined state - just skip clearing
        if let Ok(mut cache) = self.credential_cache.cache.write() {
            cache.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.registries.is_empty());
    }

    #[test]
    fn test_config_load_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::load_from(Some(temp_dir.path().to_path_buf())).unwrap();
        assert!(config.registries.is_empty());
    }

    #[test]
    fn test_config_load_valid() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("wasm");
        fs::create_dir_all(&config_dir).unwrap();

        let config_path = config_dir.join("config.toml");
        let toml_content = r#"
[registries."ghcr.io"]
credential-helper = "echo test"
"#;
        fs::write(&config_path, toml_content).unwrap();

        let config = Config::load_from(Some(temp_dir.path().to_path_buf())).unwrap();
        assert!(config.registries.contains_key("ghcr.io"));
    }

    #[test]
    fn test_config_load_split_helper() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("wasm");
        fs::create_dir_all(&config_dir).unwrap();

        let config_path = config_dir.join("config.toml");
        let toml_content = r#"
[registries."my-registry.example.com"]
credential-helper.username = "/path/to/get-user.sh"
credential-helper.password = "/path/to/get-pass.sh"
"#;
        fs::write(&config_path, toml_content).unwrap();

        let config = Config::load_from(Some(temp_dir.path().to_path_buf())).unwrap();
        let registry_config = config.registries.get("my-registry.example.com").unwrap();

        match &registry_config.credential_helper {
            Some(CredentialHelper::Split { username, password }) => {
                assert_eq!(username, "/path/to/get-user.sh");
                assert_eq!(password, "/path/to/get-pass.sh");
            }
            _ => panic!("Expected Split credential helper"),
        }
    }

    #[test]
    fn test_config_ensure_exists() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = Config::ensure_exists_at(Some(temp_dir.path().to_path_buf())).unwrap();

        assert!(config_path.exists());

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("credential-helper"));
    }

    #[test]
    fn test_config_ensure_exists_idempotent() {
        let temp_dir = TempDir::new().unwrap();

        // First call creates the file
        let path1 = Config::ensure_exists_at(Some(temp_dir.path().to_path_buf())).unwrap();

        // Modify the file
        let mut file = fs::OpenOptions::new().append(true).open(&path1).unwrap();
        writeln!(file, "# custom comment").unwrap();

        // Second call should not overwrite
        let path2 = Config::ensure_exists_at(Some(temp_dir.path().to_path_buf())).unwrap();
        assert_eq!(path1, path2);

        let content = fs::read_to_string(&path2).unwrap();
        assert!(content.contains("# custom comment"));
    }

    #[test]
    fn test_credential_cache() {
        // Write JSON to a temp file to avoid shell quoting issues across platforms
        let json = r#"[{"id": "username", "value": "user"}, {"id": "password", "value": "pass"}]"#;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        f.write_all(json.as_bytes()).unwrap();
        let tmp_path = f.into_temp_path();
        let path_str = tmp_path.to_str().unwrap();
        let echo_cmd = if cfg!(target_os = "windows") {
            format!("type {path_str}")
        } else {
            format!("cat {path_str}")
        };

        let mut registries = HashMap::new();
        registries.insert(
            "test.io".to_string(),
            RegistryConfig {
                credential_helper: Some(CredentialHelper::Json(echo_cmd)),
            },
        );
        let config = Config {
            registries,
            ..Config::default()
        };

        // First call should execute the helper
        let creds = config.get_credentials("test.io").unwrap();
        assert_eq!(creds, Some(("user".to_string(), "pass".to_string())));

        // Clear cache
        config.clear_credential_cache();

        // After clearing, no cached entry
        let cache = config.credential_cache.cache.read().unwrap();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_get_credentials_no_helper() {
        let config = Config::default();
        let creds = config.get_credentials("unknown.io").unwrap();
        assert!(creds.is_none());
    }
}
