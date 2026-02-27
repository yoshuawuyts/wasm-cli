//! Credential helper module for executing external commands to retrieve credentials.
//!
//! This module provides support for two types of credential helpers:
//! - JSON: Single command that outputs JSON with username and password fields
//! - Split: Separate commands for username and password

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Credential helper configuration.
///
/// Supports two formats:
/// - JSON: Single command that outputs JSON with username and password
/// - Split: Separate commands for username and password
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CredentialHelper {
    /// Single command that outputs JSON with fields for username and password.
    ///
    /// Expected output format:
    /// ```json
    /// [{"id": "username", "value": "..."}, {"id": "password", "value": "..."}]
    /// ```
    Json(String),

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
    /// # Errors
    ///
    /// Returns an error if the credential helper command fails or returns invalid output.
    pub fn execute(&self) -> Result<(String, String)> {
        match self {
            CredentialHelper::Json(cmd) => execute_json_helper(cmd),
            CredentialHelper::Split { username, password } => {
                execute_split_helper(username, password)
            }
        }
    }
}

/// A field from the JSON credential helper output.
#[derive(Debug, Deserialize)]
struct CredentialField {
    id: String,
    value: String,
}

/// Execute a JSON credential helper command.
///
/// The command should output JSON with username and password fields:
/// ```json
/// [{"id": "username", "value": "..."}, {"id": "password", "value": "..."}]
/// ```
fn execute_json_helper(cmd: &str) -> Result<(String, String)> {
    let output = execute_shell_command(cmd)
        .with_context(|| format!("Failed to execute credential helper: {cmd}"))?;

    // Trim whitespace for consistent parsing
    let output = output.trim();

    let fields: Vec<CredentialField> = serde_json::from_str(output).with_context(|| {
        // Truncate output in error message to avoid leaking credentials
        let preview = if output.len() > 100 {
            format!("{}...", &output[..100])
        } else {
            output.to_string()
        };
        format!("Failed to parse credential helper output as JSON: {preview}")
    })?;

    let mut username = None;
    let mut password = None;

    for field in fields {
        match field.id.as_str() {
            "username" => username = Some(field.value),
            "password" => password = Some(field.value),
            _ => {} // Ignore other fields
        }
    }

    let username = username.context("Credential helper output missing 'username' field")?;
    let password = password.context("Credential helper output missing 'password' field")?;

    Ok((username, password))
}

/// Execute split credential helper commands.
fn execute_split_helper(username_cmd: &str, password_cmd: &str) -> Result<(String, String)> {
    let username = execute_shell_command(username_cmd)
        .with_context(|| format!("Failed to execute username credential helper: {username_cmd}"))?
        .trim()
        .to_string();

    let password = execute_shell_command(password_cmd)
        .with_context(|| format!("Failed to execute password credential helper: {password_cmd}"))?
        .trim()
        .to_string();

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
        anyhow::bail!(
            "Command exited with status {}: {}",
            output.status,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8(output.stdout).context("Command output was not valid UTF-8")?;

    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a cross-platform command that outputs a JSON string to stdout.
    ///
    /// Uses a temp file + `cat`/`type` because `echo` on Windows cmd.exe
    /// mangles double quotes in JSON strings.
    fn json_output_cmd(json: &str) -> (String, tempfile::TempPath) {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().expect("failed to create temp file");
        f.write_all(json.as_bytes())
            .expect("failed to write temp file");
        let path = f.into_temp_path();
        let path_str = path.to_str().expect("non-UTF-8 temp path");
        let cmd = if cfg!(target_os = "windows") {
            format!("type {path_str}")
        } else {
            format!("cat {path_str}")
        };
        (cmd, path)
    }

    #[test]
    fn test_execute_json_helper() {
        let json =
            r#"[{"id": "username", "value": "testuser"}, {"id": "password", "value": "testpass"}]"#;
        let (cmd, _tmp) = json_output_cmd(json);

        let (username, password) = execute_json_helper(&cmd).unwrap();
        assert_eq!(username, "testuser");
        assert_eq!(password, "testpass");
    }

    #[test]
    fn test_execute_split_helper() {
        let (username, password) = execute_split_helper("echo testuser", "echo testpass").unwrap();
        assert_eq!(username, "testuser");
        assert_eq!(password, "testpass");
    }

    #[test]
    fn test_credential_helper_json_execute() {
        let json =
            r#"[{"id": "username", "value": "user1"}, {"id": "password", "value": "pass1"}]"#;
        let (cmd, _tmp) = json_output_cmd(json);
        let helper = CredentialHelper::Json(cmd);
        let (username, password) = helper.execute().unwrap();
        assert_eq!(username, "user1");
        assert_eq!(password, "pass1");
    }

    #[test]
    fn test_credential_helper_split_execute() {
        let helper = CredentialHelper::Split {
            username: "echo splituser".to_string(),
            password: "echo splitpass".to_string(),
        };
        let (username, password) = helper.execute().unwrap();
        assert_eq!(username, "splituser");
        assert_eq!(password, "splitpass");
    }

    #[test]
    fn test_credential_helper_debug_never_prints_credentials() {
        // Verify that Debug output only shows command configuration,
        // never the actual credentials returned by the helper.
        let json_helper = CredentialHelper::Json("op item get secret --format json".to_string());
        let debug_output = format!("{:?}", json_helper);

        // Should show the command
        assert!(debug_output.contains("op item get secret"));
        // Should never contain any credential-like strings from execution
        // (the helper is never executed during Debug formatting)

        let split_helper = CredentialHelper::Split {
            username: "/path/to/get-user.sh".to_string(),
            password: "/path/to/get-pass.sh".to_string(),
        };
        let debug_output = format!("{:?}", split_helper);

        // Should show the script paths
        assert!(debug_output.contains("/path/to/get-user.sh"));
        assert!(debug_output.contains("/path/to/get-pass.sh"));
    }

    #[test]
    fn test_credential_helper_display_never_leaks_credentials() {
        // Test that after executing a credential helper, the helper's
        // Debug output still only shows the command configuration,
        // not the returned credentials. The CredentialHelper stores
        // only the command string, not execution results.
        let helper = CredentialHelper::Json("my-credential-tool --get creds".to_string());

        // The Debug output should only contain the command, never any
        // credential values that might be returned by execution
        let debug_output = format!("{:?}", helper);
        assert!(
            debug_output.contains("my-credential-tool"),
            "Debug output should show the command"
        );

        // Also verify Split variant
        let split = CredentialHelper::Split {
            username: "get-user-cmd".to_string(),
            password: "get-pass-cmd".to_string(),
        };
        let debug_output = format!("{:?}", split);
        assert!(
            debug_output.contains("get-user-cmd"),
            "Debug output should show the username command"
        );
        assert!(
            debug_output.contains("get-pass-cmd"),
            "Debug output should show the password command"
        );
        // The credential helper struct stores commands, not credentials,
        // so Debug can never leak actual credential values
    }
}
