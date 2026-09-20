//! Read-only editorial worklist. This never creates or publishes CMS records.
use crate::{
    error::ApiError,
    models::{ContentDraftV2, ContentTemplateKey},
    services::cms_templates::{canonical_path, validate_content_draft, CmsTemplateValidationPhase},
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreEntry {
    pub template_key: ContentTemplateKey,
    pub slug: Option<String>,
    pub title: String,
}

pub fn core_entries() -> Vec<CoreEntry> {
    serde_json::from_str(include_str!("../../../../config/public-site-core.json"))
        .expect("compiled core site identity manifest")
}

pub async fn inspect(pool: &PgPool) -> Result<Value, ApiError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let mut entries = Vec::new();
    for entry in core_entries() {
        let key = serde_json::to_value(entry.template_key).unwrap();
        let rows = sqlx::query(
            r#"SELECT entry.id,published.document AS published_document,
                 draft.draft_id,draft.document AS draft_document,draft.state
               FROM content_entries entry
               LEFT JOIN cms_published_content published ON published.content_id=entry.id
               LEFT JOIN cms_drafts draft ON draft.content_id=entry.id
               WHERE entry.locale='en' AND entry.template_key=$1
                 AND ($2::text IS NULL OR entry.slug=$2)
               ORDER BY entry.id,draft.draft_id"#,
        )
        .bind(key.as_str())
        .bind(
            if matches!(
                entry.template_key,
                ContentTemplateKey::RfqForm | ContentTemplateKey::Legal
            ) {
                entry.slug.as_deref()
            } else {
                None
            },
        )
        .fetch_all(&mut *tx)
        .await?;
        let records = rows
            .into_iter()
            .map(|row| {
                let id: uuid::Uuid = row.get("id");
                let draft_id: Option<uuid::Uuid> = row.get("draft_id");
                let published: Option<Value> = row.get("published_document");
                let draft: Option<Value> = row.get("draft_document");
                json!({
                    "contentId": id, "draftId": draft_id,
                    "draftPath": draft_id.map(|id| format!("/content/drafts/{id}")),
                    "draftState": row.get::<Option<String>,_>("state"),
                    "published": published.is_some(),
                    "publishedIssues": published.map(issues),
                    "draftIssues": draft.map(issues),
                })
            })
            .collect::<Vec<_>>();
        entries.push(json!({
            "templateKey": key, "slug": entry.slug, "title": entry.title,
            "canonicalPath": canonical_path("en", entry.template_key, entry.slug.as_deref()),
            "missing": records.is_empty(), "records": records,
        }));
    }
    tx.commit().await?;
    Ok(json!({"locale":"en","readOnly":true,"entries":entries}))
}

fn issues(document: Value) -> Vec<Value> {
    match serde_json::from_value::<ContentDraftV2>(document) {
        Ok(draft) => {
            let mut result = validate_content_draft(&draft, CmsTemplateValidationPhase::Publish)
                .into_iter()
                .map(|issue| json!({"path":issue.path,"message":issue.message}))
                .collect::<Vec<_>>();
            if draft.is_placeholder {
                result.push(json!({"path":"isPlaceholder","message":"Editorial verification and approval required."}));
            }
            result
        }
        Err(error) => vec![json!({"path":"document","message":error.to_string()})],
    }
}
