//! Private draft, review, and current publication schemas.

use serde_json::{json, Map, Value};

use super::super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "CmsDraftState".into(),
        string_enum(&["editing", "pendingReview"]),
    );
    s.insert(
        "CmsPrivateDraft".into(),
        object(
            &[
                "draftId",
                "contentId",
                "ownerUserId",
                "document",
                "draftVersion",
                "basePublicationVersion",
                "state",
                "rejectionReason",
                "createdAt",
                "updatedAt",
            ],
            json!({
                "draftId":uuid(),"contentId":uuid(),"ownerUserId":nullable(uuid()),
                "document":r("ContentDraftV2"),"draftVersion":revision(),
                "basePublicationVersion":{"type":"integer","minimum":0},
                "state":r("CmsDraftState"),
                "rejectionReason":nullable(json!({"type":"string","maxLength":2000})),
                "createdAt":timestamp(),"updatedAt":timestamp()
            }),
        ),
    );
    s.insert(
        "CmsPublishedContent".into(),
        object(
            &[
                "contentId",
                "document",
                "publicationVersion",
                "publishedBy",
                "publishedAt",
                "updatedAt",
            ],
            json!({
                "contentId":uuid(),"document":r("ContentDraftV2"),
                "publicationVersion":revision(),"publishedBy":nullable(uuid()),
                "publishedAt":timestamp(),"updatedAt":timestamp()
            }),
        ),
    );
    s.insert(
        "CmsReviewItem".into(),
        object(
            &["draft","submittedByUserId","submittedAt"],
            json!({"draft":r("CmsPrivateDraft"),"submittedByUserId":uuid(),"submittedAt":timestamp()}),
        ),
    );
    s.insert("CmsDraftPage".into(), cms_page("CmsPrivateDraft"));
    s.insert("CmsPublishedPage".into(), cms_page("CmsPublishedContent"));
    s.insert("CmsReviewPage".into(), cms_page("CmsReviewItem"));
    s.insert(
        "CmsSiteSingletonState".into(),
        object(
            &["ownDraft", "published"],
            json!({
                "ownDraft":nullable(r("CmsPrivateDraft")),
                "published":nullable(r("CmsPublishedContent"))
            }),
        ),
    );
    s.insert(
        "CmsDraftSharesRequest".into(),
        object(
            &["userIds"],
            json!({"userIds":{"type":"array","maxItems":100,"uniqueItems":true,"items":uuid()}}),
        ),
    );
    s.insert(
        "CmsRejectRequest".into(),
        object(
            &["reason"],
            json!({"reason":{"type":"string","minLength":1,"maxLength":2000}}),
        ),
    );
    s.insert(
        "CmsPublishResult".into(),
        object(
            &["contentId", "publicationVersion", "publishedAt"],
            json!({"contentId":uuid(),"publicationVersion":revision(),"publishedAt":timestamp()}),
        ),
    );
    s.insert(
        "CmsSubmitResult".into(),
        object(
            &["status", "draft", "publication"],
            json!({
                "status":string_enum(&["pendingReview","published"]),
                "draft":nullable(r("CmsPrivateDraft")),
                "publication":nullable(r("CmsPublishResult"))
            }),
        ),
    );
    s.insert(
        "ContentTemplateDefinitionPage".into(),
        object(
            &["items"],
            json!({"items":array(r("ContentTemplateDefinition"))}),
        ),
    );
}

fn cms_page(item: &str) -> Value {
    object(
        &["items", "nextCursor", "total"],
        json!({
            "items":array(r(item)),
            "nextCursor":nullable(json!({"type":"string"})),
            "total":{"type":"integer","minimum":0}
        }),
    )
}
