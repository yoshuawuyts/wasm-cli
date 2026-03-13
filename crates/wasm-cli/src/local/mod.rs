#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use comfy_table::{Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL};
use wasm_package_manager::manager::Manager;

/// Detect and manage local WASM files
#[derive(clap::Parser)]
pub(crate) enum Opts {
    /// List local WASM files in the current directory
    List(ListOpts),
    /// Remove the lockfile and vendored dependencies
    Clean(CleanOpts),
}

#[derive(clap::Args)]
pub(crate) struct ListOpts {
    /// Directory to search for WASM files (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Include hidden files and directories
    #[arg(long)]
    hidden: bool,

    /// Follow symbolic links
    #[arg(long)]
    follow_links: bool,
}

/// Options for the `local clean` command.
#[derive(clap::Args)]
pub(crate) struct CleanOpts {
    /// Directory to clean (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,
}

impl Opts {
    pub(crate) fn run(self) {
        match self {
            Opts::List(opts) => opts.run(),
            Opts::Clean(opts) => opts.run(),
        }
    }
}

impl ListOpts {
    fn run(&self) {
        let mut wasm_files = Manager::detect_local_wasm(&self.path, self.hidden, self.follow_links);

        if wasm_files.is_empty() {
            println!("No WASM files found in {}", self.path.display());
            return;
        }

        // Sort by path for consistent output
        wasm_files.sort_by(|a, b| a.path().cmp(b.path()));

        // Create a table for nice output
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec!["#", "File Path"]);

        for (idx, entry) in wasm_files.iter().enumerate() {
            table.add_row(vec![
                format!("{}", idx + 1),
                entry.path().display().to_string(),
            ]);
        }

        println!("{table}");
        println!("\nFound {} WASM file(s)", wasm_files.len());
    }
}

impl CleanOpts {
    fn run(&self) {
        let lockfile = self.path.join("wasm.lock.toml");
        let vendor_wasm = self.path.join("vendor/wasm");
        let vendor_wit = self.path.join("vendor/wit");

        remove_file(&lockfile);
        remove_dir_contents(&vendor_wasm);
        remove_dir_contents(&vendor_wit);

        println!(
            "{:>12} local build artifacts",
            console::style("Cleaned").green().bold(),
        );
    }
}

/// Remove a file if it exists, printing what was removed.
fn remove_file(path: &std::path::Path) {
    match std::fs::remove_file(path) {
        Ok(()) => println!(
            "{:>12} {}",
            console::style("Removed").red().bold(),
            path.display()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!(
            "{:>12} failed to remove {}: {e}",
            console::style("Warning").yellow().bold(),
            path.display()
        ),
    }
}

/// Remove all contents of a directory (but keep the directory itself).
fn remove_dir_contents(dir: &std::path::Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            eprintln!(
                "{:>12} failed to read {}: {e}",
                console::style("Warning").yellow().bold(),
                dir.display()
            );
            return;
        }
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        match result {
            Ok(()) => println!(
                "{:>12} {}",
                console::style("Removed").red().bold(),
                path.display()
            ),
            Err(e) => eprintln!(
                "{:>12} failed to remove {}: {e}",
                console::style("Warning").yellow().bold(),
                path.display()
            ),
        }
    }
}
