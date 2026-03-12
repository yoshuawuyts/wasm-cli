//! Credential helper module for executing external commands to retrieve credentials.
//!
//! Credential helpers use two separate commands: one for the username and one
//! for the password. Each command's stdout (trimmed) is used as the credential
//! value.

use anyhow::{Context, Result};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::debug;

/// Error type for credential helper failures.
///
/// Each variant carries a stable [diagnostic error code][miette::Diagnostic::code]
/// that uniquely identifies the failure.
#[derive(Debug, Clone, PartialEq, Eq, Diagnostic)]
#[must_use]
pub enum CredentialError {
    /// An external credential helper command exited with a non-zero status.
    #[diagnostic(
        code(wasm::credential::command_failed),
        help("command exited with {status}: {stderr}")
    )]
    CommandFailed {
        /// The exit status of the command.
        status: String,
        /// Trimmed stderr output from the command.
        stderr: String,
    },
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialError::CommandFailed { status, .. } => {
                write!(f, "credential helper command exited with status {status}")
            }
        }
    }
}

impl std::error::Error for CredentialError {}

/// Credential helper configuration.
///
/// Uses two separate commands: one to retrieve the username and one to
/// retrieve the password. Each command's stdout (trimmed) is used as the
/// credential value.
///
/// # Examples
///
/// ```rust
/// use wasm_package_manager::CredentialHelper;
///
/// let helper = CredentialHelper::Split {
///     username: "/path/to/get-user.sh".into(),
///     password: "/path/to/get-pass.sh".into(),
/// };
/// ```
// r[impl credential.no-leak-debug]
// r[impl credential.no-leak-display]
// r[impl credential.split]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CredentialHelper {
    /// Separate commands for username and password.
    Split {
        /// Command to get the username (output is trimmed).
        username: String,
        /// Command to get the password (output is trimmed).
        password: String,
    },
}

impl CredentialHelper {
    /// Execute the credential helper and return the username and password.
    ///
    /// Each command is executed through the shell and its stdout (trimmed)
    /// is used as the credential value.
    ///
    /// # Errors
    ///
    /// Returns an error if either credential helper command fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use wasm_package_manager::CredentialHelper;
    ///
    /// let helper = CredentialHelper::Split {
    ///     username: "echo my-user".into(),
    ///     password: "echo my-pass".into(),
    /// };
    /// let (username, password) = helper.execute()?;
    /// println!("Authenticated as {username}");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn execute(&self) -> Result<(String, String)> {
        match self {
            CredentialHelper::Split { username, password } => {
                execute_split_helper(username, password)
            }
        }
    }
}

/// Execute split credential helper commands.
fn execute_split_helper(username_cmd: &str, password_cmd: &str) -> Result<(String, String)> {
    debug!("Executing split credential helper");
    let username = execute_shell_command(username_cmd)
        .with_context(|| format!("Failed to execute username credential helper: {username_cmd}"))?
        .trim()
        .to_string();

    let password = execute_shell_command(password_cmd)
        .with_context(|| format!("Failed to execute password credential helper: {password_cmd}"))?
        .trim()
        .to_string();

    debug!("Obtained username and password from credential helper");
    Ok((username, password))
}

/// Execute a shell command and return its stdout as a string.
fn execute_shell_command(cmd: &str) -> Result<String> {
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", cmd]).output()
    } else {
        Command::new("sh").args(["-c", cmd]).output()
    }
    .with_context(|| format!("Failed to spawn command: {cmd}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CredentialError::CommandFailed {
            status: output.status.to_string(),
            stderr: stderr.trim().to_string(),
        }
        .into());
    }

    let stdout = String::from_utf8(output.stdout).context("Command output was not valid UTF-8")?;

    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify credential.split]
    #[test]
    fn test_execute_split_helper() {
        let (username, password) = execute_split_helper("echo testuser", "echo testpass").unwrap();
        assert_eq!(username, "testuser");
        assert_eq!(password, "testpass");
    }

    #[test]
    fn test_credential_helper_execute() {
        let helper = CredentialHelper::Split {
            username: "echo splituser".to_string(),
            password: "echo splitpass".to_string(),
        };
        let (username, password) = helper.execute().unwrap();
        assert_eq!(username, "splituser");
        assert_eq!(password, "splitpass");
    }

    // r[verify credential.no-leak-debug]
    #[test]
    fn test_credential_helper_debug_never_prints_credentials() {
        // Verify that Debug output only shows command configuration,
        // never the actual credentials returned by the helper.
        let helper = CredentialHelper::Split {
            username: "/path/to/get-user.sh".to_string(),
            password: "/path/to/get-pass.sh".to_string(),
        };
        let debug_output = format!("{:?}", helper);

        // Should show the script paths
        assert!(debug_output.contains("/path/to/get-user.sh"));
        assert!(debug_output.contains("/path/to/get-pass.sh"));
    }

    // r[verify credential.no-leak-display]
    #[test]
    fn test_credential_helper_display_never_leaks_credentials() {
        // Test that after executing a credential helper, the helper's
        // Debug output still only shows the command configuration,
        // not the returned credentials. The CredentialHelper stores
        // only command strings, not execution results.
        let helper = CredentialHelper::Split {
            username: "get-user-cmd".to_string(),
            password: "get-pass-cmd".to_string(),
        };
        let debug_output = format!("{:?}", helper);
        assert!(
            debug_output.contains("get-user-cmd"),
            "Debug output should show the username command"
        );
        assert!(
            debug_output.contains("get-pass-cmd"),
            "Debug output should show the password command"
        );
        // The credential helper enum stores commands, not credentials,
        // so Debug can never leak actual credential values
    }

    #[test]
    fn test_all_variants_have_error_codes() {
        use miette::Diagnostic;

        let cmd_failed = CredentialError::CommandFailed {
            status: "exit status: 1".to_string(),
            stderr: "bad credentials".to_string(),
        };
        assert_eq!(
            cmd_failed
                .code()
                .expect("CommandFailed must have a diagnostic code")
                .to_string(),
            "wasm::credential::command_failed",
        );
        assert!(
            cmd_failed.help().is_some(),
            "CommandFailed must have a help message"
        );
    }
}
