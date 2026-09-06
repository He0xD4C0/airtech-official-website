//! CMS V2 enums, strict rich-text root, references, actions, and templates.

use serde_json::{json, Map, Value};

use super::super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    add_enums(s);
    add_tiptap(s);
    add_assets(s);
    add_relations_and_actions(s);
    add_template(s);
}

fn add_enums(s: &mut Map<String, Value>) {
    s.insert(
        "CmsContentKind".into(),
        string_enum(&[
            "home",
            "page",
            "solution",
            "technology",
            "article",
            "news",
            "faq",
            "caseStudy",
            "download",
            "company",
            "legal",
            "generalInformation",
            "navigation",
            "footer",
        ]),
    );
    s.insert(
        "ContentTemplateKey".into(),
        string_enum(&[
            "home",
            "productIndex",
            "productFamily",
            "selector",
            "compare",
            "solutionIndex",
            "solutionDetail",
            "technologyIndex",
            "technologyDetail",
            "articleIndex",
            "articleDetail",
            "newsIndex",
            "newsDetail",
            "faqIndex",
            "faqDetail",
            "caseStudyIndex",
            "caseStudyDetail",
            "downloadIndex",
            "downloadDetail",
            "about",
            "contact",
            "rfqRouter",
            "rfqForm",
            "search",
            "legal",
            "navigation",
            "footer",
            "generalInformation",
        ]),
    );
    s.insert(
        "ContentBlockKind".into(),
        string_enum(&[
            "hero",
            "body",
            "media",
            "featureGrid",
            "evidence",
            "cta",
            "relationCollection",
            "faqCollection",
            "downloadAsset",
            "contactBlock",
        ]),
    );
    s.insert(
        "CmsBodyPolicy".into(),
        string_enum(&["required", "optional", "forbidden"]),
    );
    s.insert(
        "CmsPublicationStatusV2".into(),
        string_enum(&["draft", "published", "archived"]),
    );
}

fn add_tiptap(s: &mut Map<String, Value>) {
    s.insert(
        "TiptapLinkAttrs".into(),
        object(
            &["href"],
            json!({
                "href": {
                    "type": "string", "maxLength": 2048,
                    "pattern": "^(?:https://[^\\s\\\\]+|/(?![/\\\\])[^\\s\\\\]*|mailto:[^\\s\\\\]+|tel:[^\\s\\\\]+)$"
                },
                "target": nullable(string_enum(&["_self", "_blank"])),
                "rel": nullable(json!({"type": "string", "maxLength": 120}))
            }),
        ),
    );
    s.insert(
        "TiptapSimpleMark".into(),
        object(
            &["type"],
            json!({"type": string_enum(&["bold", "italic", "underline", "strike", "code"])}),
        ),
    );
    s.insert(
        "TiptapLinkMark".into(),
        object(
            &["type", "attrs"],
            json!({
                "type": {"type": "string", "const": "link"},
                "attrs": r("TiptapLinkAttrs")
            }),
        ),
    );
    s.insert(
        "TiptapMark".into(),
        json!({"oneOf": [r("TiptapSimpleMark"), r("TiptapLinkMark")]}),
    );
    s.insert("TiptapNoAttrs".into(), object(&[], json!({})));
    s.insert(
        "TiptapHeadingAttrs".into(),
        object(
            &["level"],
            json!({"level": {"type": "integer", "minimum": 2, "maximum": 4}}),
        ),
    );
    s.insert(
        "TiptapOrderedListAttrs".into(),
        object(&[], json!({"start": {"type": "integer", "minimum": 1}})),
    );
    s.insert(
        "TiptapCodeBlockAttrs".into(),
        object(
            &[],
            json!({"language": {"type": "string", "maxLength": 80}}),
        ),
    );
    s.insert(
        "TiptapFormulaAttrs".into(),
        object(
            &["latex"],
            json!({"latex": {"type": "string", "minLength": 1, "maxLength": 10000}}),
        ),
    );
    s.insert(
        "TiptapTableCellAttrs".into(),
        object(
            &[],
            json!({
                "colspan": {"type": "integer", "minimum": 1},
                "rowspan": {"type": "integer", "minimum": 1}
            }),
        ),
    );
    add_tiptap_nodes(s);
}

