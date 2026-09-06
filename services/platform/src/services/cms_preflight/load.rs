use sqlx::{PgConnection, PgPool, Row};

use super::types::*;

/// Loads one consistent legacy CMS snapshot. PostgreSQL enforces `READ ONLY`,
/// and every statement after it is a `SELECT`.
pub async fn load_legacy_snapshot(pool: &PgPool) -> Result<LegacySnapshot, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let snapshot = load_legacy_snapshot_from_connection(&mut transaction).await?;
    transaction.rollback().await?;
    Ok(snapshot)
}

/// Loads the same complete snapshot inside a caller-owned transaction. The
/// unified migration service uses this while holding its advisory lock.
pub async fn load_legacy_snapshot_from_connection(
    connection: &mut PgConnection,
) -> Result<LegacySnapshot, sqlx::Error> {
    Ok(LegacySnapshot {
        content_entries: load_content_entries(connection).await?,
        content_revisions: load_content_revisions(connection).await?,
        news: load_news(connection).await?,
        news_working: load_news_working(connection).await?,
        general_information: load_general_information(connection).await?,
        general_information_revisions: load_general_information_revisions(connection).await?,
        content_relations: load_content_relations(connection).await?,
        media_assets: load_media_assets(connection).await?,
        asset_references: load_asset_references(connection).await?,
        public_routes: load_public_routes(connection).await?,
        products: load_product_identities(connection).await?,
    })
}

async fn load_product_identities(
    connection: &mut PgConnection,
) -> Result<Vec<LegacyProductIdentity>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT product.id,product.locale,product.published_revision,
                  localization.product_id IS NOT NULL AS has_localization,
                  localization.seo_metadata->>'canonicalPath' AS canonical_path,
                  COALESCE(localization.is_placeholder,false) AS is_placeholder,
                  COALESCE(localization.indexable,false) AS indexable
           FROM products product
           LEFT JOIN product_localizations localization
             ON localization.product_id=product.id
            AND localization.product_revision=product.published_revision
            AND localization.locale=product.locale
            AND localization.translation_state='verified'
           ORDER BY product.id"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyProductIdentity {
                id: row.try_get("id")?,
                locale: row.try_get("locale")?,
                published_revision: row.try_get("published_revision")?,
                has_verified_published_localization: row.try_get("has_localization")?,
                canonical_path: row.try_get("canonical_path")?,
                is_placeholder: row.try_get("is_placeholder")?,
                indexable: row.try_get("indexable")?,
            })
        })
        .collect()
}

async fn load_public_routes(
    connection: &mut PgConnection,
) -> Result<Vec<LegacyPublicRoute>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT id,entity_type,entity_id,locale,canonical_path,indexable
           FROM public_routes ORDER BY canonical_path,id"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyPublicRoute {
                id: row.try_get("id")?,
                entity_type: row.try_get("entity_type")?,
                entity_id: row.try_get("entity_id")?,
                locale: row.try_get("locale")?,
                canonical_path: row.try_get("canonical_path")?,
                indexable: row.try_get("indexable")?,
            })
        })
        .collect()
}

async fn load_content_entries(
    connection: &mut PgConnection,
) -> Result<Vec<LegacyContentEntry>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT id,kind,slug,locale,title,status,is_placeholder,data_origin,
                  current_revision,published_revision,scheduled_for,payload,updated_at
           FROM content_entries ORDER BY id"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyContentEntry {
                id: row.try_get("id")?,
                kind: row.try_get("kind")?,
                slug: row.try_get("slug")?,
                locale: row.try_get("locale")?,
                title: row.try_get("title")?,
                status: row.try_get("status")?,
                is_placeholder: row.try_get("is_placeholder")?,
                data_origin: row.try_get("data_origin")?,
                current_revision: row.try_get("current_revision")?,
                published_revision: row.try_get("published_revision")?,
                scheduled_for: row.try_get("scheduled_for")?,
                payload: row.try_get("payload")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

async fn load_content_revisions(
    connection: &mut PgConnection,
) -> Result<Vec<LegacyContentRevision>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT content_id,revision,payload,created_by,created_at
           FROM content_revisions ORDER BY content_id,revision"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyContentRevision {
                content_id: row.try_get("content_id")?,
                revision: row.try_get("revision")?,
                payload: row.try_get("payload")?,
                created_by: row.try_get("created_by")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}

async fn load_news(connection: &mut PgConnection) -> Result<Vec<LegacyNews>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT content_id,revision,content_kind,category,author_display_name,
                  cover_media_asset_id,featured,publication_at,reading_minutes,data_origin
           FROM news ORDER BY content_id,revision"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyNews {
                content_id: row.try_get("content_id")?,
                revision: row.try_get("revision")?,
                content_kind: row.try_get("content_kind")?,
                category: row.try_get("category")?,
                author_display_name: row.try_get("author_display_name")?,
                cover_media_asset_id: row.try_get("cover_media_asset_id")?,
                featured: row.try_get("featured")?,
                publication_at: row.try_get("publication_at")?,
                reading_minutes: row.try_get("reading_minutes")?,
                data_origin: row.try_get("data_origin")?,
            })
        })
        .collect()
}

