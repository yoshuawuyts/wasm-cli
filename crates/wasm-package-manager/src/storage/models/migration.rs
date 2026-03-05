use anyhow::Context;
use rusqlite::Connection;

/// A migration that can be applied to the database.
struct MigrationDef {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

/// All migrations in order. Each migration is run exactly once.
const MIGRATIONS: &[MigrationDef] = &[
    MigrationDef {
        version: 1,
        name: "init",
        sql: include_str!("../migrations/01_init.sql"),
    },
    MigrationDef {
        version: 2,
        name: "rename_wit_interface_to_wit_type",
        sql: include_str!("../migrations/02_rename_wit_interface_to_wit_type.sql"),
    },
];

/// Information about the current migration state.
///
/// # Example
///
/// ```
/// use wasm_package_manager::storage::Migrations;
///
/// let migrations = Migrations { current: 2, total: 3 };
/// assert_eq!(migrations.current, 2);
/// assert_eq!(migrations.total, 3);
/// ```
#[derive(Debug, Clone)]
pub struct Migrations {
    /// The current migration version applied to the database.
    pub current: u32,
    /// The total number of migrations available.
    pub total: u32,
}

impl Migrations {
    /// Initialize the migrations table and run all pending migrations.
    // r[impl db.migrations.create-tables]
    // r[impl db.migrations.idempotent]
    pub(crate) fn run_all(conn: &Connection) -> anyhow::Result<()> {
        // Create the migrations table if it doesn't exist
        conn.execute_batch(include_str!("../migrations/00_migrations.sql"))?;

        // Get the current migration version
        let current_version: u32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Run all migrations that haven't been applied yet
        for migration in MIGRATIONS {
            if migration.version > current_version {
                conn.execute_batch(migration.sql).with_context(|| {
                    format!(
                        "Failed to run migration {}: {}",
                        migration.version, migration.name
                    )
                })?;

                conn.execute(
                    "INSERT INTO migrations (version) VALUES (?1)",
                    [migration.version],
                )?;
            }
        }

        Ok(())
    }

    /// Returns information about the current migration state.
    // r[impl db.migrations.info]
    pub(crate) fn get(conn: &Connection) -> Self {
        let current: u32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let total = MIGRATIONS.last().map_or(0, |m| m.version);
        Self { current, total }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify db.migrations.create-tables]
    #[test]
    fn test_migrations_run_all_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();

        // Verify migrations table exists
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM migrations", [], |row| row.get(0))
            .unwrap();
        assert!(count > 0);

        // Verify OCI layer tables exist
        conn.execute("SELECT 1 FROM oci_repository LIMIT 1", [])
            .unwrap();
        conn.execute("SELECT 1 FROM oci_manifest LIMIT 1", [])
            .unwrap();
        conn.execute("SELECT 1 FROM oci_tag LIMIT 1", []).unwrap();
        conn.execute("SELECT 1 FROM oci_layer LIMIT 1", []).unwrap();

        // Verify WIT layer tables exist
        conn.execute("SELECT 1 FROM wit_package LIMIT 1", [])
            .unwrap();
        conn.execute("SELECT 1 FROM wit_world LIMIT 1", []).unwrap();

        // Verify Wasm layer tables exist
        conn.execute("SELECT 1 FROM wasm_component LIMIT 1", [])
            .unwrap();
        conn.execute("SELECT 1 FROM component_target LIMIT 1", [])
            .unwrap();

        // Verify operational tables exist
        conn.execute("SELECT 1 FROM _sync_meta LIMIT 1", [])
            .unwrap();
    }

    // r[verify db.migrations.idempotent]
    #[test]
    fn test_migrations_run_all_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Run migrations multiple times
        Migrations::run_all(&conn).unwrap();
        Migrations::run_all(&conn).unwrap();
        Migrations::run_all(&conn).unwrap();

        // Should still work correctly
        let info = Migrations::get(&conn);
        assert_eq!(info.current, info.total);
    }

    // r[verify db.migrations.info]
    #[test]
    fn test_migrations_get_info() {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();

        let info = Migrations::get(&conn);

        // Current should equal total after running all migrations
        assert_eq!(info.current, info.total);
        // Total should match the number of migrations defined
        let expected_total = MIGRATIONS.last().map(|m| m.version).unwrap_or(0);
        assert_eq!(info.total, expected_total);
    }

    #[test]
    fn test_migrations_get_before_running() {
        let conn = Connection::open_in_memory().unwrap();

        // Create migrations table manually to test get() on fresh db
        conn.execute_batch(include_str!("../migrations/00_migrations.sql"))
            .unwrap();

        let info = Migrations::get(&conn);

        // Current should be 0 before running migrations
        assert_eq!(info.current, 0);
        // Total should still reflect available migrations
        let expected_total = MIGRATIONS.last().map(|m| m.version).unwrap_or(0);
        assert_eq!(info.total, expected_total);
    }
}
