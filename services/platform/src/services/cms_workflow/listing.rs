use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use super::{
    storage::{decode_draft, decode_published},
    storage_support::{is_super_admin, visibility_sql, DRAFT_COLUMNS},
};
use crate::{
    auth::AdminPrincipal,
    error::ApiError,
    models::{
        CmsContentKind, CmsDraftPage, CmsPublishedPage, CmsReviewItem, CmsReviewPage,
        CmsSiteSingletonState,
    },
    pagination::{cursor_limit, decode_scoped_cursor, encode_scoped_cursor, CursorQuery},
    state::AppState,
};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmsListQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub q: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UpdatedCursor {
    updated_at: DateTime<Utc>,
    id: Uuid,
}

impl CmsListQuery {
    fn pagination(&self) -> CursorQuery {
        CursorQuery {
            cursor: self.cursor.clone(),
            limit: self.limit,
        }
    }

    fn search(&self) -> Result<Option<String>, ApiError> {
        let value = self
            .q
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if value.is_some_and(|value| value.chars().count() > 200) {
            return Err(ApiError::bad_request("q must be at most 200 characters."));
        }
        Ok(value.map(str::to_owned))
    }
}

pub async fn list_drafts(
    state: &AppState,
    principal: &AdminPrincipal,
    query: CmsListQuery,
) -> Result<CmsDraftPage, ApiError> {
    let pagination = query.pagination();
    let limit = cursor_limit(&pagination)?;
    let search = query.search()?;
    let scope = format!("cms-drafts|q={}", search.as_deref().unwrap_or(""));
    let after = pagination
        .cursor
        .as_deref()
        .map(|value| decode_scoped_cursor::<UpdatedCursor>(&scope, value))
        .transpose()?;
    let visible = visibility_sql(1);
    let count_sql = format!(
        r#"SELECT count(*) FROM cms_drafts draft
           WHERE {visible}
             AND ($4::text IS NULL OR draft.document->>'title' ILIKE '%'||$4||'%'
                  OR draft.document->>'slug' ILIKE '%'||$4||'%')"#
    );
    let total = sqlx::query_scalar::<_, i64>(&count_sql)
        .bind(principal.user_id)
        .bind(is_super_admin(principal))
        .bind(principal.has_permission("content.publish"))
        .bind(search.as_deref())
        .fetch_one(&state.pool)
        .await?;
    let list_sql = format!(
        r#"SELECT {DRAFT_COLUMNS} FROM cms_drafts draft
           WHERE {visible}
             AND ($4::text IS NULL OR draft.document->>'title' ILIKE '%'||$4||'%'
                  OR draft.document->>'slug' ILIKE '%'||$4||'%')
             AND ($5::timestamptz IS NULL OR (draft.updated_at,draft.draft_id)<($5,$6))
           ORDER BY draft.updated_at DESC,draft.draft_id DESC LIMIT $7"#
    );
    let rows = sqlx::query(&list_sql)
        .bind(principal.user_id)
        .bind(is_super_admin(principal))
        .bind(principal.has_permission("content.publish"))
        .bind(search.as_deref())
        .bind(after.as_ref().map(|value| value.updated_at))
        .bind(after.as_ref().map(|value| value.id))
        .bind(i64::try_from(limit + 1).unwrap_or(101))
        .fetch_all(&state.pool)
        .await?;
    let mut items = rows
        .iter()
        .map(decode_draft)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = page_cursor(&scope, &mut items, limit, |item| UpdatedCursor {
        updated_at: item.updated_at,
        id: item.draft_id,
    })?;
    Ok(CmsDraftPage {
        items,
        next_cursor,
        total,
    })
}

