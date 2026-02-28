use anyhow::Context;
use std::env;
use std::path::{Path, PathBuf};

use super::models::Migrations;

/// Information about the current state of the package manager.
#[derive(Debug, Clone)]
pub struct StateInfo {
    /// Path to the current executable
    executable: PathBuf,
    /// Path to the configuration file
    config_file: PathBuf,
    /// Path to the data storage directory
    data_dir: PathBuf,
    /// Path to the content-addressable store directory
    store_dir: PathBuf,
    /// Size of the store directory in bytes
    store_size: u64,
    /// Path to the metadata database file
    metadata_file: PathBuf,
    /// Size of the metadata file in bytes
    metadata_size: u64,
    /// Current migration version
    migration_current: u32,
    /// Total number of migrations available
    migration_total: u32,
}

impl StateInfo {
    /// Create a new StateInfo instance.
    pub fn new(
        migration_info: Migrations,
        store_size: u64,
        metadata_size: u64,
    ) -> anyhow::Result<Self> {
        let data_dir = dirs::data_local_dir()
            .context("No local data dir known for the current OS")?
            .join("wasm");
        let config_file = dirs::config_dir()
            .context("No config dir known for the current OS")?
            .join("wasm")
            .join("config.toml");
        Ok(Self::new_at(
            data_dir,
            config_file,
            migration_info,
            store_size,
            metadata_size,
        ))
    }

    /// Create a new StateInfo instance at a specific data directory.
    #[must_use]
    pub fn new_at(
        data_dir: PathBuf,
        config_file: PathBuf,
        migration_info: Migrations,
        store_size: u64,
        metadata_size: u64,
    ) -> Self {
        Self {
            executable: env::current_exe().unwrap_or_else(|_| PathBuf::from("unknown")),
            config_file,
            store_dir: data_dir.join("store"),
            store_size,
            metadata_file: data_dir.join("db").join("metadata.db3"),
            metadata_size,
            data_dir,
            migration_current: migration_info.current,
            migration_total: migration_info.total,
        }
    }

    /// Override the executable path.
    ///
    /// By default, [`new_at`](Self::new_at) uses `env::current_exe()`.
    /// Use this to set a fixed path for deterministic output.
    #[must_use]
    pub fn with_executable(mut self, executable: PathBuf) -> Self {
        self.executable = executable;
        self
    }

    /// Get the path to the current executable
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Get the path to the configuration file
    #[must_use]
    pub fn config_file(&self) -> &Path {
        &self.config_file
    }

    /// Get the location of the crate's data dir
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Get the location of the crate's content-addressable store
    #[must_use]
    pub fn store_dir(&self) -> &Path {
        &self.store_dir
    }

    /// Get the size of the store directory in bytes
    #[must_use]
    pub fn store_size(&self) -> u64 {
        self.store_size
    }

    /// Get the location of the crate's metadata file
    #[must_use]
    pub fn metadata_file(&self) -> &Path {
        &self.metadata_file
    }

    /// Get the size of the metadata file in bytes
    #[must_use]
    pub fn metadata_size(&self) -> u64 {
        self.metadata_size
    }

    /// Get the current migration version
    #[must_use]
    pub fn migration_current(&self) -> u32 {
        self.migration_current
    }

    /// Get the total number of migrations available
    #[must_use]
    pub fn migration_total(&self) -> u32 {
        self.migration_total
    }

    /// Get the log directory for the application.
    ///
    /// Uses the XDG state directory (`$XDG_STATE_HOME/wasm/logs`) on Linux,
    /// and falls back to the data directory (`data_dir/logs`) on other systems.
    #[must_use]
    pub fn log_dir(&self) -> PathBuf {
        Self::default_log_dir()
    }

    /// Compute the default log directory for the application without an instance.
    ///
    /// Uses the XDG state directory (`$XDG_STATE_HOME/wasm/logs`) on Linux,
    /// and falls back to the local data directory on other systems.
    #[must_use]
    pub fn default_log_dir() -> PathBuf {
        dirs::state_dir()
            .map(|p| p.join("wasm").join("logs"))
            .unwrap_or_else(|| {
                dirs::data_local_dir()
                    .map(|p| p.join("wasm").join("logs"))
                    .unwrap_or_else(|| PathBuf::from("."))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_migrations() -> Migrations {
        Migrations {
            current: 3,
            total: 5,
        }
    }

    #[test]
    fn test_state_info_new_at() {
        let data_dir = PathBuf::from("/test/data");
        let config_file = PathBuf::from("/test/config/config.toml");
        let state_info = StateInfo::new_at(
            data_dir.clone(),
            config_file.clone(),
            test_migrations(),
            1024,
            512,
        );

        assert_eq!(state_info.data_dir(), data_dir);
        assert_eq!(state_info.config_file(), config_file);
        assert_eq!(state_info.store_dir(), data_dir.join("store"));
        assert_eq!(
            state_info.metadata_file(),
            data_dir.join("db").join("metadata.db3")
        );
        assert_eq!(state_info.store_size(), 1024);
        assert_eq!(state_info.metadata_size(), 512);
        assert_eq!(state_info.migration_current(), 3);
        assert_eq!(state_info.migration_total(), 5);
    }

    #[test]
    fn test_state_info_executable() {
        let data_dir = PathBuf::from("/test/data");
        let config_file = PathBuf::from("/test/config/config.toml");
        let state_info = StateInfo::new_at(data_dir, config_file, test_migrations(), 0, 0);

        // executable() should return something (either the actual exe or "unknown")
        let exe = state_info.executable();
        assert!(!exe.as_os_str().is_empty());
    }

    #[test]
    fn test_state_info_sizes() {
        let data_dir = PathBuf::from("/test/data");
        let config_file = PathBuf::from("/test/config/config.toml");

        // Test with various sizes
        let state_info = StateInfo::new_at(
            data_dir.clone(),
            config_file.clone(),
            test_migrations(),
            0,
            0,
        );
        assert_eq!(state_info.store_size(), 0);
        assert_eq!(state_info.metadata_size(), 0);

        let state_info = StateInfo::new_at(
            data_dir.clone(),
            config_file.clone(),
            test_migrations(),
            1024 * 1024,
            1024,
        );
        assert_eq!(state_info.store_size(), 1024 * 1024);
        assert_eq!(state_info.metadata_size(), 1024);
    }
}
