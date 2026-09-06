use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::models::TiptapDocument;

pub(super) fn validate(document: &TiptapDocument) -> Vec<(String, String)> {
    validate_at(document, "body")
}

pub(super) fn validate_at(document: &TiptapDocument, root_path: &str) -> Vec<(String, String)> {
    let mut issues = Vec::new();
    if document.content.len() > 10_000 {
        invalid(
            &mut issues,
            &format!("{root_path}.content"),
            "Document content exceeds the supported item count.",
        );
    }
    for (index, node) in document.content.iter().enumerate() {
        validate_node(node, &format!("{root_path}.content.{index}"), &mut issues);
    }
    issues
}

fn validate_node(value: &Value, path: &str, issues: &mut Vec<(String, String)>) {
    let Some(node) = value.as_object() else {
        invalid(issues, path, "Rich-text nodes must be JSON objects.");
        return;
    };
    let Some(node_type) = node.get("type").and_then(Value::as_str) else {
        invalid(
            issues,
            &format!("{path}.type"),
            "Rich-text nodes require a type.",
        );
        return;
    };
    if !matches!(
        node_type,
        "paragraph"
            | "heading"
            | "bulletList"
            | "orderedList"
            | "listItem"
            | "blockquote"
            | "text"
            | "hardBreak"
            | "codeBlock"
            | "formula"
            | "table"
            | "tableRow"
            | "tableHeader"
            | "tableCell"
    ) {
        invalid(
            issues,
            &format!("{path}.type"),
            "The rich-text node type is not in the CMS allowlist.",
        );
    }
    reject_unknown_keys(node, path, issues);
    validate_attributes(node_type, node.get("attrs"), path, issues);
    validate_marks(node.get("marks"), path, issues);

    if node_type == "text" {
        if node.get("text").and_then(Value::as_str).is_none() {
            invalid(
                issues,
                &format!("{path}.text"),
                "Text nodes require text content.",
            );
        } else if node
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.chars().count() > 100_000)
        {
            invalid(
                issues,
                &format!("{path}.text"),
                "Text node content exceeds the supported length.",
            );
        }
        if node.contains_key("content") {
            invalid(
                issues,
                &format!("{path}.content"),
                "Text nodes cannot contain child nodes.",
            );
        }
        return;
    }
    if node.contains_key("text") {
        invalid(
            issues,
            &format!("{path}.text"),
            "Only text nodes can carry a text property.",
        );
    }
    if let Some(content) = node.get("content") {
        let Some(children) = content.as_array() else {
            invalid(
                issues,
                &format!("{path}.content"),
                "Node content must be an array.",
            );
            return;
        };
        if children.len() > 10_000 {
            invalid(
                issues,
                &format!("{path}.content"),
                "Node content exceeds the supported item count.",
            );
        }
        for (index, child) in children.iter().enumerate() {
            validate_node(child, &format!("{path}.content.{index}"), issues);
        }
    }
}

fn reject_unknown_keys(node: &Map<String, Value>, path: &str, issues: &mut Vec<(String, String)>) {
    let allowed = ["type", "attrs", "text", "marks", "content"]
        .into_iter()
        .collect::<BTreeSet<_>>();
    for key in node.keys().filter(|key| !allowed.contains(key.as_str())) {
        invalid(
            issues,
            &format!("{path}.{key}"),
            "Unknown rich-text properties are not accepted.",
        );
    }
}

fn validate_attributes(
    node_type: &str,
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<(String, String)>,
) {
    let allowed: &[&str] = match node_type {
        "heading" => &["level"],
        "orderedList" => &["start"],
        "codeBlock" => &["language"],
        "formula" => &["latex"],
        "tableHeader" | "tableCell" => &["colspan", "rowspan"],
        _ => &[],
    };
    let Some(value) = value else {
        if node_type == "formula" {
            invalid(
                issues,
                &format!("{path}.attrs.latex"),
                "Formula nodes require LaTeX.",
            );
        }
        if node_type == "heading" {
            invalid(
                issues,
                &format!("{path}.attrs.level"),
                "Only H2 through H4 are allowed.",
            );
        }
        return;
    };
    if node_type == "text" {
        invalid(
            issues,
            &format!("{path}.attrs"),
            "Text nodes do not accept attributes.",
        );
        return;
    }
    let Some(attrs) = value.as_object() else {
        invalid(
            issues,
            &format!("{path}.attrs"),
            "Node attributes must be an object.",
        );
        return;
    };
    for key in attrs.keys().filter(|key| !allowed.contains(&key.as_str())) {
        invalid(
            issues,
            &format!("{path}.attrs.{key}"),
            "The attribute is not allowed for this rich-text node.",
        );
    }
    if node_type == "heading" && !matches!(attrs.get("level").and_then(Value::as_u64), Some(2..=4))
    {
        invalid(
            issues,
            &format!("{path}.attrs.level"),
            "Only H2 through H4 are allowed.",
        );
    }
    if node_type == "formula"
        && attrs
            .get("latex")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
    {
        invalid(
            issues,
            &format!("{path}.attrs.latex"),
            "Formula nodes require LaTeX.",
        );
    }
    match node_type {
        "orderedList" => validate_positive_integer(attrs, "start", path, issues),
        "tableHeader" | "tableCell" => {
            validate_positive_integer(attrs, "colspan", path, issues);
            validate_positive_integer(attrs, "rowspan", path, issues);
        }
        "codeBlock" => validate_optional_string(attrs, "language", 80, path, issues),
        "formula" => validate_optional_string(attrs, "latex", 10_000, path, issues),
        _ => {}
    }
}

