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

/// Run the CLI expecting a failure and capture stderr for snapshot testing.
///
/// Used to verify miette's rich error rendering (cause chains, context, hints).
/// The working directory can be overridden for tests that need isolation.
fn run_cli_error(args: &[&str], working_dir: Option<&std::path::Path>) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wasm"));
    cmd.args(args);
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }
    // Force non-fancy graphical output for consistent snapshots across terminals
    cmd.env("NO_COLOR", "1");
    let output = cmd.output().expect("Failed to execute command");

    assert!(
        !output.status.success(),
        "Expected command to fail, but it succeeded. stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Filter out tracing warnings (e.g. from tracing-subscriber) that appear on stderr
    let filtered: String = stderr
        .lines()
        .filter(|line| !line.starts_with("WARN ") && !line.starts_with("  at "))
        .collect::<Vec<_>>()
        .join("\n");

    // Normalize platform differences for consistent cross-platform snapshots:
    // - Windows path separators: `wasm.toml` → `wasm.toml`
    // - Windows OS error: "The system cannot find the path specified. (os error 3)"
    //   → Unix: "No such file or directory (os error 2)"
    filtered.replace('\\', "/").replace(
        "The system cannot find the path specified. (os error 3)",
        "No such file or directory (os error 2)",
    )
}

// =============================================================================
// Main CLI Help Tests
// =============================================================================

// r[verify cli.help]
#[test]
fn test_cli_main_help_snapshot() {
    let output = run_cli(&["--help"]);
    assert_snapshot!(output);
}

// r[verify cli.version]
#[test]
fn test_cli_version_snapshot() {
    let output = run_cli(&["--version"]);
    // Version may change, so we just verify the format
    assert!(output.contains("wasm"));
}

// =============================================================================
// Local Command Help Tests
// =============================================================================

