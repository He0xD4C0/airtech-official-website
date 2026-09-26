use std::time::Duration;

pub(super) const TOKEN_TTL: Duration = Duration::from_secs(60);
pub(super) const MAX_PENDING_TOKENS: usize = 32;
pub(super) const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
pub(super) const ABSOLUTE_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
pub(super) const MAX_INPUT_BYTES: usize = 64 * 1024;
pub(super) const MAX_SESSION_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const MAX_ROWS: u16 = 240;
pub(super) const MAX_COLS: u16 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_limits_are_finite_and_ordered() {
        let token_ttl = std::hint::black_box(TOKEN_TTL);
        let idle_timeout = std::hint::black_box(IDLE_TIMEOUT);
        let absolute_timeout = std::hint::black_box(ABSOLUTE_TIMEOUT);
        let max_pending_tokens = std::hint::black_box(MAX_PENDING_TOKENS);
        let max_input_bytes = std::hint::black_box(MAX_INPUT_BYTES);
        let max_output_bytes = std::hint::black_box(MAX_SESSION_OUTPUT_BYTES);
        assert!(token_ttl < idle_timeout);
        assert!(idle_timeout < absolute_timeout);
        assert!(max_pending_tokens > 4);
        assert!(max_input_bytes <= 64 * 1024);
        assert!(max_output_bytes <= 64 * 1024 * 1024);
    }
}
