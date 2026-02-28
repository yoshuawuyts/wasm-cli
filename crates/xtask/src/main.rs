//! xtask - Build automation and task orchestration for the wasm project
//!
//! This binary provides a unified interface for running common development tasks
//! like testing, linting, and formatting checks.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::Connection;

/// Path from the workspace root to the migrations directory.
const MIGRATIONS_DIR: &str = "crates/package-manager/src/storage/migrations";

/// Path from the workspace root to the schema file.
const SCHEMA_PATH: &str = "crates/package-manager/src/storage/schema.sql";

/// Path from the workspace root to migration.rs.
const MIGRATION_RS_PATH: &str = "crates/package-manager/src/storage/models/migration.rs";

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Build automation and task orchestration")]
enum Xtask {
    /// Run tests, clippy, and formatting checks
    Test,
    /// Database schema and migration management
    Sql {
        #[command(subcommand)]
        command: SqlCommand,
    },
}

/// Subcommands for `cargo xtask sql`.
#[derive(Subcommand)]
enum SqlCommand {
    /// Generate a new migration by diffing schema.sql against existing migrations
    Migrate {
        /// Name for the new migration (e.g. "add_oci_tables")
        #[arg(long)]
        name: String,
    },
    /// Check that schema.sql is in sync with existing migrations (CI gate)
    Check,
    /// Install sqlite3def for the current platform
    Install,
}

fn main() -> Result<()> {
    let xtask = Xtask::parse();

    match xtask {
        Xtask::Test => run_tests()?,
        Xtask::Sql { command } => match command {
            SqlCommand::Migrate { name } => sql_migrate(&name)?,
            SqlCommand::Check => sql_check()?,
            SqlCommand::Install => sql_install()?,
        },
    }

    Ok(())
}

fn run_tests() -> Result<()> {
    println!("Running cargo test...");
    run_command("cargo", &["test", "--all"])?;

    println!("\nRunning cargo clippy...");
    run_command("cargo", &["clippy", "--all", "--", "-D", "warnings"])?;

    println!("\nRunning cargo fmt check...");
    run_command("cargo", &["fmt", "--all", "--", "--check"])?;

    println!("\nRunning sql check...");
    sql_check()?;

    println!("\n✓ All checks passed!");
    Ok(())
}

fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd).args(args).status()?;

    if !status.success() {
        anyhow::bail!("{} failed with exit code: {:?}", cmd, status.code());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// SQL subcommand helpers
// ---------------------------------------------------------------------------

/// Return the workspace root directory (the directory containing the root `Cargo.toml`).
fn workspace_root() -> Result<PathBuf> {
    // xtask is invoked via `cargo xtask` which sets CARGO_MANIFEST_DIR for the
    // xtask crate. Walk up to the workspace root (two levels: crates/xtask).
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Fallback: assume CWD is the workspace root.
            std::env::current_dir()
                .expect("failed to determine current directory; ensure xtask is run from the workspace root")
        });

    // If we're inside crates/xtask, go up two levels.
    if manifest_dir.ends_with("crates/xtask") {
        Ok(manifest_dir
            .parent()
            .and_then(|p| p.parent())
            .expect(
                "invalid workspace structure: expected crates/xtask to have two parent directories",
            )
            .to_path_buf())
    } else {
        Ok(manifest_dir)
    }
}

/// Sorted list of numbered migration SQL files (excluding 00_migrations.sql).
fn numbered_migrations(migrations_dir: &Path) -> Result<Vec<(u32, String, PathBuf)>> {
    let mut entries: Vec<(u32, String, PathBuf)> = Vec::new();

    for entry in fs::read_dir(migrations_dir).context("reading migrations directory")? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.ends_with(".sql") {
            continue;
        }
        // Parse NN_name.sql
        let Some((num_str, rest)) = file_name.split_once('_') else {
            continue;
        };
        let Ok(num) = num_str.parse::<u32>() else {
            continue;
        };
        if num == 0 {
            // Skip 00_migrations.sql — it's applied separately.
            continue;
        }
        let name = rest.trim_end_matches(".sql").to_string();
        entries.push((num, name, entry.path()));
    }

    entries.sort_by_key(|(num, _, _)| *num);
    Ok(entries)
}

