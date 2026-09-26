use sha2::{Digest, Sha256};

pub(super) fn terminal_token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_tokens_are_not_used_as_registry_keys() {
        let token = "153b08e3e82d4418b54a96906012b451";
        let digest = terminal_token_hash(token);
        assert_ne!(digest, token);
        assert_eq!(digest.len(), 64);
        assert_eq!(digest, terminal_token_hash(token));
    }
}