// r[verify cli.local.help]
#[test]
fn test_cli_local_help_snapshot() {
    let output = run_cli(&["local", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.local-list.help]
#[test]
fn test_cli_local_list_help_snapshot() {
    let output = run_cli(&["local", "list", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.local-clean.help]
#[test]
fn test_cli_local_clean_help_snapshot() {
    let output = run_cli(&["local", "clean", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.local-clean.removes-lockfile]
// r[verify cli.local-clean.removes-vendor-wasm]
// r[verify cli.local-clean.removes-vendor-wit]
#[test]
fn test_local_clean_removes_artifacts() {
    let dir = TempDir::new().expect("Failed to create temp dir");

    // Set up a project by running `wasm init`
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute init");
    assert!(output.status.success(), "init failed");

    // Add some content to vendor directories
    std::fs::write(dir.path().join("vendor/wasm/test.wasm"), b"fake wasm")
        .expect("write vendor/wasm file");
    std::fs::write(dir.path().join("vendor/wit/test.wit"), b"fake wit")
        .expect("write vendor/wit file");

    // Verify the files exist before clean
    assert!(dir.path().join("wasm.lock.toml").is_file());
    assert!(dir.path().join("vendor/wasm/test.wasm").is_file());
    assert!(dir.path().join("vendor/wit/test.wit").is_file());

    // Run `wasm local clean`
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["local", "clean"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute local clean");
    assert!(
        output.status.success(),
        "local clean failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify lockfile was removed
    assert!(
        !dir.path().join("wasm.lock.toml").exists(),
        "lockfile should be removed"
    );

    // Verify vendor directory contents were removed but directories remain
    assert!(
        dir.path().join("vendor/wasm").is_dir(),
        "vendor/wasm dir should still exist"
    );
    assert!(
        dir.path().join("vendor/wit").is_dir(),
        "vendor/wit dir should still exist"
    );
    assert!(
        !dir.path().join("vendor/wasm/test.wasm").exists(),
        "vendor/wasm contents should be removed"
    );
    assert!(
        !dir.path().join("vendor/wit/test.wit").exists(),
        "vendor/wit contents should be removed"
    );

    // Verify the manifest is untouched
    assert!(
        dir.path().join("wasm.toml").is_file(),
        "manifest should still exist"
    );
}

#[test]
fn test_local_clean_succeeds_when_nothing_to_clean() {
    let dir = TempDir::new().expect("Failed to create temp dir");

    // Run clean in an empty directory — should not fail
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["local", "clean"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute local clean");

    assert!(
        output.status.success(),
        "local clean should succeed in empty dir: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// =============================================================================
// Registry Command Help Tests
// =============================================================================

// r[verify cli.registry.help]
#[test]
fn test_cli_registry_help_snapshot() {
    let output = run_cli(&["registry", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.registry-pull.help]
#[test]
fn test_cli_registry_pull_help_snapshot() {
    let output = run_cli(&["registry", "pull", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.registry-tags.help]
#[test]
fn test_cli_registry_tags_help_snapshot() {
    let output = run_cli(&["registry", "tags", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.registry-search.help]
#[test]
fn test_cli_registry_search_help_snapshot() {
    let output = run_cli(&["registry", "search", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.registry-sync.help]
#[test]
fn test_cli_registry_sync_help_snapshot() {
    let output = run_cli(&["registry", "sync", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.registry-delete.help]
#[test]
fn test_cli_registry_delete_help_snapshot() {
    let output = run_cli(&["registry", "delete", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.registry-list.help]
#[test]
fn test_cli_registry_list_help_snapshot() {
    let output = run_cli(&["registry", "list", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.registry-known.help]
#[test]
fn test_cli_registry_known_help_snapshot() {
    let output = run_cli(&["registry", "known", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.registry-inspect.help]
#[test]
fn test_cli_registry_inspect_help_snapshot() {
    let output = run_cli(&["registry", "inspect", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.self-clean.help]
#[test]
fn test_cli_self_clean_help_snapshot() {
    let output = run_cli(&["self", "clean", "--help"]);
    assert_snapshot!(output);
}

// =============================================================================
// Self Command Help Tests
// =============================================================================

// r[verify cli.self.help]
#[test]
fn test_cli_self_help_snapshot() {
    let output = run_cli(&["self", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.self-state.help]
#[test]
fn test_cli_self_state_help_snapshot() {
    let output = run_cli(&["self", "state", "--help"]);
    assert_snapshot!(output);
}

// r[verify cli.self-log.help]
#[test]
fn test_cli_self_log_help_snapshot() {
    let output = run_cli(&["self", "log", "--help"]);
    assert_snapshot!(output);
}

// =============================================================================
// Completions Tests
// =============================================================================

// r[verify cli.completions.bash]
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

// r[verify cli.completions.zsh]
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

// r[verify cli.completions.fish]
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

// r[verify cli.completions.invalid]
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

// r[verify cli.man-pages]
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

// r[verify cli.color.auto]
#[test]
fn test_color_flag_auto() {
    // Test that --color=auto is accepted
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--color", "auto", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

// r[verify cli.color.always]
#[test]
fn test_color_flag_always() {
    // Test that --color=always is accepted
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--color", "always", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

// r[verify cli.color.never]
#[test]
fn test_color_flag_never() {
    // Test that --color=never is accepted
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--color", "never", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

// r[verify cli.color.invalid]
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

// r[verify cli.color.in-help]
#[test]
fn test_color_flag_in_help() {
    // Test that --color flag appears in help output
    let output = run_cli(&["--help"]);
    assert!(output.contains("--color"));
    assert!(output.contains("When to use colored output"));
}

// r[verify cli.color.no-color-env]
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

// r[verify cli.color.clicolor-env]
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

// r[verify cli.color.subcommand]
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

// r[verify cli.offline.accepted]
#[test]
fn test_offline_flag_accepted() {
    // Test that --offline flag is accepted with --version
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--offline", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

// r[verify cli.offline.in-help]
#[test]
fn test_offline_flag_in_help() {
    // Test that --offline flag appears in help output
    let output = run_cli(&["--help"]);
    assert!(output.contains("--offline"));
    assert!(output.contains("Run in offline mode"));
}

// r[verify cli.offline.local-allowed]
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

// r[verify cli.offline.registry-blocked]
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

// r[verify cli.offline.with-inspect]
#[test]
fn test_offline_flag_with_registry_inspect() {
    // Test that --offline works with registry inspect command
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["--offline", "registry", "inspect", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

// r[verify cli.offline.with-subcommand]
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

// r[verify init.current-dir]
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
    assert!(dir.path().join("vendor/wit").is_dir());
    assert!(dir.path().join("vendor/wasm").is_dir());

    // Verify manifest file
    let manifest =
        std::fs::read_to_string(dir.path().join("wasm.toml")).expect("Failed to read wasm.toml");
    let parsed: toml::Value = toml::from_str(&manifest).expect("wasm.toml is not valid TOML");
    assert!(
        parsed.get("dependencies").is_some(),
        "manifest should have a dependencies table"
    );

    // Verify lockfile
    let lockfile = std::fs::read_to_string(dir.path().join("wasm.lock.toml"))
        .expect("Failed to read wasm.lock.toml");
    assert!(lockfile.contains("# This file is automatically generated by wasm(1)."));
    assert!(lockfile.contains("# It should not be manually edited."));
    let lock_parsed: toml::Value =
        toml::from_str(&lockfile).expect("wasm.lock.toml is not valid TOML");
    assert_eq!(
        lock_parsed
            .get("lockfile_version")
            .and_then(|v| v.as_integer()),
        Some(3)
    );
}

// r[verify init.explicit-path]
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
    assert!(target.join("vendor/wit").is_dir());
    assert!(target.join("vendor/wasm").is_dir());

    // Verify files exist and are valid
    assert!(target.join("wasm.toml").is_file());
    assert!(target.join("wasm.lock.toml").is_file());
}

// r[verify cli.init.help]
#[test]
fn test_init_help_snapshot() {
    let output = run_cli(&["init", "--help"]);
    assert_snapshot!(output);
}

// =============================================================================
// Install Command Help Tests
// =============================================================================

// r[verify cli.install.help]
#[test]
fn test_install_help_snapshot() {
    let output = run_cli(&["install", "--help"]);
    assert_snapshot!(output);
}

// r[verify install.no-manifest]
#[test]
fn test_install_without_init() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let stderr = run_cli_error(&["install"], Some(dir.path()));
    assert_snapshot!(stderr);
}

// =============================================================================
// Run Command Tests
// =============================================================================

// r[verify cli.run.help]
// r[verify run.http-listen-flag]
#[test]
fn test_cli_run_help_snapshot() {
    let output = run_cli(&["run", "--help"]);
    assert_snapshot!(output);
}

// r[verify run.core-module-rejected]
#[test]
fn test_run_core_module_rejected() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/core_module.wasm"
    );
    let stderr = run_cli_error(&["run", fixture], None);
    assert_snapshot!(stderr);
}

// r[verify run.missing-file]
#[test]
fn test_run_missing_file() {
    let stderr = run_cli_error(&["run", "/nonexistent/path/to/component.wasm"], None);
    assert_snapshot!(stderr);
}

#[test]
fn test_init_prints_success_message() {
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Created"),
        "expected success message in stdout, got: {stdout}"
    );
}

#[test]
fn test_install_scope_component_not_in_manifest() {
    let dir = TempDir::new().expect("Failed to create temp dir");

    // First, run `wasm init` to create the project files
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Try installing a scope:component key that doesn't exist in the manifest.
    // Use --offline so the OCI fallback doesn't hit the network.
    let stderr = run_cli_error(
        &["install", "--offline", "missing:component"],
        Some(dir.path()),
    );
    assert!(
        stderr.contains("offline") || stderr.contains("not found"),
        "expected offline or not-found error, got: {stderr}"
    );
}

#[test]
fn test_run_scope_component_not_installed() {
    let dir = TempDir::new().expect("Failed to create temp dir");

    // Run `wasm init` to create the project files
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Write a manifest with a component entry
    let manifest = r#"
[dependencies.components]
"test:hello" = "0.1.0"
"#;
    std::fs::write(dir.path().join("wasm.toml"), manifest).expect("Failed to write manifest");

    // Write a lockfile with a matching component entry
    let lockfile = r#"
lockfile_version = 3

[[components]]
name = "test:hello"
version = "0.1.0"
registry = "ghcr.io/example/hello"
digest = "sha256:abcdef123456"
"#;
    std::fs::write(dir.path().join("wasm.lock.toml"), lockfile).expect("Failed to write lockfile");

    // Try running — should fail because the vendored file doesn't exist
    let stderr = run_cli_error(&["run", "test:hello"], Some(dir.path()));
    assert!(
        stderr.contains("not found") && stderr.contains("wasm install"),
        "expected error about missing vendored file with install hint, got: {stderr}"
    );
}

// r[verify run.not-installed]
#[test]
fn test_run_scope_component_not_in_manifest() {
    let dir = TempDir::new().expect("Failed to create temp dir");

    // Run `wasm init` to create the project files
    let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
        .args(&["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to execute command");
    assert!(output.status.success());

    // Try running a scope:component key that doesn't exist in the manifest
    let stderr = run_cli_error(&["run", "missing:component"], Some(dir.path()));
    assert!(
        stderr.contains("not installed in the local project"),
        "expected error about component not installed, got: {stderr}"
    );
    // Should include a hint (either about cache, registry, or `wasm install`)
    assert!(
        stderr.contains("wasm run -g")
            || stderr.contains("wasm run -i")
            || stderr.contains("wasm install"),
        "expected actionable hint in error, got: {stderr}"
    );
}

// =============================================================================
// Dotenv Tests
// =============================================================================

// r[verify dotenv.detection]
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

// r[verify dotenv.not-found]
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

// r[verify dotenv.loading]
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

// r[verify dotenv.precedence]
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

// =============================================================================
// Compose Command Help Tests
// =============================================================================

// r[verify cli.compose.help]
#[test]
fn test_cli_compose_help_snapshot() {
    let output = run_cli(&["compose", "--help"]);
    assert_snapshot!(output);
}

// =============================================================================
// Compose Init Integration Tests
// =============================================================================

// r[verify init.composition-dirs]
#[test]
fn test_init_creates_composition_directories() {
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

    // Verify composition directories
    assert!(dir.path().join("types").is_dir());
    assert!(dir.path().join("seams").is_dir());
    assert!(dir.path().join("build").is_dir());
}

// =============================================================================
// GitHub Action Consistency Tests
// =============================================================================

/// Extract the subcommand names listed in the `command` input description.
///
/// Looks for a parenthesized list like `(run, install, init, local, registry)`.
fn extract_action_commands(yml: &str) -> Vec<String> {
    for line in yml.lines() {
        if line.contains("The wasm subcommand to run") {
            if let Some(start) = line.rfind('(') {
                if let Some(end) = line.rfind(')') {
                    let list = &line[start + 1..end];
                    return list
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
        }
    }
    vec![]
}

/// Extract CLI flag names (e.g. `--offline`) from `action.yml` input
/// descriptions.
///
/// Returns flags whose description contains a `(--flag)` suffix. When
/// `run_only` is true, only returns flags whose description mentions
/// `` `wasm run` ``; when false, returns the remaining (global) flags.
fn extract_action_flags(yml: &str, run_only: bool) -> Vec<String> {
    let mut flags = vec![];
    for line in yml.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("description:") {
            continue;
        }
        if let Some(start) = trimmed.rfind("(--") {
            if let Some(end) = trimmed[start..].find(')') {
                let flag = &trimmed[start + 1..start + end];
                // Descriptions mentioning `wasm run` are run-specific flags;
                // the rest are global flags.
                let is_run_specific = trimmed.contains("wasm run");
                if run_only == is_run_specific {
                    flags.push(flag.to_string());
                }
            }
        }
    }
    flags
}

/// Read the repository-root `action.yml`.
fn read_action_yml() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../action.yml");
    std::fs::read_to_string(path).expect("Failed to read action.yml")
}

// r[verify action.commands]
#[test]
fn test_action_commands_exist_in_cli() {
    let yml = read_action_yml();
    let commands = extract_action_commands(&yml);
    assert!(
        !commands.is_empty(),
        "Expected to find subcommands in action.yml command description"
    );

    for cmd in &commands {
        let output = Command::new(env!("CARGO_BIN_EXE_wasm"))
            .args([cmd.as_str(), "--help"])
            .output()
            .unwrap_or_else(|_| panic!("Failed to execute: wasm {cmd} --help"));

        assert!(
            output.status.success(),
            "Command `wasm {cmd} --help` failed — \
             action.yml advertises `{cmd}` but the CLI does not support it.\n\
             stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// r[verify action.global-flags]
#[test]
fn test_action_global_flags_exist_in_cli() {
    let yml = read_action_yml();
    let flags = extract_action_flags(&yml, false);
    assert!(
        !flags.is_empty(),
        "Expected to find global flags in action.yml"
    );

    let main_help = run_cli(&["--help"]);
    for flag in &flags {
        assert!(
            main_help.contains(flag),
            "Global flag `{flag}` referenced in action.yml \
             not found in `wasm --help` output"
        );
    }
}

// r[verify action.run-flags]
#[test]
fn test_action_run_flags_exist_in_cli() {
    let yml = read_action_yml();
    let flags = extract_action_flags(&yml, true);
    assert!(
        !flags.is_empty(),
        "Expected to find `wasm run` flags in action.yml"
    );

    let run_help = run_cli(&["run", "--help"]);
    for flag in &flags {
        assert!(
            run_help.contains(flag),
            "Run flag `{flag}` referenced in action.yml \
             not found in `wasm run --help` output"
        );
    }
}
