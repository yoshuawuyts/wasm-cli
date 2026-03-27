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

use std::ffi::OsString;
use std::path::PathBuf;

/// Return the XDG config home directory.
///
/// Uses `$XDG_CONFIG_HOME` if set (and non-empty) on any platform. Otherwise
/// falls back to `$HOME/.config` on Unix/macOS or `%APPDATA%` on Windows.
///
/// Returns `None` when no suitable directory can be determined (e.g. neither
/// `$XDG_CONFIG_HOME`, `$HOME`, nor the platform-specific fallback is
/// available).
pub(crate) fn xdg_config_home() -> Option<PathBuf> {
    resolve_config_home(
        std::env::var_os("XDG_CONFIG_HOME"),
        dirs::home_dir(),
        platform_env(),
    )
}

/// Pure implementation that resolves the config home from explicit inputs.
///
/// This is separated from [`xdg_config_home`] so it can be tested
/// deterministically without depending on the process environment.
fn resolve_config_home(
    xdg_config_home: Option<OsString>,
    home_dir: Option<PathBuf>,
    platform_dir: Option<PathBuf>,
) -> Option<PathBuf> {
    // Honor $XDG_CONFIG_HOME when set to a non-empty, absolute path.
    if let Some(val) = xdg_config_home {
        let path = PathBuf::from(val);
        if !path.as_os_str().is_empty() && path.is_absolute() {
            return Some(path);
        }
    }

    // Platform-specific fallback (e.g. %APPDATA% on Windows).
    if let Some(dir) = platform_dir {
        return Some(dir);
    }

    // Final fallback: $HOME/.config
    home_dir.map(|h| h.join(".config"))
}

/// Return the platform-specific config env var, if any.
#[cfg(windows)]
fn platform_env() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

/// Return the platform-specific config env var, if any.
#[cfg(not(windows))]
fn platform_env() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Tests for the pure `resolve_config_home` helper
    // ------------------------------------------------------------------

    #[test]
    fn respects_absolute_xdg_config_home() {
        let result = resolve_config_home(
            Some(OsString::from("/custom/config")),
            Some(PathBuf::from("/home/user")),
            None,
        );
        assert_eq!(result, Some(PathBuf::from("/custom/config")));
    }

    #[test]
    fn ignores_empty_xdg_config_home() {
        let result = resolve_config_home(
            Some(OsString::from("")),
            Some(PathBuf::from("/home/user")),
            None,
        );
        assert_eq!(result, Some(PathBuf::from("/home/user/.config")));
    }

    #[test]
    fn ignores_relative_xdg_config_home() {
        let result = resolve_config_home(
            Some(OsString::from("relative/path")),
            Some(PathBuf::from("/home/user")),
            None,
        );
        assert_eq!(result, Some(PathBuf::from("/home/user/.config")));
    }

    #[test]
    fn falls_back_to_platform_dir() {
        let result = resolve_config_home(
            None,
            Some(PathBuf::from("/home/user")),
            Some(PathBuf::from("/appdata/roaming")),
        );
        assert_eq!(result, Some(PathBuf::from("/appdata/roaming")));
    }

    #[test]
    fn falls_back_to_home_dot_config() {
        let result = resolve_config_home(None, Some(PathBuf::from("/home/user")), None);
        assert_eq!(result, Some(PathBuf::from("/home/user/.config")));
    }

    #[test]
    fn returns_none_when_nothing_available() {
        let result = resolve_config_home(None, None, None);
        assert_eq!(result, None);
    }

    #[test]
    fn xdg_overrides_platform_dir() {
        let result = resolve_config_home(
            Some(OsString::from("/xdg/override")),
            Some(PathBuf::from("/home/user")),
            Some(PathBuf::from("/appdata/roaming")),
        );
        assert_eq!(result, Some(PathBuf::from("/xdg/override")));
    }

    // ------------------------------------------------------------------
    // Integration smoke test using the real environment
    // ------------------------------------------------------------------

    #[test]
    fn xdg_config_home_returns_absolute_or_none() {
        if let Some(path) = xdg_config_home() {
            assert!(
                path.is_absolute(),
                "xdg_config_home() should return an absolute path, got: {}",
                path.display()
            );
        }
    }
}
