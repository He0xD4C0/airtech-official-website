use airtek_domain::models::ContentBlock;

use super::{
    invalid, optional_media, optional_string, string, unique, validate_action, validate_media,
    ContractIssue,
};

pub(super) fn validate(block: &ContentBlock, path: &str, issues: &mut Vec<ContractIssue>) {
    match block {
        ContentBlock::Hero(value) => {
            optional_string(
                value.eyebrow.as_deref(),
                160,
                &format!("{path}.eyebrow"),
                issues,
            );
            optional_string(
                value.heading.as_deref(),
                300,
                &format!("{path}.heading"),
                issues,
            );
            optional_string(
                value.lead.as_deref(),
                2_000,
                &format!("{path}.lead"),
                issues,
            );
            optional_media(value.media.as_ref(), &format!("{path}.media"), issues);
            if value.actions.len() > 2 {
                invalid(
                    issues,
                    &format!("{path}.actions"),
                    "Hero blocks accept at most two actions.",
                );
            }
            for (index, action) in value.actions.iter().enumerate() {
                validate_action(action, &format!("{path}.actions.{index}"), issues);
            }
        }
        ContentBlock::Body(_) => {}
        ContentBlock::Media(value) => {
            validate_media(&value.media, &format!("{path}.media"), issues);
            optional_string(
                value.caption.as_deref(),
                1_000,
                &format!("{path}.caption"),
                issues,
            );
        }
        ContentBlock::FeatureGrid(value) => {
            optional_string(
                value.heading.as_deref(),
                300,
                &format!("{path}.heading"),
                issues,
            );
            for (index, item) in value.items.iter().enumerate() {
                let item_path = format!("{path}.items.{index}");
                string(&item.title, 200, &format!("{item_path}.title"), issues);
                optional_string(
                    item.description.as_deref(),
                    2_000,
                    &format!("{item_path}.description"),
                    issues,
                );
                optional_media(item.icon.as_ref(), &format!("{item_path}.icon"), issues);
            }
        }
        ContentBlock::Evidence(value) => {
            optional_string(
                value.heading.as_deref(),
                300,
                &format!("{path}.heading"),
                issues,
            );
            for (index, item) in value.items.iter().enumerate() {
                let item_path = format!("{path}.items.{index}");
                string(&item.label, 200, &format!("{item_path}.label"), issues);
                string(
                    &item.statement,
                    2_000,
                    &format!("{item_path}.statement"),
                    issues,
                );
                optional_string(
                    item.source_note.as_deref(),
                    1_000,
                    &format!("{item_path}.sourceNote"),
                    issues,
                );
            }
        }
        ContentBlock::Cta(value) => {
            optional_string(
                value.eyebrow.as_deref(),
                160,
                &format!("{path}.eyebrow"),
                issues,
            );
            string(&value.heading, 300, &format!("{path}.heading"), issues);
            optional_string(
                value.body.as_deref(),
                2_000,
                &format!("{path}.body"),
                issues,
            );
            validate_action(&value.action, &format!("{path}.action"), issues);
        }
        ContentBlock::RelationCollection(value) => {
            optional_string(
                value.heading.as_deref(),
                300,
                &format!("{path}.heading"),
                issues,
            );
            unique(
                &value.relation_ids,
                &format!("{path}.relationIds"),
                "Relation collection IDs must be unique.",
                issues,
            );
        }
        ContentBlock::FaqCollection(value) => {
            optional_string(
                value.heading.as_deref(),
                300,
                &format!("{path}.heading"),
                issues,
            );
        }
        ContentBlock::DownloadAsset(value) => {
            string(&value.label, 200, &format!("{path}.label"), issues);
            optional_string(
                value.description.as_deref(),
                2_000,
                &format!("{path}.description"),
                issues,
            );
        }
        ContentBlock::ContactBlock(value) => {
            optional_string(
                value.heading.as_deref(),
                300,
                &format!("{path}.heading"),
                issues,
            );
            unique(
                &value.channels,
                &format!("{path}.channels"),
                "Contact channels must be unique.",
                issues,
            );
            if let Some(action) = &value.action {
                validate_action(action, &format!("{path}.action"), issues);
            }
        }
    }
}
