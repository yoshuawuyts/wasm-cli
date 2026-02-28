//! `cargo xtask readme` — update or check the README commands section.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

const COMMANDS_START: &str = "<!-- commands-start -->";
const COMMANDS_END: &str = "<!-- commands-end -->";

/// Build the wasm binary and return the path to it.
///
/// If the binary already exists at the expected location it is returned
/// immediately so that, when called from `cargo xtask test`, the binary that
/// was already compiled by `cargo test --all` is re-used rather than
/// triggering a new build with potentially different `RUSTFLAGS`.
fn build_wasm_bin(workspace_root: &Path) -> Result<std::path::PathBuf> {
    let bin_name = format!("wasm{}", std::env::consts::EXE_SUFFIX);
    let bin_path = workspace_root.join("target").join("debug").join(&bin_name);

    if bin_path.exists() {
        return Ok(bin_path);
    }

    // Clear RUSTFLAGS so that CI-specific flags like `-Dwarnings` do not cause
    // this build to fail with platform-specific warnings unrelated to the
    // README check itself.  Warnings are already checked by `cargo clippy`.
    let status = Command::new("cargo")
        .args(["build", "-p", "wasm"])
        .current_dir(workspace_root)
        .env_remove("RUSTFLAGS")
        .status()
        .context("failed to run `cargo build -p wasm`")?;

    if !status.success() {
        anyhow::bail!("`cargo build -p wasm` failed");
    }

    Ok(bin_path)
}

/// Run `wasm --help` and return the output, normalized for cross-platform use.
fn wasm_help(workspace_root: &Path) -> Result<String> {
    let bin = build_wasm_bin(workspace_root)?;
    let output = Command::new(&bin)
        .arg("--help")
        // Disable color so ANSI escape codes never appear in the output.
        .env("NO_COLOR", "1")
        // Remove COLUMNS so clap uses its default width on every platform.
        .env_remove("COLUMNS")
        .output()
        .with_context(|| format!("failed to run `{}`", bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "`wasm --help` exited with {}\nstderr: {}",
            output.status,
            stderr.trim()
        );
    }

    if output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "`wasm --help` produced no stdout output\nstderr: {}",
            stderr.trim()
        );
    }

    let help = String::from_utf8_lossy(&output.stdout).into_owned();
    // Normalize trailing whitespace on every line.  clap may emit trailing
    // spaces on blank separator lines (e.g. `          \n`) on some platforms
    // and omit them on others.  Stripping them keeps the output identical
    // regardless of OS or clap version.
    let help = help
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        + if help.ends_with('\n') { "\n" } else { "" };
    // On Windows the binary is named "wasm.exe", which clap uses in the usage
    // line. Normalize to "wasm" so the README is platform-independent.
    Ok(help.replace("wasm.exe", "wasm"))
}

/// Format the help output as the markdown section content (between markers).
fn format_section(help: &str) -> String {
    format!("\n```\n{}\n```\n", help.trim_end())
}

/// Extract the section content currently in the README (between markers).
fn extract_section(readme: &str) -> Result<String> {
    let start = readme
        .find(COMMANDS_START)
        .context("README is missing the `<!-- commands-start -->` marker")?;
    let end = readme
        .find(COMMANDS_END)
        .context("README is missing the `<!-- commands-end -->` marker")?;

    Ok(readme[start + COMMANDS_START.len()..end].to_owned())
}

/// Replace the section between markers with new content.
fn replace_section(readme: &str, help: &str) -> Result<String> {
    let start = readme
        .find(COMMANDS_START)
        .context("README is missing the `<!-- commands-start -->` marker")?;
    let end = readme
        .find(COMMANDS_END)
        .context("README is missing the `<!-- commands-end -->` marker")?;

    let before = &readme[..start + COMMANDS_START.len()];
    let after = &readme[end..];

    Ok(format!("{}{}{}", before, format_section(help), after))
}

/// Update the README commands section from the current `wasm --help` output.
pub(crate) fn update(workspace_root: &Path) -> Result<()> {
    let help = wasm_help(workspace_root)?;
    let readme_path = workspace_root.join("README.md");
    let readme = std::fs::read_to_string(&readme_path).context("failed to read README.md")?;

    let updated = replace_section(&readme, &help)?;
    std::fs::write(&readme_path, updated).context("failed to write README.md")?;

    println!("✓ README commands section updated");
    Ok(())
}

/// Check that the README commands section matches `wasm --help`.
///
/// This is run as part of `cargo xtask test`. It requires the wasm binary to
/// already be built (e.g. via a prior `cargo test` or `cargo build` invocation).
pub(crate) fn check(workspace_root: &Path) -> Result<()> {
    let help = wasm_help(workspace_root)?;
    let readme_path = workspace_root.join("README.md");
    let readme = std::fs::read_to_string(&readme_path).context("failed to read README.md")?;

    let current = extract_section(&readme)?;
    let expected = format_section(&help);

    // Normalize line endings for cross-platform comparison.
    let current_norm = current.replace("\r\n", "\n");
    let expected_norm = expected.replace("\r\n", "\n");
    if current_norm != expected_norm {
        // Show first differing line for diagnosis.
        let current_lines: Vec<&str> = current_norm.lines().collect();
        let expected_lines: Vec<&str> = expected_norm.lines().collect();
        let diff_line = current_lines
            .iter()
            .zip(expected_lines.iter())
            .enumerate()
            .find(|(_, (a, b))| a != b);
        let note = if let Some((i, (a, b))) = diff_line {
            format!(
                "\nFirst diff at line {}:\n  README:   {a:?}\n  Expected: {b:?}",
                i + 1
            )
        } else {
            // All shared lines matched; the difference is in length.
            format!(
                "\nLine count differs: README {} lines, expected {} lines",
                current_lines.len(),
                expected_lines.len()
            )
        };
        anyhow::bail!(
            "README commands section is out of date.{note}\n\
             Run `cargo xtask readme update` to regenerate it."
        );
    }

    println!("✓ README commands section is up to date");
    Ok(())
}
