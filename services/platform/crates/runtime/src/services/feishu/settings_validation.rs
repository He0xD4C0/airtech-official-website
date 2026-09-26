use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveTime;

use crate::error::ApiError;
use airtek_domain::models::{FeishuSource, UpdateFeishuSettings};

pub(super) fn normalize_sources(sources: &[FeishuSource]) -> Vec<FeishuSource> {
    sources
        .iter()
        .map(|source| FeishuSource {
            enabled: source.enabled,
            wiki_token: source.wiki_token.trim().to_owned(),
            table_id: source.table_id.trim().to_owned(),
            name: source.name.trim().to_owned(),
            family: source.family,
            application: source
                .application
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        })
        .collect()
}

pub(super) fn validate_update(input: &UpdateFeishuSettings) -> Result<(), ApiError> {
    let mut errors = BTreeMap::<String, Vec<String>>::new();
    if input.clear_credentials {
        if !input.app_id.trim().is_empty() || !input.app_secret.is_empty() {
            errors.insert(
                "clearCredentials".into(),
                vec!["Clear App ID and App Secret when removing credentials.".into()],
            );
        }
        if input.enabled {
            errors.insert(
                "enabled".into(),
                vec!["Disable synchronization before removing credentials.".into()],
            );
        }
    } else {
        valid_text(&mut errors, "appId", input.app_id.trim(), 100);
        if input.app_secret.len() > 512 {
            errors.insert(
                "appSecret".into(),
                vec!["App Secret must contain at most 512 bytes.".into()],
            );
        }
    }
    if !(5..=1440).contains(&input.interval_minutes) {
        errors.insert(
            "intervalMinutes".into(),
            vec!["Value must be between 5 and 1440.".into()],
        );
    }
    if NaiveTime::parse_from_str(&input.daily_local_time, "%H:%M").is_err() {
        errors.insert(
            "dailyLocalTime".into(),
            vec!["Value must use HH:MM.".into()],
        );
    }
    validate_sources(input, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn validate_sources(input: &UpdateFeishuSettings, errors: &mut BTreeMap<String, Vec<String>>) {
    if input.sources.len() > 100 {
        errors.insert(
            "sources".into(),
            vec!["At most 100 sources are supported.".into()],
        );
    }
    if input.enabled && !input.sources.iter().any(|source| source.enabled) {
        errors.insert(
            "sources".into(),
            vec!["Enable at least one Feishu product table.".into()],
        );
    }
    let mut identities = BTreeSet::new();
    for (index, source) in input.sources.iter().enumerate() {
        valid_text(
            errors,
            &format!("sources.{index}.wikiToken"),
            &source.wiki_token,
            200,
        );
        valid_text(
            errors,
            &format!("sources.{index}.tableId"),
            &source.table_id,
            100,
        );
        valid_text(errors, &format!("sources.{index}.name"), &source.name, 120);
        if source
            .application
            .as_ref()
            .is_some_and(|value| value.trim().is_empty() || value.len() > 100)
        {
            errors.insert(
                format!("sources.{index}.application"),
                vec!["Application must contain 1 to 100 bytes when set.".into()],
            );
        }
        let identity = (
            source.wiki_token.trim().to_owned(),
            source.table_id.trim().to_owned(),
        );
        if !identities.insert(identity) {
            errors.insert(
                format!("sources.{index}.tableId"),
                vec!["Wiki Token and Table ID pairs must be unique.".into()],
            );
        }
    }
}

fn valid_text(
    errors: &mut BTreeMap<String, Vec<String>>,
    field: &str,
    value: &str,
    maximum: usize,
) {
    if value.trim().is_empty() || value.len() > maximum {
        errors.insert(
            field.into(),
            vec![format!("Value must contain 1 to {maximum} bytes.")],
        );
    }
}

pub(super) fn validation(field: &str, detail: &str) -> ApiError {
    ApiError::validation(BTreeMap::from([(field.into(), vec![detail.into()])]))
}

#[cfg(test)]
mod tests {
    use airtek_domain::models::{FeishuSource, ProductFamily, UpdateFeishuSettings};

    use super::{normalize_sources, validate_update};

    fn source(wiki_token: &str, table_id: &str) -> FeishuSource {
        FeishuSource {
            enabled: true,
            wiki_token: wiki_token.into(),
            table_id: table_id.into(),
            name: format!("{wiki_token}/{table_id}"),
            family: ProductFamily::Axial,
            application: None,
        }
    }

    fn update(sources: Vec<FeishuSource>) -> UpdateFeishuSettings {
        UpdateFeishuSettings {
            app_id: "cli_test".into(),
            app_secret: String::new(),
            clear_credentials: false,
            sources,
            enabled: true,
            interval_enabled: false,
            interval_minutes: 15,
            daily_enabled: false,
            daily_local_time: "02:00".into(),
        }
    }

    #[test]
    fn accepts_multiple_tables_from_one_wiki() {
        assert!(validate_update(&update(vec![
            source("wiki", "table-a"),
            source("wiki", "table-b"),
        ]))
        .is_ok());
    }

    #[test]
    fn source_identity_is_the_wiki_and_table_pair() {
        assert!(validate_update(&update(vec![
            source("wiki-a", "table"),
            source("wiki-b", "table"),
        ]))
        .is_ok());
        assert!(validate_update(&update(vec![
            source("wiki", "table"),
            source("wiki", "table"),
        ]))
        .is_err());
    }

    #[test]
    fn enabled_connector_requires_an_enabled_source() {
        let mut value = update(vec![source("wiki", "table")]);
        value.sources[0].enabled = false;
        assert!(validate_update(&value).is_err());
        value.enabled = false;
        assert!(validate_update(&value).is_ok());
    }

    #[test]
    fn source_identifiers_are_trimmed_before_the_revision_is_frozen() {
        let mut value = source(" wiki ", " table ");
        value.name = " Axial source ".into();
        value.application = Some(" agriculture-livestock ".into());
        let normalized = normalize_sources(&[value]);
        assert_eq!(normalized[0].wiki_token, "wiki");
        assert_eq!(normalized[0].table_id, "table");
        assert_eq!(normalized[0].name, "Axial source");
        assert_eq!(
            normalized[0].application.as_deref(),
            Some("agriculture-livestock")
        );
    }
}