async fn load_news_working(
    connection: &mut PgConnection,
) -> Result<Vec<LegacyNewsWorking>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT content_id,content_kind,category,author_display_name,
                  cover_media_asset_id,featured,publication_at,reading_minutes,
                  data_origin,updated_at
           FROM news_working ORDER BY content_id"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyNewsWorking {
                content_id: row.try_get("content_id")?,
                content_kind: row.try_get("content_kind")?,
                category: row.try_get("category")?,
                author_display_name: row.try_get("author_display_name")?,
                cover_media_asset_id: row.try_get("cover_media_asset_id")?,
                featured: row.try_get("featured")?,
                publication_at: row.try_get("publication_at")?,
                reading_minutes: row.try_get("reading_minutes")?,
                data_origin: row.try_get("data_origin")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

async fn load_general_information(
    connection: &mut PgConnection,
) -> Result<Vec<LegacyGeneralInformation>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT id,scope,locale,status,is_placeholder,data_origin,current_revision,
                  published_revision,scheduled_for,payload,updated_by,updated_at
           FROM general_information ORDER BY id"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyGeneralInformation {
                id: row.try_get("id")?,
                scope: row.try_get("scope")?,
                locale: row.try_get("locale")?,
                status: row.try_get("status")?,
                is_placeholder: row.try_get("is_placeholder")?,
                data_origin: row.try_get("data_origin")?,
                current_revision: row.try_get("current_revision")?,
                published_revision: row.try_get("published_revision")?,
                scheduled_for: row.try_get("scheduled_for")?,
                payload: row.try_get("payload")?,
                updated_by: row.try_get("updated_by")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

async fn load_general_information_revisions(
    connection: &mut PgConnection,
) -> Result<Vec<LegacyGeneralInformationRevision>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT general_information_id,revision,locale,is_placeholder,data_origin,
                  payload,created_by,created_at
           FROM general_information_revisions
           ORDER BY general_information_id,revision"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyGeneralInformationRevision {
                general_information_id: row.try_get("general_information_id")?,
                revision: row.try_get("revision")?,
                locale: row.try_get("locale")?,
                is_placeholder: row.try_get("is_placeholder")?,
                data_origin: row.try_get("data_origin")?,
                payload: row.try_get("payload")?,
                created_by: row.try_get("created_by")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}

async fn load_content_relations(
    connection: &mut PgConnection,
) -> Result<Vec<LegacyContentRelation>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT from_type,from_id,relation_type,to_type,to_id,sort_order
           FROM content_relations
           ORDER BY from_type,from_id,relation_type,to_type,to_id"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyContentRelation {
                from_type: row.try_get("from_type")?,
                from_id: row.try_get("from_id")?,
                relation_type: row.try_get("relation_type")?,
                to_type: row.try_get("to_type")?,
                to_id: row.try_get("to_id")?,
                sort_order: row.try_get("sort_order")?,
            })
        })
        .collect()
}

async fn load_media_assets(
    connection: &mut PgConnection,
) -> Result<Vec<LegacyMediaAsset>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT id,storage_key,original_name,media_type,byte_size,checksum,
                  scan_status,access_level,metadata,created_at,deleted_at
           FROM media_assets ORDER BY id"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyMediaAsset {
                id: row.try_get("id")?,
                storage_key: row.try_get("storage_key")?,
                original_name: row.try_get("original_name")?,
                media_type: row.try_get("media_type")?,
                byte_size: row.try_get("byte_size")?,
                checksum: row.try_get("checksum")?,
                scan_status: row.try_get("scan_status")?,
                access_level: row.try_get("access_level")?,
                metadata: row.try_get("metadata")?,
                created_at: row.try_get("created_at")?,
                deleted_at: row.try_get("deleted_at")?,
            })
        })
        .collect()
}

async fn load_asset_references(
    connection: &mut PgConnection,
) -> Result<Vec<LegacyAssetReference>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT id,media_asset_id,content_id,content_revision,product_id,
                  product_revision,general_information_id,
                  general_information_revision,usage,locale,alt_text,sort_order
           FROM asset_references ORDER BY id"#,
    )
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(LegacyAssetReference {
                id: row.try_get("id")?,
                media_asset_id: row.try_get("media_asset_id")?,
                content_id: row.try_get("content_id")?,
                content_revision: row.try_get("content_revision")?,
                product_id: row.try_get("product_id")?,
                product_revision: row.try_get("product_revision")?,
                general_information_id: row.try_get("general_information_id")?,
                general_information_revision: row.try_get("general_information_revision")?,
                usage: row.try_get("usage")?,
                locale: row.try_get("locale")?,
                alt_text: row.try_get("alt_text")?,
                sort_order: row.try_get("sort_order")?,
            })
        })
        .collect()
}
