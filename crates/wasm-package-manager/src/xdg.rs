//! XDG Base Directory helpers.
//!
//! These helpers follow the [XDG Base Directory Specification] directly,
//! rather than using the platform-specific mappings provided by the `dirs`
//! crate.
//!
//! When `$XDG_CONFIG_HOME` is set it is always respected, regardless of
//! platform. When it is **not** set the fallback is:
//!
//! - **Unix / macOS**: `$HOME/.config`
//! - **Windows**: `%APPDATA%` (typically `C:\Users\<user>\AppData\Roaming`)
//!
//! [XDG Base Directory Specification]: https://specifications.freedesktop.org/basedir-spec/latest/

use std::env;
use std::path::PathBuf;

/// Return the XDG config home directory.
///
/// Uses `$XDG_CONFIG_HOME` if set on any platform. Otherwise falls back to
/// `$HOME/.config` on Unix/macOS or `%APPDATA%` on Windows.
pub(crate) fn xdg_config_home() -> PathBuf {
    if let Some(val) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(val);
    }
    platform_config_home()
}

/// Fallback when no XDG or platform-specific env var is set: `$HOME/.config`.
fn home_dot_config() -> PathBuf {
    dirs::home_dir().map_or_else(|| PathBuf::from(".config"), |h| h.join(".config"))
}

/// Platform-specific default when `$XDG_CONFIG_HOME` is not set.
#[cfg(windows)]
fn platform_config_home() -> PathBuf {
    // %APPDATA% is the conventional roaming config directory on Windows.
    env::var_os("APPDATA").map_or_else(home_dot_config, PathBuf::from)
}

/// Platform-specific default when `$XDG_CONFIG_HOME` is not set.
#[cfg(not(windows))]
fn platform_config_home() -> PathBuf {
    home_dot_config()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_config_home_returns_non_empty_path() {
        let path = xdg_config_home();
        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    fn xdg_config_home_defaults_correctly_when_env_unset() {
        let path = xdg_config_home();
        // When $XDG_CONFIG_HOME is not set, verify the platform default.
        if env::var_os("XDG_CONFIG_HOME").is_none() {
            if cfg!(windows) {
                // On Windows the fallback is %APPDATA%, which is always
                // expected to be set. If it is missing something is very
                // wrong with the environment, so we let the test fail.
                let appdata = env::var_os("APPDATA").expect("%APPDATA% should be set on Windows");
                assert_eq!(path, PathBuf::from(appdata));
            } else {
                assert!(
                    path.ends_with(".config"),
                    "expected path to end with .config, got: {}",
                    path.display()
                );
            }
        }
    }
}
