use crate::models::{
    ContentBlock, ContentDraftV2, ContentTypeFields, EditorialAction, FooterColumn,
    LinkTargetReference, NavigationItem,
};

pub(super) fn validate(draft: &ContentDraftV2) -> Vec<(String, String)> {
    let mut issues = Vec::new();
    for (index, block) in draft.composition.blocks.iter().enumerate() {
        match block {
            ContentBlock::Hero(value) => {
                for (action_index, action) in value.actions.iter().enumerate() {
                    validate_action(
                        action,
                        &format!("composition.blocks.{index}.actions.{action_index}"),
                        &mut issues,
                    );
                }
            }
            ContentBlock::Cta(value) => validate_action(
                &value.action,
                &format!("composition.blocks.{index}.action"),
                &mut issues,
            ),
            ContentBlock::ContactBlock(value) => {
                if let Some(action) = &value.action {
                    validate_action(
                        action,
                        &format!("composition.blocks.{index}.action"),
                        &mut issues,
                    );
                }
            }
            _ => {}
        }
    }
    match &draft.type_fields {
        ContentTypeFields::GeneralInformation(value) => {
            if value
                .home_path
                .as_deref()
                .is_some_and(|path| !safe_route(path))
            {
                invalid(
                    &mut issues,
                    "typeFields.homePath",
                    "Home path must be a safe local absolute path without query or fragment.",
                );
            }
            if let Some(action) = &value.navigation_cta {
                validate_action(action, "typeFields.navigationCta", &mut issues);
            }
            for (index, link) in value.social_links.iter().enumerate() {
                if !safe_external_url(&link.url) {
                    invalid(
                        &mut issues,
                        &format!("typeFields.socialLinks.{index}.url"),
                        "Social links require an absolute HTTPS URL.",
                    );
                }
            }
        }
        ContentTypeFields::Navigation(value) => {
            validate_navigation(&value.items, "typeFields.items", &mut issues);
        }
        ContentTypeFields::Footer(value) => {
            for (index, column) in value.columns.iter().enumerate() {
                validate_footer_column(column, &format!("typeFields.columns.{index}"), &mut issues);
            }
            validate_navigation(&value.legal_links, "typeFields.legalLinks", &mut issues);
        }
        _ => {}
    }
    issues
}

fn validate_footer_column(column: &FooterColumn, path: &str, issues: &mut Vec<(String, String)>) {
    validate_navigation(&column.links, &format!("{path}.links"), issues);
}

fn validate_navigation(items: &[NavigationItem], path: &str, issues: &mut Vec<(String, String)>) {
    for (index, item) in items.iter().enumerate() {
        let item_path = format!("{path}.{index}");
        if let Some(target) = &item.target {
            validate_target(target, &format!("{item_path}.target"), issues);
        }
        validate_navigation(&item.children, &format!("{item_path}.children"), issues);
    }
}

fn validate_action(action: &EditorialAction, path: &str, issues: &mut Vec<(String, String)>) {
    validate_target(&action.target, &format!("{path}.target"), issues);
}

fn validate_target(target: &LinkTargetReference, path: &str, issues: &mut Vec<(String, String)>) {
    let valid = match target {
        LinkTargetReference::Content { .. } => true,
        LinkTargetReference::Route { path } => safe_route(path),
        LinkTargetReference::External { url } => safe_external_url(url),
    };
    if !valid {
        invalid(
            issues,
            path,
            "Link target must use a local absolute path or an absolute HTTPS URL.",
        );
    }
}

fn safe_route(value: &str) -> bool {
    value.starts_with('/')
        && !value.starts_with("//")
        && !value.contains('\\')
        && !value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        && !value.contains(['?', '#'])
}

fn safe_external_url(value: &str) -> bool {
    value.starts_with("https://") && !value.chars().any(char::is_whitespace)
}

fn invalid(issues: &mut Vec<(String, String)>, path: &str, message: &str) {
    issues.push((path.into(), message.into()));
}

#[cfg(test)]
mod tests {
    use super::safe_route;

    #[test]
    fn local_routes_reject_network_paths_and_backslashes() {
        assert!(safe_route("/en/resources"));
        assert!(!safe_route("//example.test"));
        assert!(!safe_route("/\\example.test"));
    }
}
