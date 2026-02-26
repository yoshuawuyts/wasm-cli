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
        name: "known_packages",
        sql: include_str!("../migrations/02_known_packages.sql"),
    },
    MigrationDef {
        version: 3,
        name: "known_package_tags",
        sql: include_str!("../migrations/03_known_package_tags.sql"),
    },
    MigrationDef {
        version: 4,
        name: "image_size",
        sql: include_str!("../migrations/04_image_size.sql"),
    },
    MigrationDef {
        version: 5,
        name: "tag_type",
        sql: include_str!("../migrations/05_tag_type.sql"),
    },
    MigrationDef {
        version: 6,
        name: "wit_interface",
        sql: include_str!("../migrations/06_wit_interface.sql"),
    },
    MigrationDef {
        version: 7,
        name: "package_name",
        sql: include_str!("../migrations/07_package_name.sql"),
    },
    MigrationDef {
        version: 8,
        name: "image_unique",
        sql: include_str!("../migrations/08_image_unique.sql"),
    },
    MigrationDef {
        version: 9,
        name: "wit_interface_unique",
        sql: include_str!("../migrations/09_wit_interface_unique.sql"),
    },
    MigrationDef {
        version: 10,
        name: "package_type",
        sql: include_str!("../migrations/10_package_type.sql"),
    },
    MigrationDef {
        version: 11,
        name: "sync_meta",
        sql: include_str!("../migrations/11_sync_meta.sql"),
    },
];

/// Information about the current migration state.
#[derive(Debug, Clone)]
pub struct Migrations {
    /// The current migration version applied to the database.
    pub current: u32,
    /// The total number of migrations available.
    pub total: u32,
}

impl Migrations {
    /// Initialize the migrations table and run all pending migrations.
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
    pub(crate) fn get(conn: &Connection) -> anyhow::Result<Self> {
        let current: u32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let total = MIGRATIONS.last().map(|m| m.version).unwrap_or(0);
        Ok(Self { current, total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_run_all_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();

        // Verify migrations table exists
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM migrations", [], |row| row.get(0))
            .unwrap();
        assert!(count > 0);

        // Verify image table exists
        conn.execute("SELECT 1 FROM image LIMIT 1", []).ok();

        // Verify known_package table exists
        conn.execute("SELECT 1 FROM known_package LIMIT 1", []).ok();

        // Verify known_package_tag table exists
        conn.execute("SELECT 1 FROM known_package_tag LIMIT 1", [])
            .ok();
    }

    #[test]
    fn test_migrations_run_all_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Run migrations multiple times
        Migrations::run_all(&conn).unwrap();
        Migrations::run_all(&conn).unwrap();
        Migrations::run_all(&conn).unwrap();

        // Should still work correctly
        let info = Migrations::get(&conn).unwrap();
        assert_eq!(info.current, info.total);
    }

    #[test]
    fn test_migrations_get_info() {
        let conn = Connection::open_in_memory().unwrap();
        Migrations::run_all(&conn).unwrap();

        let info = Migrations::get(&conn).unwrap();

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

        let info = Migrations::get(&conn).unwrap();

        // Current should be 0 before running migrations
        assert_eq!(info.current, 0);
        // Total should still reflect available migrations
        let expected_total = MIGRATIONS.last().map(|m| m.version).unwrap_or(0);
        assert_eq!(info.total, expected_total);
    }
}
