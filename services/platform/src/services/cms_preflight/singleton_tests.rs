use crate::models::{MigrationPreflightIssueCode, MigrationPreflightSeverity};

use super::{
    analyze_legacy_snapshot,
    tests::{legacy_entry, timestamp},
    types::LegacySnapshot,
};

#[test]
fn active_entries_cannot_share_a_singleton_template_and_locale() {
    let first = legacy_entry(uuid::Uuid::from_u128(1_700), "home", "draft", None);
    let second = legacy_entry(uuid::Uuid::from_u128(1_701), "home", "draft", None);
    let report = analyze_legacy_snapshot(
        LegacySnapshot {
            content_entries: vec![first, second],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );

    assert!(!report.can_migrate);
    assert_eq!(
        report
            .issues
            .iter()
            .filter(|issue| {
                issue.severity == MigrationPreflightSeverity::Blocking
                    && issue.code == MigrationPreflightIssueCode::RevisionConversionFailed
                    && issue.json_path.as_deref() == Some("templateKey")
            })
            .count(),
        2
    );
}

#[test]
fn archived_entry_does_not_conflict_with_an_active_singleton() {
    let archived = legacy_entry(uuid::Uuid::from_u128(1_710), "home", "archived", None);
    let active = legacy_entry(uuid::Uuid::from_u128(1_711), "home", "draft", None);
    let report = analyze_legacy_snapshot(
        LegacySnapshot {
            content_entries: vec![archived, active],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );

    assert!(report.can_migrate);
    assert!(!report.issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::RevisionConversionFailed
            && issue.json_path.as_deref() == Some("templateKey")
    }));
}