/// Create a temporary SQLite database and apply all existing migrations in order.
/// Then create a *second* temp DB with the schema normalized for sqlite3def
/// (replaces `datetime('now')` with `CURRENT_TIMESTAMP` and strips SQL comments).
/// Returns the clean temp DB file (kept alive by the `NamedTempFile`).
fn build_clean_migrations_db(migrations_dir: &Path) -> Result<tempfile::NamedTempFile> {
    // Step 1: Replay migrations into a temp DB.
    let tmp1 = tempfile::NamedTempFile::new().context("creating temp database file")?;
    let conn =
        Connection::open(tmp1.path()).context("opening temp database for migration replay")?;

    let init_sql = fs::read_to_string(migrations_dir.join("00_migrations.sql"))
        .context("reading 00_migrations.sql")?;
    conn.execute_batch(&init_sql)
        .context("applying 00_migrations.sql")?;

    for (num, name, path) in numbered_migrations(migrations_dir)? {
        let sql = fs::read_to_string(&path)
            .with_context(|| format!("reading migration {num}_{name}.sql"))?;
        conn.execute_batch(&sql)
            .with_context(|| format!("applying migration {num}_{name}.sql"))?;
    }

    // Step 2: Extract DDL from sqlite_master and normalize it.
    let mut stmt = conn
        .prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY rowid")
        .context("querying sqlite_master")?;
    let ddl_rows: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .context("reading sqlite_master rows")?
        .filter_map(|r| r.ok())
        .collect();

    drop(stmt);
    drop(conn);

    let mut normalized = String::new();
    for ddl in &ddl_rows {
        let fixed = normalize_ddl(ddl);
        normalized.push_str(&fixed);
        normalized.push_str(";\n");
    }

    // Step 3: Create a clean temp DB from the normalized DDL.
    let tmp2 = tempfile::NamedTempFile::new().context("creating clean temp database")?;
    let conn2 = Connection::open(tmp2.path()).context("opening clean temp database")?;
    conn2
        .execute_batch(&normalized)
        .context("applying normalized schema to clean temp database")?;
    drop(conn2);

    Ok(tmp2)
}

/// Normalize a DDL statement so sqlite3def can parse it.
///
/// - Replaces `DEFAULT (datetime('now'))` with `DEFAULT CURRENT_TIMESTAMP`.
/// - Strips SQL comments (`--` to end of line).
/// - Quotes SQL reserved words used as column names (e.g. `key`, `value`).
fn normalize_ddl(ddl: &str) -> String {
    let mut out = String::with_capacity(ddl.len());
    for line in ddl.lines() {
        // Strip inline `-- ...` comments.
        let line = if let Some(idx) = line.find("--") {
            line.get(..idx).unwrap_or(line)
        } else {
            line
        };
        let line = line.trim_end();
        if !line.is_empty() {
            out.push_str(line);
            out.push('\n');
        }
    }
    // Replace datetime('now') variants.
    let out = out.replace("DEFAULT (datetime('now'))", "DEFAULT CURRENT_TIMESTAMP");

    // Quote reserved SQL words used as column identifiers.
    // sqlite3def's parser requires backtick-quoting for these.
    quote_reserved_column_names(&out)
}

