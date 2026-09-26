#[cfg(test)]
mod cases {
    use super::super::*;
    use airtek_runtime::config::Config;

    #[test]
    pub(super) fn invitation_replay_is_encrypted_and_bound_to_the_idempotent_request() {
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