fn add_tiptap_nodes(s: &mut Map<String, Value>) {
    s.insert(
        "TiptapTextNode".into(),
        tiptap_node(&["text"], &["text"], None, true, false),
    );
    s.insert(
        "TiptapPlainNode".into(),
        tiptap_node(
            &[
                "paragraph",
                "bulletList",
                "listItem",
                "blockquote",
                "hardBreak",
                "table",
                "tableRow",
            ],
            &[],
            Some(r("TiptapNoAttrs")),
            false,
            true,
        ),
    );
    for (name, node_type, attrs, attrs_required) in [
        ("TiptapHeadingNode", "heading", "TiptapHeadingAttrs", true),
        (
            "TiptapOrderedListNode",
            "orderedList",
            "TiptapOrderedListAttrs",
            false,
        ),
        (
            "TiptapCodeBlockNode",
            "codeBlock",
            "TiptapCodeBlockAttrs",
            false,
        ),
        ("TiptapFormulaNode", "formula", "TiptapFormulaAttrs", true),
    ] {
        let required = if attrs_required {
            &["attrs"] as &[&str]
        } else {
            &[]
        };
        s.insert(
            name.into(),
            tiptap_node(&[node_type], required, Some(r(attrs)), false, true),
        );
    }
    s.insert(
        "TiptapTableCellNode".into(),
        tiptap_node(
            &["tableHeader", "tableCell"],
            &[],
            Some(r("TiptapTableCellAttrs")),
            false,
            true,
        ),
    );
    s.insert(
        "TiptapNode".into(),
        json!({
            "oneOf": [
                r("TiptapTextNode"), r("TiptapPlainNode"), r("TiptapHeadingNode"),
                r("TiptapOrderedListNode"), r("TiptapCodeBlockNode"),
                r("TiptapFormulaNode"), r("TiptapTableCellNode")
            ]
        }),
    );
    s.insert(
        "TiptapDocument".into(),
        object(
            &["type", "content"],
            json!({
                "type": {"type": "string", "const": "doc"},
                "content": {"type": "array", "maxItems": 10000, "items": r("TiptapNode")}
            }),
        ),
    );
}

fn tiptap_node(
    node_types: &[&str],
    required: &[&str],
    attrs: Option<Value>,
    text: bool,
    content: bool,
) -> Value {
    let mut properties = Map::new();
    properties.insert("type".into(), string_enum(node_types));
    if let Some(attrs) = attrs {
        properties.insert("attrs".into(), attrs);
    }
    if text {
        properties.insert(
            "text".into(),
            json!({"type": "string", "maxLength": 100000}),
        );
    }
    properties.insert(
        "marks".into(),
        json!({"type": "array", "maxItems": 16, "items": r("TiptapMark")}),
    );
    if content {
        properties.insert(
            "content".into(),
            json!({"type": "array", "maxItems": 10000, "items": r("TiptapNode")}),
        );
    }
    let mut all_required = vec!["type"];
    all_required.extend_from_slice(required);
    object(&all_required, Value::Object(properties))
}

fn add_assets(s: &mut Map<String, Value>) {
    s.insert(
        "AssetVersionReference".into(),
        object(
            &["assetId", "versionId"],
            json!({"assetId": uuid(), "versionId": uuid()}),
        ),
    );
    s.insert(
        "MediaUseReference".into(),
        object(
            &["asset", "decorative"],
            json!({
                "asset": r("AssetVersionReference"),
                "altText": nullable(json!({"type": "string", "maxLength": 500})),
                "decorative": {"type": "boolean"}
            }),
        ),
    );
}

