use serde_json::json;

use crate::models::{TiptapDocument, TiptapRootType};

use super::{is_safe_href, validate};

#[test]
fn accepts_the_controlled_body_node_set() {
    let document = TiptapDocument {
        node_type: TiptapRootType::Doc,
        content: vec![
            json!({
                "type": "heading", "attrs": {"level": 2},
                "content": [{"type": "text", "text": "Heading", "marks": [{"type": "bold"}]}]
            }),
            json!({
                "type": "paragraph",
                "content": [{
                    "type": "text", "text": "Contact",
                    "marks": [{"type": "link", "attrs": {"href": "/en/company/contact"}}]
                }]
            }),
            json!({"type": "formula", "attrs": {"latex": "Q=P/A"}}),
            json!({
                "type": "table",
                "content": [{
                    "type": "tableRow",
                    "content": [{"type": "tableCell", "content": [{"type": "paragraph"}]}]
                }]
            }),
        ],
    };
    assert!(validate(&document).is_empty());
}

#[test]
fn rejects_legacy_slots_unsafe_links_and_unapproved_headings() {
    let document = TiptapDocument {
        node_type: TiptapRootType::Doc,
        content: vec![
            json!({"type": "heading", "attrs": {"level": 5, "pageSlots": {}}}),
            json!({
                "type": "text", "text": "unsafe",
                "marks": [{"type": "link", "attrs": {"href": "//example.test"}}]
            }),
            json!({"type": "script", "text": "alert(1)"}),
        ],
    };
    let paths = validate(&document)
        .into_iter()
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    assert!(paths.iter().any(|path| path.ends_with("attrs.pageSlots")));
    assert!(paths.iter().any(|path| path.ends_with("attrs.level")));
    assert!(paths.iter().any(|path| path.ends_with("attrs.href")));
    assert!(paths.iter().any(|path| path.ends_with("type")));
}

#[test]
fn rejects_rich_text_values_outside_the_openapi_contract() {
    let document = TiptapDocument {
        node_type: TiptapRootType::Doc,
        content: vec![
            json!({"type": "orderedList", "attrs": {"start": 0}}),
            json!({"type": "codeBlock", "attrs": {"language": 7}}),
            json!({"type": "tableCell", "attrs": {"colspan": -1}}),
            json!({"type": "text", "text": "value", "attrs": {}}),
            json!({
                "type": "text", "text": "link",
                "marks": [{
                    "type": "link",
                    "attrs": {"href": "/en/page trailing", "target": "popup", "rel": 42}
                }]
            }),
        ],
    };
    let paths = validate(&document)
        .into_iter()
        .map(|(path, _)| path)
        .collect::<Vec<_>>();

    for suffix in [
        "attrs.start",
        "attrs.language",
        "attrs.colspan",
        "content.3.attrs",
        "attrs.href",
        "attrs.target",
        "attrs.rel",
    ] {
        assert!(paths.iter().any(|path| path.ends_with(suffix)), "{suffix}");
    }

    let oversized = TiptapDocument {
        node_type: TiptapRootType::Doc,
        content: vec![json!({"type": "paragraph"}); 10_001],
    };
    assert!(validate(&oversized)
        .iter()
        .any(|(path, _)| path == "body.content"));
}

#[test]
fn permits_local_query_and_fragment_links_without_whitespace() {
    let document = TiptapDocument {
        node_type: TiptapRootType::Doc,
        content: vec![json!({
            "type": "paragraph",
            "content": [{
                "type": "text", "text": "details",
                "marks": [{
                    "type": "link",
                    "attrs": {"href": "/en/resources?type=pdf#downloads", "target": null}
                }]
            }]
        })],
    };

    assert!(validate(&document).is_empty());
    assert!(!is_safe_href("/\\evil.example"));
}