/// Quote known SQL reserved words that appear as column names in CREATE TABLE
/// statements. This handles the pattern of a bare word at the start of a column
/// definition (after leading whitespace and a comma or opening paren).
fn quote_reserved_column_names(ddl: &str) -> String {
    // Reserved words that sqlite3def's parser rejects as bare column names.
    const RESERVED: &[&str] = &["key", "value"];

    let mut out = String::with_capacity(ddl.len());
    for line in ddl.lines() {
        let trimmed = line.trim();
        let mut replaced = false;
        for &word in RESERVED {
            // Match lines like "    key TEXT ..." or "key TEXT ..." (column defs)
            if trimmed.starts_with(word)
                && trimmed
                    .get(word.len()..word.len() + 1)
                    .is_some_and(|c| c == " " || c == "\t")
            {
                let leading: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                let rest = trimmed.get(word.len()..).unwrap_or("");
                out.push_str(&leading);
                out.push('`');
                out.push_str(word);
                out.push('`');
                out.push_str(rest);
                out.push('\n');
                replaced = true;
                break;
            }
        }
        if !replaced {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Locate the `sqlite3def` binary, checking `target/tools/` first, then PATH.
fn find_sqlite3def(root: &Path) -> PathBuf {
    let binary_name = if cfg!(windows) {
        "sqlite3def.exe"
    } else {
        "sqlite3def"
    };
    let local_path = root.join("target").join("tools").join(binary_name);
    if local_path.exists() {
        local_path
    } else {
        PathBuf::from(binary_name) // Fall back to PATH
    }
}

/// `cargo xtask sql install` — download sqlite3def for the current platform.
fn sql_install() -> Result<()> {
    let root = workspace_root()?;
    let tools_dir = root.join("target").join("tools");
    fs::create_dir_all(&tools_dir).context("creating target/tools directory")?;

    let (os_name, arch_name, ext) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("linux", "amd64", "tar.gz"),
        ("macos", "x86_64") => ("darwin", "amd64", "zip"),
        ("macos", "aarch64") => ("darwin", "arm64", "zip"),
        ("windows", "x86_64") => ("windows", "amd64", "zip"),
        (os, arch) => anyhow::bail!("unsupported platform: {os}/{arch}"),
    };

    let version = "v3.9.8";
    let url = format!(
        "https://github.com/sqldef/sqldef/releases/download/{version}/sqlite3def_{os_name}_{arch_name}.{ext}"
    );
    let archive_path = tools_dir.join(format!("sqlite3def.{ext}"));

    println!("Downloading sqlite3def from {url}...");

    // Download with curl.
    let status = Command::new("curl")
        .args(["-sfL", "-o"])
        .arg(&archive_path)
        .arg(&url)
        .status()
        .context("failed to run curl — is it installed?")?;
    if !status.success() {
        anyhow::bail!("curl failed to download sqlite3def (HTTP error or network failure)");
    }

    // Extract the archive.
    let status = if ext == "tar.gz" {
        Command::new("tar")
            .args(["xzf"])
            .arg(&archive_path)
            .arg("-C")
            .arg(&tools_dir)
            .status()
            .context("failed to run tar")?
    } else {
        // .zip — use tar on Windows (available since Windows 10 1803).
        Command::new("tar")
            .args(["-xf"])
            .arg(&archive_path)
            .arg("-C")
            .arg(&tools_dir)
            .status()
            .context("failed to extract zip archive")?
    };
    if !status.success() {
        anyhow::bail!("failed to extract sqlite3def archive");
    }

    // Clean up the archive file.
    let _ = fs::remove_file(&archive_path);

    let binary_name = if cfg!(windows) {
        "sqlite3def.exe"
    } else {
        "sqlite3def"
    };
    let installed_path = tools_dir.join(binary_name);

    if installed_path.exists() {
        println!(
            "✓ Installed sqlite3def to {}",
            installed_path
                .strip_prefix(&root)
                .unwrap_or(&installed_path)
                .display()
        );
    } else {
        anyhow::bail!(
            "installation failed: {} not found after extraction",
            installed_path.display()
        );
    }

    Ok(())
}

/// Run `sqlite3def <db_path> --dry-run < schema.sql` and return the diff output.
///
/// Returns an empty string when no changes are needed.
fn run_sqlite3def_diff(sqlite3def: &Path, db_path: &Path, schema_sql: &str) -> Result<String> {
    let output = Command::new(sqlite3def)
        .arg(db_path)
        .arg("--dry-run")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(schema_sql.as_bytes())?;
            }
            child.wait_with_output()
        })
        .context(
            "failed to run sqlite3def. Is it installed? \
             Run `cargo xtask sql install` to install it.",
        )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("sqlite3def failed:\n{stderr}");
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // sqlite3def outputs "-- Nothing is modified --" when there are no changes,
    // or "-- dry run --" followed by the diff. Filter these markers.
    if raw.contains("-- Nothing is modified --") {
        return Ok(String::new());
    }

    // Strip the "-- dry run --" marker and BEGIN/COMMIT wrappers added by --dry-run.
    let diff: String = raw
        .lines()
        .filter(|l| {
            let t = l.trim();
            t != "-- dry run --" && t != "BEGIN;" && t != "COMMIT;"
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    Ok(diff)
}

/// Generate the expected contents of `migration.rs` from the current set of numbered
/// migration files. Returns the full file content as a `String`.
fn generate_migration_rs_content(root: &Path) -> Result<String> {
    let migrations_dir = root.join(MIGRATIONS_DIR);
    let migration_rs = root.join(MIGRATION_RS_PATH);
    let entries = numbered_migrations(&migrations_dir)?;

    let mut buf = String::new();

    // Header
    buf.push_str("use anyhow::Context;\n");
    buf.push_str("use rusqlite::Connection;\n");
    buf.push('\n');
    buf.push_str("/// A migration that can be applied to the database.\n");
    buf.push_str("struct MigrationDef {\n");
    buf.push_str("    version: u32,\n");
    buf.push_str("    name: &'static str,\n");
    buf.push_str("    sql: &'static str,\n");
    buf.push_str("}\n");
    buf.push('\n');
    buf.push_str("/// All migrations in order. Each migration is run exactly once.\n");
    buf.push_str("const MIGRATIONS: &[MigrationDef] = &[\n");

    for (num, name, _) in &entries {
        writeln!(buf, "    MigrationDef {{").expect("write to String");
        writeln!(buf, "        version: {num},").expect("write to String");
        writeln!(buf, "        name: \"{name}\",").expect("write to String");
        writeln!(
            buf,
            "        sql: include_str!(\"../migrations/{num:02}_{name}.sql\"),"
        )
        .expect("write to String");
        writeln!(buf, "    }},").expect("write to String");
    }

    buf.push_str("];\n");
    buf.push('\n');

    // Append the rest of the file (Migrations struct + impl + tests) — read it from
    // the existing file so we only regenerate the const array.
    // Normalize line endings so the output is always LF regardless of platform.
    let existing = fs::read_to_string(&migration_rs)
        .context("reading migration.rs")?
        .replace("\r\n", "\n");

    // Find the marker after the const array: the `/// Information about...` doc comment.
    let marker = "/// Information about the current migration state.";
    let rest = existing
        .find(marker)
        .map(|idx| &existing[idx..])
        .context("could not find Migrations struct marker in migration.rs")?;

    buf.push_str(rest);

    Ok(buf)
}

/// Regenerate `migration.rs` from the current set of numbered migration files.
fn regenerate_migration_rs(root: &Path) -> Result<()> {
    let migration_rs = root.join(MIGRATION_RS_PATH);
    let content = generate_migration_rs_content(root)?;
    fs::write(&migration_rs, content).context("writing migration.rs")?;
    Ok(())
}

/// `cargo xtask sql migrate --name <name>`
fn sql_migrate(name: &str) -> Result<()> {
    // Validate the migration name: only alphanumeric characters and underscores allowed.
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        anyhow::bail!(
            "invalid migration name '{name}': only ASCII alphanumeric characters and underscores are allowed"
        );
    }

    let root = workspace_root()?;
    let migrations_dir = root.join(MIGRATIONS_DIR);
    let schema_path = root.join(SCHEMA_PATH);
    let sqlite3def = find_sqlite3def(&root);

    let schema_sql = fs::read_to_string(&schema_path).context("reading schema.sql")?;

    // 1. Replay existing migrations into a clean temp database.
    let clean_db = build_clean_migrations_db(&migrations_dir)?;

    // 2. Diff via sqlite3def --dry-run.
    let diff = run_sqlite3def_diff(&sqlite3def, clean_db.path(), &schema_sql)?;

    if diff.is_empty() {
        println!("schema.sql is already in sync with migrations — nothing to generate.");
        return Ok(());
    }

    // 3. Determine next migration number.
    let entries = numbered_migrations(&migrations_dir)?;
    let next_num = entries.last().map_or(1, |(n, _, _)| n + 1);

    // 4. Write the new migration file.
    let migration_file = migrations_dir.join(format!("{next_num:02}_{name}.sql"));
    fs::write(&migration_file, format!("{diff}\n"))
        .with_context(|| format!("writing {}", migration_file.display()))?;
    println!(
        "Created migration: {}",
        migration_file
            .strip_prefix(&root)
            .unwrap_or(&migration_file)
            .display()
    );

    // 5. Regenerate migration.rs.
    regenerate_migration_rs(&root)?;
    println!("Regenerated {MIGRATION_RS_PATH}");

    Ok(())
}

