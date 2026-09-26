//! Strict CMS V2 composition block variants.

use serde_json::{json, Map, Value};

use super::super::super::support::*;
use super::core::typed_object;

pub(super) fn add(s: &mut Map<String, Value>) {
    add_enums(s);
    add_items(s);
    add_variants(s);
    add_union(s);
}

fn add_enums(s: &mut Map<String, Value>) {
    s.insert(
        "HeroVariant".into(),
        string_enum(&["standard", "splitMedia", "minimal"]),
    );
    s.insert(
        "ContentWidth".into(),
        string_enum(&["narrow", "standard", "wide"]),
    );
    s.insert(
        "MediaLayout".into(),
        string_enum(&["inline", "fullWidth", "aside"]),
    );
    s.insert(
        "CtaVariant".into(),
        string_enum(&["standard", "emphasized"]),
    );
    s.insert(
        "CollectionPresentation".into(),
        string_enum(&["cards", "list", "compact"]),
    );
    s.insert(
        "ContactChannelKind".into(),
        string_enum(&["email", "phone", "address", "social"]),
    );
}

fn add_items(s: &mut Map<String, Value>) {
    s.insert(
        "FeatureItem".into(),
        object(
            &["id", "title"],
            json!({
                "id": uuid(),
                "title": {"type": "string", "maxLength": 200},
                "description": nullable(json!({"type": "string", "maxLength": 2000})),
                "icon": nullable(r("MediaUseReference"))
            }),
        ),
    );
    s.insert(
        "EvidenceItem".into(),
        object(
            &["id", "label", "statement"],
            json!({
                "id": uuid(),
                "label": {"type": "string", "maxLength": 200},
                "statement": {"type": "string", "maxLength": 2000},
                "sourceNote": nullable(json!({"type": "string", "maxLength": 1000}))
            }),
        ),
    );
}

fn add_variants(s: &mut Map<String, Value>) {
    s.insert(
        "HeroBlock".into(),
        typed_object(
            "type",
            "hero",
            &["id", "actions", "variant"],
            json!({
                "id": uuid(),
                "eyebrow": nullable(json!({"type": "string", "maxLength": 160})),
                "heading": nullable(json!({"type": "string", "maxLength": 300})),
                "lead": nullable(json!({"type": "string", "maxLength": 2000})),
                "media": nullable(r("MediaUseReference")),
                "actions": {"type": "array", "maxItems": 2, "items": r("EditorialAction")},
                "variant": r("HeroVariant")
            }),
        ),
    );
    s.insert(
        "BodyBlock".into(),
        typed_object(
            "type",
            "body",
            &["id", "width"],
            json!({"id": uuid(), "width": r("ContentWidth")}),
        ),
    );
    s.insert(
        "MediaBlock".into(),
        typed_object(
            "type",
            "media",
            &["id", "media", "layout"],
            json!({
                "id": uuid(), "media": r("MediaUseReference"),
                "caption": nullable(json!({"type": "string", "maxLength": 1000})),
                "layout": r("MediaLayout")
            }),
        ),
    );
    s.insert(
        "FeatureGridBlock".into(),
        typed_object(
            "type",
            "featureGrid",
            &["id", "items"],
            json!({
                "id": uuid(),
                "heading": nullable(json!({"type": "string", "maxLength": 300})),
                "items": {"type": "array", "items": r("FeatureItem")}
            }),
        ),
    );
    s.insert(
        "EvidenceBlock".into(),
        typed_object(
            "type",
            "evidence",
            &["id", "items"],
            json!({
                "id": uuid(),
                "heading": nullable(json!({"type": "string", "maxLength": 300})),
                "items": {"type": "array", "items": r("EvidenceItem")}
            }),
        ),
    );
    s.insert(
        "CtaBlock".into(),
        typed_object(
            "type",
            "cta",
            &["id", "heading", "action", "variant"],
            json!({
                "id": uuid(),
                "eyebrow": nullable(json!({"type": "string", "maxLength": 160})),
                "heading": {"type": "string", "maxLength": 300},
                "body": nullable(json!({"type": "string", "maxLength": 2000})),
                "action": r("EditorialAction"), "variant": r("CtaVariant")
            }),
        ),
    );
    s.insert(
        "RelationCollectionBlock".into(),
        typed_object(
            "type",
            "relationCollection",
            &["id", "relationIds", "presentation"],
            json!({
                "id": uuid(),
                "heading": nullable(json!({"type": "string", "maxLength": 300})),
                "relationIds": {"type": "array", "uniqueItems": true, "items": uuid()},
                "presentation": r("CollectionPresentation")
            }),
        ),
    );
    s.insert(
        "FaqCollectionBlock".into(),
        typed_object(
            "type",
            "faqCollection",
            &["id"],
            json!({
                "id": uuid(),
                "heading": nullable(json!({"type": "string", "maxLength": 300}))
            }),
        ),
    );
    s.insert(
        "DownloadAssetBlock".into(),
        typed_object(
            "type",
            "downloadAsset",
            &["id", "asset", "label"],
            json!({
                "id": uuid(), "asset": r("AssetVersionReference"),
                "label": {"type": "string", "maxLength": 200},
                "description": nullable(json!({"type": "string", "maxLength": 2000}))
            }),
        ),
    );
    s.insert(
        "ContactBlock".into(),
        typed_object(
            "type",
            "contactBlock",
            &["id", "channels"],
            json!({
                "id": uuid(),
                "heading": nullable(json!({"type": "string", "maxLength": 300})),
                "channels": {
                    "type": "array", "uniqueItems": true, "items": r("ContactChannelKind")
                },
                "action": nullable(r("EditorialAction"))
            }),
        ),
    );
}

fn add_union(s: &mut Map<String, Value>) {
    s.insert(
        "ContentBlock".into(),
        json!({
            "oneOf": [
                r("HeroBlock"), r("BodyBlock"), r("MediaBlock"),
                r("FeatureGridBlock"), r("EvidenceBlock"), r("CtaBlock"),
                r("RelationCollectionBlock"), r("FaqCollectionBlock"),
                r("DownloadAssetBlock"), r("ContactBlock")
            ],
            "discriminator": {
                "propertyName": "type",
                "mapping": {
                    "hero": "#/components/schemas/HeroBlock",
                    "body": "#/components/schemas/BodyBlock",
                    "media": "#/components/schemas/MediaBlock",
                    "featureGrid": "#/components/schemas/FeatureGridBlock",
                    "evidence": "#/components/schemas/EvidenceBlock",
                    "cta": "#/components/schemas/CtaBlock",
                    "relationCollection": "#/components/schemas/RelationCollectionBlock",
                    "faqCollection": "#/components/schemas/FaqCollectionBlock",
                    "downloadAsset": "#/components/schemas/DownloadAssetBlock",
                    "contactBlock": "#/components/schemas/ContactBlock"
                }
            }
        }),
    );
    s.insert(
        "PageComposition".into(),
        object(
            &["blocks"],
            json!({"blocks": {"type": "array", "items": r("ContentBlock")}}),
        ),
    );
}
