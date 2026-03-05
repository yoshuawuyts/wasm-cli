//! `cargo xtask test` — run tests, clippy, and formatting checks.

#![allow(clippy::print_stdout)]

use anyhow::Result;

use crate::{readme, run_command, sql, workspace_root};

pub(crate) fn run_tests() -> Result<()> {
    println!("Running cargo nextest...");
    run_command("cargo", &["nextest", "run", "--all"])?;

    println!("\nRunning doc tests...");
    run_command("cargo", &["test", "--doc", "--all"])?;

    println!("\nRunning cargo clippy...");
    run_command("cargo", &["clippy", "--all", "--", "-D", "warnings"])?;

    println!("\nRunning cargo fmt check...");
    run_command("cargo", &["fmt", "--all", "--", "--check"])?;

    println!("\nRunning sql check...");
    sql::check()?;

    println!("\nRunning readme check...");
    readme::check(&workspace_root()?)?;

    println!("\n✓ All checks passed!");
    Ok(())
}
