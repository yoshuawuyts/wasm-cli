use sea_orm::DatabaseConnection;

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

    /// Convert a SeaORM wit_interface model to a WitInterface.
    fn from_model(model: crate::storage::entities::wit_interface::Model) -> Self {
        Self {
            id: model.id,
            package_name: model.package_name,
            wit_text: model.wit_text,
            world_name: model.world_name,
            import_count: model.import_count,
            export_count: model.export_count,
            created_at: model.created_at,
        }
    }

    /// Insert a new WIT interface and return its ID.
    /// Uses content-addressable storage - if the same WIT text already exists, returns existing ID.
    pub(crate) async fn insert(
        conn: &DatabaseConnection,
        wit_text: &str,
        package_name: Option<&str>,
        world_name: Option<&str>,
        import_count: i32,
        export_count: i32,
    ) -> anyhow::Result<i64> {
        use crate::storage::entities::wit_interface;
        use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

        // Check if this exact WIT text already exists
        let existing = wit_interface::Entity::find()
            .filter(wit_interface::Column::WitText.eq(wit_text))
            .one(conn)
            .await?;

        if let Some(model) = existing {
            return Ok(model.id);
        }

        // Insert new WIT interface
        let model = wit_interface::ActiveModel {
            wit_text: Set(wit_text.to_string()),
            package_name: Set(package_name.map(|s| s.to_string())),
            world_name: Set(world_name.map(|s| s.to_string())),
            import_count: Set(import_count),
            export_count: Set(export_count),
            ..Default::default()
        };

        let result = wit_interface::Entity::insert(model).exec(conn).await?;
        Ok(result.last_insert_id)
    }

    /// Link an image to a WIT interface.
    pub(crate) async fn link_to_image(
        conn: &DatabaseConnection,
        image_id: i64,
        wit_interface_id: i64,
    ) -> anyhow::Result<()> {
        use crate::storage::entities::image_wit_interface;
        use sea_orm::{ActiveValue::Set, EntityTrait};

        let model = image_wit_interface::ActiveModel {
            image_id: Set(image_id),
            wit_interface_id: Set(wit_interface_id),
        };

        let on_conflict = sea_orm::sea_query::OnConflict::columns([
            image_wit_interface::Column::ImageId,
            image_wit_interface::Column::WitInterfaceId,
        ])
        .do_nothing()
        .to_owned();

        // INSERT OR IGNORE semantics: ignore RecordNotInserted errors
        match image_wit_interface::Entity::insert(model)
            .on_conflict(on_conflict)
            .exec(conn)
            .await
        {
            Ok(_) | Err(sea_orm::DbErr::RecordNotInserted) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Get WIT interface for an image by image ID.
    #[allow(dead_code)]
    pub(crate) async fn get_for_image(
        conn: &DatabaseConnection,
        image_id: i64,
    ) -> anyhow::Result<Option<Self>> {
        use crate::storage::entities::{image_wit_interface, wit_interface};
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        let model = wit_interface::Entity::find()
            .inner_join(image_wit_interface::Entity)
            .filter(image_wit_interface::Column::ImageId.eq(image_id))
            .one(conn)
            .await?;

        Ok(model.map(Self::from_model))
    }

    /// Get all WIT interfaces with their associated image references.
    pub(crate) async fn get_all_with_images(
        conn: &DatabaseConnection,
    ) -> anyhow::Result<Vec<(Self, String)>> {
        use crate::storage::entities::{image, wit_interface};
        use sea_orm::{EntityTrait, QueryOrder};

        let results: Vec<(wit_interface::Model, Vec<image::Model>)> = wit_interface::Entity::find()
            .find_with_related(image::Entity)
            .order_by_asc(wit_interface::Column::PackageName)
            .order_by_asc(wit_interface::Column::WorldName)
            .all(conn)
            .await?;

        let mut output = Vec::new();
        for (wit_model, mut image_models) in results {
            // Skip wit interfaces without images (equivalent to INNER JOIN)
            if image_models.is_empty() {
                continue;
            }
            // Sort images by repository to match original ordering
            image_models.sort_by(|a, b| a.ref_repository.cmp(&b.ref_repository));
            for img in image_models {
                let mut reference = format!("{}/{}", img.ref_registry, img.ref_repository);
                if let Some(tag) = &img.ref_tag {
                    reference.push(':');
                    reference.push_str(tag);
                }
                output.push((Self::from_model(wit_model.clone()), reference));
            }
        }
        Ok(output)
    }

    /// Get all unique WIT interfaces.
    #[allow(dead_code)]
    pub(crate) async fn get_all(conn: &DatabaseConnection) -> anyhow::Result<Vec<Self>> {
        use crate::storage::entities::wit_interface;
        use sea_orm::{EntityTrait, QueryOrder};

        let models = wit_interface::Entity::find()
            .order_by_asc(wit_interface::Column::PackageName)
            .order_by_asc(wit_interface::Column::WorldName)
            .all(conn)
            .await?;

        Ok(models.into_iter().map(Self::from_model).collect())
    }

    /// Delete a WIT interface by ID (also removes links).
    #[allow(dead_code)]
    pub(crate) async fn delete(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
        use crate::storage::entities::wit_interface;
        use sea_orm::EntityTrait;

        let result = wit_interface::Entity::delete_by_id(id).exec(conn).await?;
        Ok(result.rows_affected > 0)
    }
}