fn add_relations_and_actions(s: &mut Map<String, Value>) {
    s.insert(
        "RelationTargetContent".into(),
        typed_object(
            "targetType",
            "content",
            &["contentId"],
            json!({"contentId": uuid()}),
        ),
    );
    s.insert(
        "RelationTargetProduct".into(),
        typed_object(
            "targetType",
            "product",
            &["productId"],
            json!({"productId": uuid()}),
        ),
    );
    s.insert(
        "RelationTargetReference".into(),
        json!({
            "oneOf": [r("RelationTargetContent"), r("RelationTargetProduct")],
            "discriminator": {
                "propertyName": "targetType",
                "mapping": {
                    "content": "#/components/schemas/RelationTargetContent",
                    "product": "#/components/schemas/RelationTargetProduct"
                }
            }
        }),
    );
    s.insert(
        "ContentRelationReference".into(),
        object(
            &["id", "slot", "target"],
            json!({
                "id": uuid(),
                "slot": {"type": "string", "maxLength": 120},
                "target": r("RelationTargetReference")
            }),
        ),
    );
    s.insert(
        "LinkTargetContent".into(),
        typed_object(
            "targetType",
            "content",
            &["contentId"],
            json!({"contentId": uuid()}),
        ),
    );
    s.insert(
        "LinkTargetRoute".into(),
        typed_object(
            "targetType",
            "route",
            &["path"],
            json!({
                "path": {
                    "type": "string", "maxLength": 2048,
                    "pattern": "^/(?![/\\\\])[^?#\\s\\\\]*$"
                }
            }),
        ),
    );
    s.insert(
        "LinkTargetExternal".into(),
        typed_object(
            "targetType",
            "external",
            &["url"],
            json!({
                "url": {"type": "string", "format": "uri", "maxLength": 2048, "pattern": "^https://"}
            }),
        ),
    );
    s.insert(
        "LinkTargetReference".into(),
        json!({
            "oneOf": [r("LinkTargetContent"), r("LinkTargetRoute"), r("LinkTargetExternal")],
            "discriminator": {
                "propertyName": "targetType",
                "mapping": {
                    "content": "#/components/schemas/LinkTargetContent",
                    "route": "#/components/schemas/LinkTargetRoute",
                    "external": "#/components/schemas/LinkTargetExternal"
                }
            }
        }),
    );
    s.insert(
        "EditorialAction".into(),
        object(
            &["label", "target"],
            json!({
                "label": {"type": "string", "maxLength": 120},
                "target": r("LinkTargetReference")
            }),
        ),
    );
}

fn add_template(s: &mut Map<String, Value>) {
    s.insert(
        "ContentTemplateDefinition".into(),
        object(
            &[
                "key",
                "contentKind",
                "bodyPolicy",
                "requiredBlocks",
                "allowedBlocks",
                "routable",
                "singletonPerLocale",
            ],
            json!({
                "key": r("ContentTemplateKey"), "contentKind": r("CmsContentKind"),
                "bodyPolicy": r("CmsBodyPolicy"),
                "requiredBlocks": {
                    "type": "array", "uniqueItems": true, "items": r("ContentBlockKind")
                },
                "allowedBlocks": {
                    "type": "array", "uniqueItems": true, "items": r("ContentBlockKind")
                },
                "routable": {"type": "boolean"},
                "singletonPerLocale": {"type": "boolean"}
            }),
        ),
    );
}

pub(super) fn typed_object(
    tag_name: &str,
    tag_value: &str,
    required: &[&str],
    properties: Value,
) -> Value {
    let mut properties = properties.as_object().cloned().expect("properties object");
    properties.insert(
        tag_name.into(),
        json!({"type": "string", "const": tag_value}),
    );
    let mut all_required = vec![tag_name];
    all_required.extend_from_slice(required);
    object(&all_required, Value::Object(properties))
}
