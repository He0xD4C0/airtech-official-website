#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        models::{ContentDraftInput, RichTextDocument, SeoMetadata},
    };

    #[test]
    fn clearing_a_development_news_placeholder_takes_editorial_ownership() {
        let mut input = NewsDraftInput {
            content: ContentDraftInput {
                kind: ContentKind::Article,
                slug: "development-news".into(),
                locale: "en".into(),
                title: "Development news".into(),
                summary: None,
                body: RichTextDocument {
                    schema_version: 1,
                    doc: json!({"type":"doc","content":[]}),
                },
                seo: SeoMetadata {
                    indexable: true,
                    canonical_path: Some("/en/resources/news/development-news".into()),
                    ..SeoMetadata::default()
                },
                is_placeholder: false,
            },
            category: "Company".into(),
            author_display_name: "AIRTEKPOWER".into(),
            cover_media_id: None,
            published_at: None,
            featured: false,
            data_class: DataClass::DevelopmentFixture,
        };
        normalize_news_ownership(false, Some(DataClass::DevelopmentFixture), &mut input).unwrap();
        validate_news_input(&mut input).unwrap();
        assert_eq!(input.content.kind, ContentKind::News);
        assert_eq!(input.data_class, DataClass::Editorial);
        assert!(!input.content.is_placeholder);
        assert!(input.content.seo.indexable);
    }

    #[test]
    fn admin_news_cannot_create_or_reclaim_development_fixture_ownership() {
        let fixture_input = || NewsDraftInput {
            content: ContentDraftInput {
                kind: ContentKind::News,
                slug: "fixture-ownership".into(),
                locale: "en".into(),
                title: "Fixture ownership".into(),
                summary: None,
                body: RichTextDocument {
                    schema_version: 1,
                    doc: json!({"type":"doc","content":[]}),
                },
                seo: SeoMetadata {
                    canonical_path: Some("/en/resources/news/fixture-ownership".into()),
                    indexable: false,
                    ..SeoMetadata::default()
                },
                is_placeholder: true,
            },
            category: "Company".into(),
            author_display_name: "AIRTEKPOWER".into(),
            cover_media_id: None,
            published_at: None,
            featured: false,
            data_class: DataClass::DevelopmentFixture,
        };

        assert!(normalize_news_ownership(false, None, &mut fixture_input()).is_err());
        assert!(
            normalize_news_ownership(false, Some(DataClass::Editorial), &mut fixture_input(),)
                .is_err()
        );
        assert!(normalize_news_ownership(
            true,
            Some(DataClass::DevelopmentFixture),
            &mut fixture_input(),
        )
        .is_err());
    }

    #[test]
    fn news_rejects_stored_script_content() {
        let mut input = NewsDraftInput {
            content: ContentDraftInput {
                kind: ContentKind::News,
                slug: "unsafe-news".into(),
                locale: "en".into(),
                title: "Unsafe news".into(),
                summary: None,
                body: RichTextDocument {
                    schema_version: 1,
                    doc: json!({"type":"doc","content":[{"type":"script","text":"alert(1)"}]}),
                },
                seo: SeoMetadata {
                    canonical_path: Some("/en/resources/news/unsafe-news".into()),
                    indexable: false,
                    ..SeoMetadata::default()
                },
                is_placeholder: false,
            },
            category: "Company".into(),
            author_display_name: "AIRTEKPOWER".into(),
            cover_media_id: None,
            published_at: None,
            featured: false,
            data_class: DataClass::Editorial,
        };
        assert!(validate_news_input(&mut input).is_err());
    }

    #[test]
    fn general_information_requires_frontend_contract_keys() {
        let input = GeneralInformationDraftInput {
            locale: "en".into(),
            payload: json!({"brandName":"AIRTEKPOWER"}),
            is_placeholder: true,
        };
        assert!(validate_general_information(&input).is_err());
    }

    #[test]
    fn invitation_replay_is_encrypted_and_bound_to_the_idempotent_request() {
        let config = Config::for_test();
        let key = config
            .invitation_replay_encryption_key
            .as_ref()
            .expect("test invitation replay key");
        let raw_token = "test-one-time-token-that-must-not-appear";
        let invitation = UserInvitation {
            id: Uuid::new_v4(),
            email: "invitee@example.com".into(),
            display_name: "Invitee".into(),
            locale: "zh-CN".into(),
            role_keys: vec!["content-editor".into()],
            status: "pending".into(),
            invited_at: Utc::now(),
            expires_at: Utc::now() + Duration::days(7),
            invitation_token: Some(raw_token.into()),
        };
        let aad = b"airtek.invitation-replay.v1|request-a";
        let envelope = seal_invitation_replay(&invitation, key, aad).unwrap();
        let stored = serde_json::to_string(&envelope).unwrap();
        assert!(!stored.contains(raw_token));
        assert_eq!(
            open_invitation_replay(&envelope, key, aad)
                .unwrap()
                .invitation_token
                .as_deref(),
            Some(raw_token)
        );
        assert!(open_invitation_replay(
            &envelope,
            key,
            b"airtek.invitation-replay.v1|different-request"
        )
        .is_err());
    }
}
