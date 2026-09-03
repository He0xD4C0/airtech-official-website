use std::{collections::HashMap, time::Instant};

use sha2::{Digest, Sha256};

use crate::state::DevtoolTokenGrant;

pub(super) fn terminal_token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub(super) fn take_terminal_grant(
    tokens: &mut HashMap<String, DevtoolTokenGrant>,
    token_hash: &str,
    now: Instant,
) -> Option<DevtoolTokenGrant> {
    tokens
        .remove(token_hash)
        .filter(|value| value.expires_at > now)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use uuid::Uuid;

    use super::*;
    use crate::devtools::limits::TOKEN_TTL;

    #[test]
    fn raw_tokens_are_not_used_as_registry_keys() {
        let token = "153b08e3e82d4418b54a96906012b451";
        let digest = terminal_token_hash(token);
        assert_ne!(digest, token);
        assert_eq!(digest.len(), 64);
        assert_eq!(digest, terminal_token_hash(token));
    }

    #[test]
    fn grants_are_one_time_and_expired_grants_fail_closed() {
        let now = Instant::now();
        let grant = DevtoolTokenGrant {
            expires_at: now + TOKEN_TTL,
            session_id: Uuid::new_v4(),
            actor: "developer@example.com".into(),
            user_id: Uuid::new_v4(),
            admin_session_id: Uuid::new_v4(),
        };
        let mut tokens = HashMap::from([("valid".into(), grant.clone())]);
        assert_eq!(
            take_terminal_grant(&mut tokens, "valid", now)
                .unwrap()
                .session_id,
            grant.session_id
        );
        assert!(take_terminal_grant(&mut tokens, "valid", now).is_none());

        tokens.insert(
            "expired".into(),
            DevtoolTokenGrant {
                expires_at: now - Duration::from_secs(1),
                ..grant
            },
        );
        assert!(take_terminal_grant(&mut tokens, "expired", now).is_none());
        assert!(!tokens.contains_key("expired"));
    }
}
