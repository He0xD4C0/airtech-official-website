#[cfg(test)]
mod tests {
    use super::{
        is_public_projection_topic, product_import_id_from_job_payload, resolve_integer_setting,
        retention_job_is_due, COMPLETE_JOB_SQL, FAIL_JOB_SQL, JOB_LEASE_RENEW_INTERVAL,
        JOB_LEASE_SECONDS, RENEW_JOB_LEASE_SQL,
    };
    use chrono::{Duration, Utc};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn product_import_job_payload_is_reference_only() {
        let id = Uuid::new_v4();
        assert_eq!(
            product_import_id_from_job_payload(&json!({"importRunId": id})).unwrap(),
            id
        );
        assert!(product_import_id_from_job_payload(&json!({
            "importRunId": id,
            "csv": "stableId,price"
        }))
        .is_err());
        assert!(product_import_id_from_job_payload(&json!({
            "importRunId": id,
            "price": "12.50"
        }))
        .is_err());
    }

    #[test]
    fn every_publication_topic_has_a_request_time_projection_consumer() {
        for topic in [
            "public.content.published",
            "public.content.unpublished",
            "public.product.published",
            "public.news.published",
            "public.generalInformation.published",
        ] {
            assert!(is_public_projection_topic(topic), "missing topic {topic}");
        }
        assert!(!is_public_projection_topic("public.unknown.published"));
    }

    #[test]
    fn deployment_retention_values_apply_until_an_admin_overrides_migration_defaults() {
        assert_eq!(
            resolve_integer_setting(
                "guestVisitRetentionDays",
                Some((json!(180), "migration".into())),
                42,
                1,
                3_650,
            )
            .unwrap(),
            42
        );
        assert_eq!(
            resolve_integer_setting(
                "guestVisitRetentionDays",
                Some((json!(90), "admin-user".into())),
                42,
                1,
                3_650,
            )
            .unwrap(),
            90
        );
        assert!(resolve_integer_setting(
            "guestVisitRetentionDays",
            Some((json!(0), "admin-user".into())),
            42,
            1,
            3_650,
        )
        .is_err());
    }

    #[test]
    fn daily_retention_schedule_is_due_only_without_active_or_recent_work() {
        let now = Utc::now();
        assert!(retention_job_is_due(None, now));
        assert!(!retention_job_is_due(
            Some(("queued", now - Duration::days(7))),
            now
        ));
        assert!(!retention_job_is_due(
            Some(("running", now - Duration::days(7))),
            now
        ));
        assert!(!retention_job_is_due(
            Some(("completed", now - Duration::hours(23))),
            now
        ));
        assert!(retention_job_is_due(
            Some(("completed", now - Duration::hours(24))),
            now
        ));
        assert!(retention_job_is_due(
            Some(("failed", now - Duration::hours(25))),
            now
        ));
    }

    #[test]
    fn lease_renewal_has_headroom_and_all_lifecycle_writes_are_owner_fenced() {
        assert!(JOB_LEASE_RENEW_INTERVAL.as_secs() * 2 < JOB_LEASE_SECONDS as u64);
        for statement in [RENEW_JOB_LEASE_SQL, COMPLETE_JOB_SQL, FAIL_JOB_SQL] {
            assert!(statement.contains("status='running'"));
            assert!(statement.contains("lease_owner=$"));
        }
    }
}
