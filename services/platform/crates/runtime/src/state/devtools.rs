use std::time::Instant;

use tokio::sync::OwnedSemaphorePermit;

use super::{ApiError, AppState, DevtoolTokenGrant};

impl AppState {
    pub fn has_devtool_session_capacity(&self) -> bool {
        self.devtool_session_slots.available_permits() > 0
    }

    pub fn acquire_devtool_session(&self) -> Result<OwnedSemaphorePermit, ApiError> {
        self.devtool_session_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                ApiError::too_many_requests(
                    "The development terminal session limit has been reached.",
                )
            })
    }

    pub async fn issue_devtool_grant(
        &self,
        token_hash: String,
        grant: DevtoolTokenGrant,
        maximum_pending: usize,
    ) -> Result<(), ApiError> {
        let mut tokens = self.devtool_tokens.write().await;
        let now = Instant::now();
        tokens.retain(|_, value| value.expires_at > now);
        if tokens.len() >= maximum_pending {
            return Err(ApiError::too_many_requests(
                "Too many unused development terminal authorizations are pending.",
            ));
        }
        tokens.insert(token_hash, grant);
        Ok(())
    }

    pub async fn revoke_devtool_grant(&self, token_hash: &str) {
        self.devtool_tokens.write().await.remove(token_hash);
    }

    pub async fn take_devtool_grant(&self, token_hash: &str) -> Option<DevtoolTokenGrant> {
        self.devtool_tokens
            .write()
            .await
            .remove(token_hash)
            .filter(|grant| grant.expires_at > Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use uuid::Uuid;
    #[tokio::test]
    async fn grants_are_one_time_and_expired_grants_fail_closed() {
        let mut config = crate::Config::for_test();
        config.database_url = Some("postgres://unused:unused@127.0.0.1:1/unused".into());
        let state = AppState::new(config).unwrap();
        let now = Instant::now();
        let grant = DevtoolTokenGrant {
            expires_at: now + Duration::from_secs(30),
            session_id: Uuid::new_v4(),
            actor: "developer@example.com".into(),
            user_id: Uuid::new_v4(),
            admin_session_id: Uuid::new_v4(),
        };
        state
            .issue_devtool_grant("valid".into(), grant.clone(), 1)
            .await
            .unwrap();
        assert!(state
            .issue_devtool_grant("overflow".into(), grant.clone(), 1)
            .await
            .is_err());
        assert_eq!(
            state.take_devtool_grant("valid").await.unwrap().session_id,
            grant.session_id
        );
        assert!(state.take_devtool_grant("valid").await.is_none());

        state
            .issue_devtool_grant(
                "expired".into(),
                DevtoolTokenGrant {
                    expires_at: now - Duration::from_secs(1),
                    ..grant.clone()
                },
                1,
            )
            .await
            .unwrap();
        assert!(state.take_devtool_grant("expired").await.is_none());
        assert!(state.take_devtool_grant("expired").await.is_none());
        state
            .issue_devtool_grant("revoked".into(), grant, 1)
            .await
            .unwrap();
        state.revoke_devtool_grant("revoked").await;
        assert!(state.take_devtool_grant("revoked").await.is_none());
    }
}