pub async fn list_published(
    state: &AppState,
    query: CmsListQuery,
) -> Result<CmsPublishedPage, ApiError> {
    let pagination = query.pagination();
    let limit = cursor_limit(&pagination)?;
    let search = query.search()?;
    let scope = format!("cms-published|q={}", search.as_deref().unwrap_or(""));
    let after = pagination
        .cursor
        .as_deref()
        .map(|value| decode_scoped_cursor::<UpdatedCursor>(&scope, value))
        .transpose()?;
    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*) FROM cms_published_content published
           WHERE $1::text IS NULL OR published.document->>'title' ILIKE '%'||$1||'%'
              OR published.document->>'slug' ILIKE '%'||$1||'%'"#,
    )
    .bind(search.as_deref())
    .fetch_one(&state.pool)
    .await?;
    let rows = sqlx::query(
        r#"SELECT content_id,document,publication_version,published_by,
                  published_at,updated_at
           FROM cms_published_content published
           WHERE ($1::text IS NULL OR published.document->>'title' ILIKE '%'||$1||'%'
                  OR published.document->>'slug' ILIKE '%'||$1||'%')
             AND ($2::timestamptz IS NULL OR (published.updated_at,published.content_id)<($2,$3))
           ORDER BY published.updated_at DESC,published.content_id DESC LIMIT $4"#,
    )
    .bind(search.as_deref())
    .bind(after.as_ref().map(|value| value.updated_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut items = rows
        .iter()
        .map(decode_published)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = page_cursor(&scope, &mut items, limit, |item| UpdatedCursor {
        updated_at: item.updated_at,
        id: item.content_id,
    })?;
    Ok(CmsPublishedPage {
        items,
        next_cursor,
        total,
    })
}

pub async fn list_reviews(
    state: &AppState,
    query: CmsListQuery,
) -> Result<CmsReviewPage, ApiError> {
    let pagination = query.pagination();
    let limit = cursor_limit(&pagination)?;
    let search = query.search()?;
    let scope = format!("cms-reviews|q={}", search.as_deref().unwrap_or(""));
    let after = pagination
        .cursor
        .as_deref()
        .map(|value| decode_scoped_cursor::<UpdatedCursor>(&scope, value))
        .transpose()?;
    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*) FROM cms_review_queue queue
           JOIN cms_drafts draft ON draft.draft_id=queue.draft_id
           WHERE $1::text IS NULL OR draft.document->>'title' ILIKE '%'||$1||'%'
              OR draft.document->>'slug' ILIKE '%'||$1||'%'"#,
    )
    .bind(search.as_deref())
    .fetch_one(&state.pool)
    .await?;
    let rows = sqlx::query(&format!(
        r#"SELECT {DRAFT_COLUMNS},queue.submitted_by_user_id,queue.submitted_at
           FROM cms_review_queue queue
           JOIN cms_drafts draft ON draft.draft_id=queue.draft_id
           WHERE ($1::text IS NULL OR draft.document->>'title' ILIKE '%'||$1||'%'
                  OR draft.document->>'slug' ILIKE '%'||$1||'%')
             AND ($2::timestamptz IS NULL OR (queue.submitted_at,draft.draft_id)>($2,$3))
           ORDER BY queue.submitted_at,draft.draft_id LIMIT $4"#
    ))
    .bind(search.as_deref())
    .bind(after.as_ref().map(|value| value.updated_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut items = rows
        .iter()
        .map(|row| {
            Ok(CmsReviewItem {
                draft: decode_draft(row)?,
                submitted_by_user_id: row.try_get("submitted_by_user_id")?,
                submitted_at: row.try_get("submitted_at")?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let next_cursor = page_cursor(&scope, &mut items, limit, |item| UpdatedCursor {
        updated_at: item.submitted_at,
        id: item.draft.draft_id,
    })?;
    Ok(CmsReviewPage {
        items,
        next_cursor,
        total,
    })
}

pub async fn get_site_singleton(
    state: &AppState,
    principal: &AdminPrincipal,
    kind: CmsContentKind,
    locale: &str,
) -> Result<CmsSiteSingletonState, ApiError> {
    if !matches!(
        kind,
        CmsContentKind::GeneralInformation | CmsContentKind::Navigation | CmsContentKind::Footer
    ) {
        return Err(ApiError::bad_request(
            "Site singleton kind must be generalInformation, navigation, or footer.",
        ));
    }
    let locale = locale.trim();
    if locale.is_empty()
        || locale.chars().count() > 32
        || !locale
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
    {
        return Err(ApiError::bad_request(
            "locale must be a valid locale identifier.",
        ));
    }
    let kind = serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| ApiError::internal("Site singleton kind serialization failed."))?;
    let own_draft = sqlx::query(&format!(
        r#"SELECT {DRAFT_COLUMNS} FROM cms_drafts draft
           WHERE draft.owner_user_id=$1
             AND draft.document->>'kind'=$2
             AND draft.document->>'locale'=$3
           ORDER BY draft.updated_at DESC,draft.draft_id DESC LIMIT 1"#
    ))
    .bind(principal.user_id)
    .bind(&kind)
    .bind(locale)
    .fetch_optional(&state.pool)
    .await?
    .as_ref()
    .map(decode_draft)
    .transpose()?;
    let published = sqlx::query(
        r#"SELECT content_id,document,publication_version,published_by,
                  published_at,updated_at
           FROM cms_published_content
           WHERE document->>'kind'=$1 AND document->>'locale'=$2
           ORDER BY updated_at DESC,content_id DESC LIMIT 1"#,
    )
    .bind(&kind)
    .bind(locale)
    .fetch_optional(&state.pool)
    .await?
    .as_ref()
    .map(decode_published)
    .transpose()?;
    Ok(CmsSiteSingletonState {
        own_draft,
        published,
    })
}

fn page_cursor<T>(
    scope: &str,
    items: &mut Vec<T>,
    limit: usize,
    position: impl Fn(&T) -> UpdatedCursor,
) -> Result<Option<String>, ApiError> {
    let has_more = items.len() > limit;
    items.truncate(limit);
    if has_more {
        items
            .last()
            .map(|item| encode_scoped_cursor(scope, &position(item)))
            .transpose()
    } else {
        Ok(None)
    }
}
