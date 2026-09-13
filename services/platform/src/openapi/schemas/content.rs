//! Content authoring and publication schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "ContentKind".into(),
        string_enum(&[
            "home",
            "solution",
            "technology",
            "article",
            "news",
            "faq",
            "caseStudy",
            "download",
            "company",
            "legal",
            "navigation",
            "footer",
        ]),
    );
    s.insert(
        "PublicationStatus".into(),
        string_enum(&["draft", "scheduled", "published", "archived"]),
    );
    s.insert(
        "RichTextDocument".into(),
        object(
            &["schemaVersion", "doc"],
            json!({"schemaVersion": {"type": "integer", "minimum": 1}, "doc": {}}),
        ),
    );
    s.insert(
        "SeoMetadata".into(),
        object(
            &["title", "description", "canonicalPath", "indexable"],
            seo_properties(),
        ),
    );
    s.insert(
        "SeoMetadataInput".into(),
        json!({"type": "object", "additionalProperties": false, "required": ["indexable"], "properties": seo_properties()}),
    );
    s.insert("ContentEntry".into(), object(
        &["id", "kind", "slug", "locale", "title", "summary", "body", "seo", "status", "isPlaceholder", "currentRevision", "publishedRevision", "scheduledFor", "updatedAt"],
        json!({
            "id": uuid(), "kind": r("ContentKind"), "slug": slug(), "locale": {"type": "string"}, "title": {"type": "string"},
            "summary": nullable(json!({"type": "string"})), "body": r("RichTextDocument"), "seo": r("SeoMetadata"), "status": r("PublicationStatus"),
            "isPlaceholder": {"type": "boolean"}, "currentRevision": revision(), "publishedRevision": nullable(revision()),
            "scheduledFor": nullable(timestamp()), "updatedAt": timestamp()
        })
    ));
    s.insert("ContentDraftInput".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["kind", "slug", "title", "body"],
        "properties": {
            "kind": r("ContentKind"), "slug": slug(), "locale": {"type": "string", "enum": ["en"], "default": "en"},
            "title": {"type": "string", "minLength": 1, "maxLength": 300}, "summary": nullable(json!({"type": "string"})),
            "body": r("RichTextDocument"), "seo": r("SeoMetadataInput"), "isPlaceholder": {"type": "boolean", "default": false}
        }
    }));
    s.insert(
        "CreateContentPreviewRequest".into(),
        json!({
            "type": "object", "additionalProperties": false, "required": ["revision"],
            "properties": {
                "revision": revision(),
                "expiresInSeconds": {"type": "integer", "minimum": 1, "maximum": 600, "default": 600}
            }
        }),
    );
    s.insert(
        "ContentPreviewLink".into(),
        object(
            &["url", "contentId", "revision", "issuedAt", "expiresAt"],
            json!({
                "url": {"type": "string", "format": "uri", "readOnly": true},
                "contentId": uuid(), "revision": revision(),
                "issuedAt": timestamp(), "expiresAt": timestamp()
            }),
        ),
    );
    s.insert(
        "ContentPreviewResponse".into(),
        object(
            &["content", "previewExpiresAt"],
            json!({"content": r("PublicContentProjection"), "previewExpiresAt": timestamp()}),
        ),
    );
    s.insert(
        "RollbackContentRequest".into(),
        object(
            &["revision", "reason"],
            json!({"revision": revision(), "reason": {"type": "string", "minLength": 10}}),
        ),
    );
    s.insert(
        "PublishContentRequest".into(),
        object(
            &["reason"],
            json!({"reason": {"type": "string", "minLength": 10, "maxLength": 2000}}),
        ),
    );
    s.insert(
        "ContentPublicationAction".into(),
        string_enum(&["save", "publish", "unpublish"]),
    );
    s.insert(
        "PublicationReadinessIssue".into(),
        object(
            &["code", "path", "detail", "failedGate", "targetId"],
            json!({
                "code": {"type": "string"},
                "path": {"type": "string"},
                "detail": {"type": "string"},
                "failedGate": nullable(json!({"type": "string"})),
                "targetId": nullable(uuid())
            }),
        ),
    );
    s.insert(
        "ContentPublicationReadiness".into(),
        object(
            &[
                "contentId",
                "draftVersion",
                "ready",
                "issues",
                "allowedActions",
            ],
            json!({
                "contentId": uuid(),
                "draftVersion": revision(),
                "ready": {"type": "boolean"},
                "issues": {"type": "array", "items": r("PublicationReadinessIssue")},
                "allowedActions": {"type": "array", "items": r("ContentPublicationAction")}
            }),
        ),
    );
    s.insert("ContentPage".into(), page("ContentEntry"));
}
