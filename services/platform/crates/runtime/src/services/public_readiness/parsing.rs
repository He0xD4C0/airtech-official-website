use std::collections::BTreeSet;

pub(super) fn tag_attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = tag.find(&needle)? + needle.len();
    let quote = tag.as_bytes().get(start).copied()? as char;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let value = &tag[start + 1..];
    let end = value.find(quote)?;
    Some(value[..end].to_owned())
}

fn tags<'a>(html: &'a str, name: &str) -> Vec<&'a str> {
    let marker = format!("<{name}");
    html.match_indices(&marker)
        .filter_map(|(start, _)| {
            html[start..]
                .find('>')
                .map(|end| &html[start..=start + end])
        })
        .collect()
}

pub(super) fn canonical_href(html: &str) -> Option<String> {
    tags(html, "link").into_iter().find_map(|tag| {
        let relations = tag_attribute(tag, "rel")?;
        relations
            .split_ascii_whitespace()
            .any(|value| value.eq_ignore_ascii_case("canonical"))
            .then(|| tag_attribute(tag, "href"))
            .flatten()
    })
}

pub(super) fn javascript_assets(html: &str) -> BTreeSet<String> {
    let mut assets = BTreeSet::new();
    for tag in tags(html, "script") {
        if let Some(source) = tag_attribute(tag, "src").filter(|value| is_javascript(value)) {
            assets.insert(source);
        }
    }
    for tag in tags(html, "link") {
        let relation = tag_attribute(tag, "rel").unwrap_or_default();
        if relation.split_ascii_whitespace().any(|value| {
            value.eq_ignore_ascii_case("modulepreload") || value.eq_ignore_ascii_case("preload")
        }) {
            if let Some(source) = tag_attribute(tag, "href").filter(|value| is_javascript(value)) {
                assets.insert(source);
            }
        }
    }
    assets
}

fn is_javascript(value: &str) -> bool {
    value
        .split(['?', '#'])
        .next()
        .is_some_and(|path| path.ends_with(".js") || path.ends_with(".mjs"))
}

pub(super) fn xml_locations(xml: &str) -> Result<Vec<String>, roxmltree::Error> {
    let document = roxmltree::Document::parse(xml)?;
    Ok(document
        .descendants()
        .filter(|node| node.has_tag_name("loc"))
        .filter_map(|node| node.text().map(str::to_owned))
        .collect())
}

pub(super) fn local_route_targets(value: &serde_json::Value) -> BTreeSet<String> {
    fn visit(value: &serde_json::Value, output: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    visit(value, output);
                }
            }
            serde_json::Value::Object(values) => {
                if values.get("targetType").and_then(serde_json::Value::as_str) == Some("route") {
                    if let Some(path) = values.get("path").and_then(serde_json::Value::as_str) {
                        output.insert(path.to_owned());
                    }
                }
                for value in values.values() {
                    visit(value, output);
                }
            }
            _ => {}
        }
    }
    let mut output = BTreeSet::new();
    visit(value, &mut output);
    output
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn extracts_canonical_and_javascript_assets_without_attribute_order_assumptions() {
        let html = r#"<link href='https://www.example.test/en' rel='canonical'>
          <link rel="modulepreload" href="/assets/chunk.js?x=1">
          <script defer src='/assets/app.mjs'></script>"#;
        assert_eq!(
            canonical_href(html).as_deref(),
            Some("https://www.example.test/en")
        );
        assert_eq!(
            javascript_assets(html).into_iter().collect::<Vec<_>>(),
            vec!["/assets/app.mjs", "/assets/chunk.js?x=1"]
        );
    }

    #[test]
    fn extracts_xml_locations_and_nested_route_targets() {
        assert_eq!(
            xml_locations(
                "<urlset><url><loc>https://example.test/a?a=1&amp;b=2</loc></url></urlset>"
            )
            .unwrap(),
            vec!["https://example.test/a?a=1&b=2"]
        );
        let targets = local_route_targets(&json!({
            "items": [{ "target": { "targetType": "route", "path": "/en" } }],
            "external": { "targetType": "external", "url": "https://example.test" }
        }));
        assert_eq!(targets.into_iter().collect::<Vec<_>>(), vec!["/en"]);
    }
}