/// `cargo xtask sql check`
fn sql_check() -> Result<()> {
    let root = workspace_root()?;
    let migrations_dir = root.join(MIGRATIONS_DIR);
    let schema_path = root.join(SCHEMA_PATH);
    let sqlite3def = find_sqlite3def(&root);

    let schema_sql = fs::read_to_string(&schema_path).context("reading schema.sql")?;

    // 1. Replay existing migrations into a clean temp database.
    let clean_db = build_clean_migrations_db(&migrations_dir)?;

    // 2. Diff via sqlite3def --dry-run.
    let diff = run_sqlite3def_diff(&sqlite3def, clean_db.path(), &schema_sql)?;

    if !diff.is_empty() {
        eprintln!("schema.sql has changes not captured in migrations:\n");
        eprintln!("{diff}\n");
        anyhow::bail!(
            "schema.sql is out of sync. Run `cargo xtask sql migrate --name <description>` to generate a migration."
        );
    }

    // 3. Verify migration.rs matches the current set of migration files.
    // Normalize line endings so the check works on Windows (where git may check
    // out files with \r\n) as well as Unix.
    let migration_rs = root.join(MIGRATION_RS_PATH);
    let existing = fs::read_to_string(&migration_rs)
        .context("reading migration.rs")?
        .replace("\r\n", "\n");
    let expected = generate_migration_rs_content(&root)?;

    if existing != expected {
        anyhow::bail!(
            "migration.rs is out of date. Run `cargo xtask sql migrate --name <description>` to regenerate it."
        );
    }

    println!("✓ schema.sql and migration.rs are in sync with migrations.");
    Ok(())
}
