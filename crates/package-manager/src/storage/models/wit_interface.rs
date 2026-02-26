use rusqlite::Connection;

/// A WIT interface extracted from a WebAssembly component.
#[derive(Debug, Clone)]
pub struct WitInterface {
    id: i64,
    /// The package name (e.g., "wasi:http@0.2.0")
    pub package_name: Option<String>,
    /// The full WIT text representation
    pub wit_text: String,
    /// The world name if available
    pub world_name: Option<String>,
    /// Number of imports
    pub import_count: i32,
    /// Number of exports
    pub export_count: i32,
    /// When this was created
    pub created_at: String,
}

impl WitInterface {
    /// Returns the ID of this WIT interface.
    #[must_use]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Create a new WitInterface for testing purposes
    #[must_use]
    pub fn new_for_testing(
        id: i64,
        package_name: Option<String>,
        wit_text: String,
        world_name: Option<String>,
        import_count: i32,
        export_count: i32,
        created_at: String,
    ) -> Self {
        Self {
            id,
            package_name,
            wit_text,
            world_name,
            import_count,
            export_count,
            created_at,
        }
    }

    /// Insert a new WIT interface and return its ID.
    /// Uses atomic `INSERT ... ON CONFLICT` for content-addressable storage.
    /// If the same WIT text already exists, returns existing ID.
    pub(crate) fn insert(
        conn: &Connection,
        wit_text: &str,
        package_name: Option<&str>,
        world_name: Option<&str>,
        import_count: i32,
        export_count: i32,
    ) -> anyhow::Result<i64> {
        // Use atomic upsert to prevent race conditions
        // First try to insert, on conflict (same wit_text) do nothing
        conn.execute(
            "INSERT INTO wit_interface (wit_text, package_name, world_name, import_count, export_count) 
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(wit_text) DO NOTHING",
            (wit_text, package_name, world_name, import_count, export_count),
        )?;

        // Now retrieve the ID (either newly inserted or existing)
        let id: i64 = conn.query_row(
            "SELECT id FROM wit_interface WHERE wit_text = ?1",
            [wit_text],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    /// Link an image to a WIT interface.
    pub(crate) fn link_to_image(
        conn: &Connection,
        image_id: i64,
        wit_interface_id: i64,
    ) -> anyhow::Result<()> {
        conn.execute(
            "INSERT OR IGNORE INTO image_wit_interface (image_id, wit_interface_id) VALUES (?1, ?2)",
            (image_id, wit_interface_id),
        )?;
        Ok(())
    }

    /// Get WIT interface for an image by image ID.
    #[allow(dead_code)]
    pub(crate) fn get_for_image(conn: &Connection, image_id: i64) -> anyhow::Result<Option<Self>> {
        let result = conn.query_row(
            "SELECT w.id, w.package_name, w.wit_text, w.world_name, w.import_count, w.export_count, w.created_at
             FROM wit_interface w
             JOIN image_wit_interface iwi ON w.id = iwi.wit_interface_id
             WHERE iwi.image_id = ?1",
            [image_id],
            |row| {
                Ok(WitInterface {
                    id: row.get(0)?,
                    package_name: row.get(1)?,
                    wit_text: row.get(2)?,
                    world_name: row.get(3)?,
                    import_count: row.get(4)?,
                    export_count: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        );

        match result {
            Ok(interface) => Ok(Some(interface)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get all WIT interfaces with their associated image references.
    pub(crate) fn get_all_with_images(conn: &Connection) -> anyhow::Result<Vec<(Self, String)>> {
        let mut stmt = conn.prepare(
            "SELECT w.id, w.package_name, w.wit_text, w.world_name, w.import_count, w.export_count, w.created_at,
                    i.ref_registry || '/' || i.ref_repository || COALESCE(':' || i.ref_tag, '') as reference
             FROM wit_interface w
             JOIN image_wit_interface iwi ON w.id = iwi.wit_interface_id
             JOIN image i ON iwi.image_id = i.id
             ORDER BY w.package_name ASC, w.world_name ASC, i.ref_repository ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                WitInterface {
                    id: row.get(0)?,
                    package_name: row.get(1)?,
                    wit_text: row.get(2)?,
                    world_name: row.get(3)?,
                    import_count: row.get(4)?,
                    export_count: row.get(5)?,
                    created_at: row.get(6)?,
                },
                row.get::<_, String>(7)?,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get all unique WIT interfaces.
    #[allow(dead_code)]
    pub(crate) fn get_all(conn: &Connection) -> anyhow::Result<Vec<Self>> {
        let mut stmt = conn.prepare(
            "SELECT id, package_name, wit_text, world_name, import_count, export_count, created_at
             FROM wit_interface
             ORDER BY package_name ASC, world_name ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(WitInterface {
                id: row.get(0)?,
                package_name: row.get(1)?,
                wit_text: row.get(2)?,
                world_name: row.get(3)?,
                import_count: row.get(4)?,
                export_count: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Delete a WIT interface by ID (also removes links).
    #[allow(dead_code)]
    pub(crate) fn delete(conn: &Connection, id: i64) -> anyhow::Result<bool> {
        let rows = conn.execute("DELETE FROM wit_interface WHERE id = ?1", [id])?;
        Ok(rows > 0)
    }
}