fn validate_positive_integer(
    attrs: &Map<String, Value>,
    key: &str,
    path: &str,
    issues: &mut Vec<(String, String)>,
) {
    if attrs
        .get(key)
        .is_some_and(|value| !matches!(value.as_u64(), Some(1..)))
    {
        invalid(
            issues,
            &format!("{path}.attrs.{key}"),
            "The attribute must be a positive integer.",
        );
    }
}

fn validate_optional_string(
    attrs: &Map<String, Value>,
    key: &str,
    max_length: usize,
    path: &str,
    issues: &mut Vec<(String, String)>,
) {
    if attrs.get(key).is_some_and(|value| {
        value
            .as_str()
            .is_none_or(|text| text.chars().count() > max_length)
    }) {
        invalid(
            issues,
            &format!("{path}.attrs.{key}"),
            "The attribute must be a supported string value.",
        );
    }
}

fn validate_marks(value: Option<&Value>, path: &str, issues: &mut Vec<(String, String)>) {
    let Some(value) = value else { return };
    let Some(marks) = value.as_array() else {
        invalid(issues, &format!("{path}.marks"), "Marks must be an array.");
        return;
    };
    if marks.len() > 16 {
        invalid(
            issues,
            &format!("{path}.marks"),
            "A rich-text node cannot contain more than 16 marks.",
        );
    }
    for (index, mark) in marks.iter().enumerate() {
        let mark_path = format!("{path}.marks.{index}");
        let Some(mark) = mark.as_object() else {
            invalid(issues, &mark_path, "Marks must be objects.");
            continue;
        };
        let mark_type = mark.get("type").and_then(Value::as_str);
        if !matches!(
            mark_type,
            Some("bold" | "italic" | "underline" | "strike" | "code" | "link")
        ) {
            invalid(
                issues,
                &format!("{mark_path}.type"),
                "The mark type is not allowed.",
            );
        }
        for key in mark
            .keys()
            .filter(|key| !matches!(key.as_str(), "type" | "attrs"))
        {
            invalid(
                issues,
                &format!("{mark_path}.{key}"),
                "Unknown mark properties are not accepted.",
            );
        }
        if mark_type == Some("link") {
            validate_link(mark.get("attrs"), &mark_path, issues);
        } else if mark.contains_key("attrs") {
            invalid(
                issues,
                &format!("{mark_path}.attrs"),
                "Only link marks accept attributes.",
            );
        }
    }
}

fn validate_link(value: Option<&Value>, path: &str, issues: &mut Vec<(String, String)>) {
    let Some(attrs) = value.and_then(Value::as_object) else {
        invalid(
            issues,
            &format!("{path}.attrs"),
            "Link marks require attributes.",
        );
        return;
    };
    for key in attrs
        .keys()
        .filter(|key| !matches!(key.as_str(), "href" | "target" | "rel"))
    {
        invalid(
            issues,
            &format!("{path}.attrs.{key}"),
            "Unknown link attributes are not accepted.",
        );
    }
    let safe_href = attrs
        .get("href")
        .and_then(Value::as_str)
        .is_some_and(is_safe_href);
    if !safe_href {
        invalid(
            issues,
            &format!("{path}.attrs.href"),
            "Link href uses a disallowed scheme.",
        );
    }
    if attrs.get("target").is_some_and(|target| {
        !target.is_null() && !matches!(target.as_str(), Some("_self" | "_blank"))
    }) {
        invalid(
            issues,
            &format!("{path}.attrs.target"),
            "Link target must be _self, _blank, or null.",
        );
    }
    if attrs.get("rel").is_some_and(|rel| {
        !rel.is_null() && rel.as_str().is_none_or(|value| value.chars().count() > 120)
    }) {
        invalid(
            issues,
            &format!("{path}.attrs.rel"),
            "Link rel must be a supported string or null.",
        );
    }
}

fn is_safe_href(href: &str) -> bool {
    if href.chars().count() > 2_048
        || href.contains('\\')
        || href
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return false;
    }
    (href.starts_with('/') && !href.starts_with("//"))
        || href
            .strip_prefix("https://")
            .is_some_and(|remainder| !remainder.is_empty())
        || href
            .strip_prefix("mailto:")
            .is_some_and(|remainder| !remainder.is_empty())
        || href
            .strip_prefix("tel:")
            .is_some_and(|remainder| !remainder.is_empty())
}

fn invalid(issues: &mut Vec<(String, String)>, path: &str, message: &str) {
    issues.push((path.into(), message.into()));
}

#[cfg(test)]
mod tests;
