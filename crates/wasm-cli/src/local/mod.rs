#![allow(clippy::print_stdout)]

use std::path::PathBuf;

use comfy_table::{Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL};
use wasm_package_manager::manager::Manager;

/// Detect and manage local WASM files
#[derive(clap::Parser)]
pub(crate) enum Opts {
    /// List local WASM files in the current directory
    List(ListOpts),
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

impl Opts {
    pub(crate) fn run(self) {
        match self {
            Opts::List(opts) => opts.run(),
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
