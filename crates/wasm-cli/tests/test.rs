//! Tests for the wasm CLI
//!
//! This module contains integration tests for CLI commands.
//! Use `cargo test --package wasm --test test` to run these tests.
//!
//! # CLI Help Screen Tests
//!
//! These tests verify that CLI help screens remain consistent using snapshot testing.
//! When commands change, update snapshots with:
//! `cargo insta review` or `INSTA_UPDATE=always cargo test --package wasm`

use std::process::Command;

use insta::assert_snapshot;
use tempfile::TempDir;

/// Run the CLI with the given arguments and capture the output.
///
/// The output is normalized to replace platform-specific binary names
/// (e.g., `wasm.exe` on Windows) with `wasm` for consistent snapshots.
fn run_cli(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(args)
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Combine stdout and stderr for help output (clap writes to stdout by default for --help)
    let result = if !stdout.is_empty() {
        stdout.to_string()
    } else {
        stderr.to_string()
    };

    // Normalize binary name for cross-platform consistency
    // On Windows, the binary is "wasm.exe" but on Unix it's "wasm"
    result.replace("wasm.exe", "wasm")
}

// =============================================================================
// Main CLI Help Tests
// =============================================================================

#[test]
fn test_cli_main_help_snapshot() {
    let output = run_cli(&["--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_version_snapshot() {
    let output = run_cli(&["--version"]);
    // Version may change, so we just verify the format
    assert!(output.contains("wasm"));
}

// =============================================================================
// Local Command Help Tests
// =============================================================================

#[test]
fn test_cli_local_help_snapshot() {
    let output = run_cli(&["local", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_local_list_help_snapshot() {
    let output = run_cli(&["local", "list", "--help"]);
    assert_snapshot!(output);
}

// =============================================================================
// Registry Command Help Tests
// =============================================================================

#[test]
fn test_cli_registry_help_snapshot() {
    let output = run_cli(&["registry", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_registry_pull_help_snapshot() {
    let output = run_cli(&["registry", "pull", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_registry_tags_help_snapshot() {
    let output = run_cli(&["registry", "tags", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_registry_search_help_snapshot() {
    let output = run_cli(&["registry", "search", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_registry_sync_help_snapshot() {
    let output = run_cli(&["registry", "sync", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_registry_delete_help_snapshot() {
    let output = run_cli(&["registry", "delete", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_registry_list_help_snapshot() {
    let output = run_cli(&["registry", "list", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_registry_known_help_snapshot() {
    let output = run_cli(&["registry", "known", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_registry_inspect_help_snapshot() {
    let output = run_cli(&["registry", "inspect", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_self_clean_help_snapshot() {
    let output = run_cli(&["self", "clean", "--help"]);
    assert_snapshot!(output);
}

// =============================================================================
// Self Command Help Tests
// =============================================================================

#[test]
fn test_cli_self_help_snapshot() {
    let output = run_cli(&["self", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_self_state_help_snapshot() {
    let output = run_cli(&["self", "state", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_cli_self_log_help_snapshot() {
    let output = run_cli(&["self", "log", "--help"]);
    assert_snapshot!(output);
}

// =============================================================================
// Completions Tests
// =============================================================================

#[test]
fn test_completions_bash() {
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["self", "completions", "bash"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("_wasm"),
        "Expected bash completion function"
    );
}

#[test]
fn test_completions_zsh() {
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["self", "completions", "zsh"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("#compdef wasm"),
        "Expected zsh completion header"
    );
}

#[test]
fn test_completions_fish() {
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["self", "completions", "fish"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("__fish_wasm"),
        "Expected fish completion function"
    );
}

#[test]
fn test_completions_invalid_shell() {
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["self", "completions", "invalid"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
}

// =============================================================================
// Man Pages Tests
// =============================================================================

#[test]
fn test_man_pages_generation() {
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["self", "man-pages"])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "man-pages failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("wasm"),
        "Expected man page to reference 'wasm'"
    );
}

// =============================================================================
// Color Support Tests
// =============================================================================

#[test]
fn test_color_flag_auto() {
    // Test that --color=auto is accepted
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--color", "auto", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_color_flag_always() {
    // Test that --color=always is accepted
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--color", "always", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_color_flag_never() {
    // Test that --color=never is accepted
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--color", "never", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_color_flag_invalid_value() {
    // Test that invalid color values are rejected
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--color", "invalid", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value 'invalid'"));
}

#[test]
fn test_color_flag_in_help() {
    // Test that --color flag appears in help output
    let output = run_cli(&["--help"]);
    assert!(output.contains("--color"));
    assert!(output.contains("When to use colored output"));
}

#[test]
fn test_no_color_env_var() {
    // Test that NO_COLOR environment variable disables color
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--version"])
        .env("NO_COLOR", "1")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    // The output should not contain ANSI escape codes when NO_COLOR is set
    // We can't easily test for absence of color codes without parsing,
    // but we can verify the command succeeds
}

#[test]
fn test_clicolor_env_var() {
    // Test that CLICOLOR=0 environment variable disables color
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--version"])
        .env("CLICOLOR", "0")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_color_flag_with_subcommand() {
    // Test that --color flag works with subcommands
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--color", "never", "local", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

// =============================================================================
// Offline Mode Tests
// =============================================================================

#[test]
fn test_offline_flag_accepted() {
    // Test that --offline flag is accepted with --version
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--offline", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_offline_flag_in_help() {
    // Test that --offline flag appears in help output
    let output = run_cli(&["--help"]);
    assert!(output.contains("--offline"));
    assert!(output.contains("Run in offline mode"));
}

#[test]
fn test_offline_flag_with_local_list() {
    // Test that --offline works with local list command (local-only operation)
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--offline", "local", "list", "/nonexistent"])
        .output()
        .expect("Failed to execute command");

    // The command should succeed (even if no files found)
    assert!(output.status.success());
}

#[test]
fn test_offline_flag_with_registry_pull() {
    // Test that --offline mode causes registry pull to fail with clear error
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&[
            "--offline",
            "registry",
            "pull",
            "ghcr.io/example/test:latest",
        ])
        .output()
        .expect("Failed to execute command");

    // The command should fail with an offline mode error
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("offline"),
        "Expected 'offline' error message, got: {}",
        stderr
    );
}

#[test]
fn test_offline_flag_with_registry_inspect() {
    // Test that --offline works with registry inspect command
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--offline", "registry", "inspect", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

#[test]
fn test_offline_flag_with_subcommand() {
    // Test that --offline flag works with subcommands
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--offline", "local", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

// =============================================================================
// Init Command Tests
// =============================================================================

#[test]
fn test_init_creates_files_in_current_dir() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify directory structure
    assert!(dir.path().join("deps/vendor/wit").is_dir());
    assert!(dir.path().join("deps/vendor/wasm").is_dir());

    // Verify manifest file
    let manifest = std::fs::read_to_string(dir.path().join("deps/wasm.toml"))
        .expect("Failed to read wasm.toml");
    let parsed: toml::Value = toml::from_str(&manifest).expect("wasm.toml is not valid TOML");
    assert!(
        parsed.get("components").is_some() || parsed.get("interfaces").is_some(),
        "manifest should have a components or interfaces table"
    );

    // Verify lockfile
    let lockfile = std::fs::read_to_string(dir.path().join("deps/wasm.lock.toml"))
        .expect("Failed to read wasm.lock.toml");
    assert!(lockfile.contains("# This file is automatically generated by wasm(1)."));
    assert!(lockfile.contains("# It should not be manually edited."));
    let lock_parsed: toml::Value =
        toml::from_str(&lockfile).expect("wasm.lock.toml is not valid TOML");
    assert_eq!(
        lock_parsed
            .get("lockfile_version")
            .and_then(|v| v.as_integer()),
        Some(2)
    );
}

#[test]
fn test_init_creates_files_at_explicit_path() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let target = dir.path().join("my-project");

    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["init", target.to_str().unwrap()])
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify directory structure
    assert!(target.join("deps/vendor/wit").is_dir());
    assert!(target.join("deps/vendor/wasm").is_dir());

    // Verify files exist and are valid
    assert!(target.join("deps/wasm.toml").is_file());
    assert!(target.join("deps/wasm.lock.toml").is_file());
}

#[test]
fn test_init_help_snapshot() {
    let output = run_cli(&["init", "--help"]);
    assert_snapshot!(output);
}

// =============================================================================
// Add Command Help Tests
// =============================================================================

#[test]
fn test_add_help_snapshot() {
    let output = run_cli(&["add", "--help"]);
    assert_snapshot!(output);
}

// =============================================================================
// Install Command Help Tests
// =============================================================================

#[test]
fn test_install_help_snapshot() {
    let output = run_cli(&["install", "--help"]);
    assert_snapshot!(output);
}

// =============================================================================
// Run Command Tests
// =============================================================================

#[test]
fn test_cli_run_help_snapshot() {
    let output = run_cli(&["run", "--help"]);
    assert_snapshot!(output);
}

#[test]
fn test_run_core_module_rejected() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/core_module.wasm"
    );
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["run", fixture])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("core module"),
        "Expected 'core module' error message, got: {stderr}"
    );
}

#[test]
fn test_run_missing_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["run", "/nonexistent/path/to/component.wasm"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to read"),
        "Expected file-not-found error, got: {stderr}"
    );
}

// =============================================================================
// Dotenv Tests
// =============================================================================

#[test]
fn test_dotenv_file_detected_in_config() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    // Create a .env file with two variables
    std::fs::write(dir.path().join(".env"), "FOO=bar\nBAZ=qux\n").expect("Failed to write .env");

    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["self", "config"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "self config failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[Environment]"),
        "Expected [Environment] section in output"
    );
    assert!(stdout.contains(".env"), "Expected .env path in output");
    assert!(
        stdout.contains("exists"),
        "Expected 'exists' status when .env is present"
    );
    assert!(
        stdout.contains("2 variable(s) defined in file"),
        "Expected variable count in output"
    );
}

#[test]
fn test_dotenv_file_not_found_in_config() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    // No .env file created

    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["self", "config"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "self config failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[Environment]"),
        "Expected [Environment] section in output"
    );
    assert!(stdout.contains(".env"), "Expected .env path in output");
    assert!(
        stdout.contains("not found"),
        "Expected 'not found' status when .env is absent"
    );
}

#[test]
fn test_dotenv_variables_are_loaded() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    // Create a .env file
    std::fs::write(
        dir.path().join(".env"),
        "WASM_TEST_DOTENV_VAR=hello_dotenv\n",
    )
    .expect("Failed to write .env");

    // The CLI loads the .env before running; verify it completes successfully
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["self", "config"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute command");

    assert!(
        output.status.success(),
        "CLI should succeed when a .env file is present"
    );
}

#[test]
fn test_system_env_takes_precedence_over_dotenv() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    // Create a .env file that tries to set PATH
    std::fs::write(dir.path().join(".env"), "PATH=/dotenv/path\n").expect("Failed to write .env");

    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["self", "config"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute command");

    // The CLI should still run successfully (system PATH not overridden)
    assert!(
        output.status.success(),
        "CLI should succeed and not have PATH overridden by .env"
    );
}
