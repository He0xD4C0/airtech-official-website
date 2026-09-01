use std::{collections::HashMap, net::IpAddr};

use axum::http::HeaderMap;
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};

use crate::{config::IpCidr, error::ApiError, state::AppState};

const MAX_FORWARDED_CHAIN_BYTES: usize = 1_024;
const MAX_FORWARDED_HOPS: usize = 16;
pub(crate) const MAX_IN_MEMORY_RATE_LIMIT_KEYS: usize = 4_096;

#[derive(Clone, Copy)]
pub struct PublicRateLimitPolicy {
    scope: &'static str,
    limit: u32,
    window_seconds: i64,
}

pub const CONTACT_POLICY: PublicRateLimitPolicy = PublicRateLimitPolicy {
    scope: "public.contact",
    limit: 5,
    window_seconds: 60 * 60,
};

pub const RFQ_POLICY: PublicRateLimitPolicy = PublicRateLimitPolicy {
    scope: "public.rfq",
    limit: 5,
    window_seconds: 60 * 60,
};

pub const ANALYTICS_POLICY: PublicRateLimitPolicy = PublicRateLimitPolicy {
    scope: "public.analytics",
    limit: 120,
    window_seconds: 60,
};

pub const ANALYTICS_CONSENT_POLICY: PublicRateLimitPolicy = PublicRateLimitPolicy {
    scope: "public.analyticsConsent",
    limit: 20,
    window_seconds: 60 * 60,
};

#[derive(Clone, Debug)]
pub struct InMemoryRateLimit {
    request_count: u32,
    expires_at: DateTime<Utc>,
}

pub async fn enforce_public_rate_limit(
    state: &AppState,
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    policy: PublicRateLimitPolicy,
) -> Result<(), ApiError> {
    let source = resolve_client_source(headers, peer, &state.config.trusted_proxy_cidrs);
    let source_hash = format!(
        "{:x}",
        Sha256::digest(format!("{}|{source}", policy.scope).as_bytes())
    );
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        // The expiry index keeps the durable key space bounded to recently
        // active sources without storing a raw client address.
        sqlx::query("DELETE FROM public_rate_limits WHERE expires_at <= now()")
            .execute(&mut *transaction)
            .await?;
        let accepted = sqlx::query_scalar::<_, i32>(
            r#"INSERT INTO public_rate_limits
               (scope, source_hash, window_started_at, request_count, expires_at)
               VALUES ($1,$2,now(),1,now() + ($3::bigint * interval '1 second'))
               ON CONFLICT (scope, source_hash) DO UPDATE
               SET request_count=public_rate_limits.request_count + 1
               WHERE public_rate_limits.expires_at > now()
                 AND public_rate_limits.request_count < $4
               RETURNING request_count"#,
        )
        .bind(policy.scope)
        .bind(&source_hash)
        .bind(policy.window_seconds)
        .bind(policy.limit as i32)
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        if accepted.is_none() {
            return Err(rate_limit_error());
        }
        return Ok(());
    }

    let now = Utc::now();
    let mut data = state.data.write().await;
    let accepted = consume_in_memory(
        &mut data.public_rate_limits,
        (policy.scope.to_owned(), source_hash),
        policy.limit,
        Duration::seconds(policy.window_seconds),
        now,
    );
    accepted.then_some(()).ok_or_else(rate_limit_error)
}

fn consume_in_memory(
    limits: &mut HashMap<(String, String), InMemoryRateLimit>,
    key: (String, String),
    limit: u32,
    window: Duration,
    now: DateTime<Utc>,
) -> bool {
    limits.retain(|_, entry| entry.expires_at > now);
    if let Some(entry) = limits.get_mut(&key) {
        if entry.request_count >= limit {
            return false;
        }
        entry.request_count += 1;
        return true;
    }
    if limits.len() >= MAX_IN_MEMORY_RATE_LIMIT_KEYS {
        return false;
    }
    limits.insert(
        key,
        InMemoryRateLimit {
            request_count: 1,
            expires_at: now + window,
        },
    );
    true
}

pub(crate) fn resolve_client_source(
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    trusted_proxy_cidrs: &[IpCidr],
) -> String {
    let Some(peer) = peer else {
        // Test harnesses and non-TCP embedding do not get to manufacture a
        // source from request headers; they share a deliberately safe bucket.
        return "peer-unavailable".into();
    };
    if !is_trusted_proxy(peer, trusted_proxy_cidrs) {
        return peer.to_string();
    }
    let Some(chain) = parse_forwarded_chain(headers) else {
        return peer.to_string();
    };
    chain
        .into_iter()
        .rev()
        .find(|address| !is_trusted_proxy(*address, trusted_proxy_cidrs))
        .unwrap_or(peer)
        .to_string()
}

fn is_trusted_proxy(address: IpAddr, trusted_proxy_cidrs: &[IpCidr]) -> bool {
    trusted_proxy_cidrs
        .iter()
        .any(|network| network.contains(address))
}

fn parse_forwarded_chain(headers: &HeaderMap) -> Option<Vec<IpAddr>> {
    let value = headers.get("x-forwarded-for")?.to_str().ok()?;
    if value.len() > MAX_FORWARDED_CHAIN_BYTES {
        return None;
    }
    let values: Vec<_> = value.split(',').map(str::trim).collect();
    if values.is_empty() || values.len() > MAX_FORWARDED_HOPS {
        return None;
    }
    values
        .into_iter()
        .map(|value| value.parse::<IpAddr>().ok())
        .collect()
}

fn rate_limit_error() -> ApiError {
    ApiError::too_many_requests(
        "This public endpoint has received too many requests from the same source; try again later.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(forwarded_for: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", forwarded_for.parse().unwrap());
        headers
    }

    #[test]
    fn untrusted_peer_cannot_spoof_forwarded_source() {
        let source = resolve_client_source(
            &headers("203.0.113.99"),
            Some("198.51.100.20".parse().unwrap()),
            &["172.28.0.10/32".parse().unwrap()],
        );
        assert_eq!(source, "198.51.100.20");
    }

    #[test]
    fn trusted_proxy_chain_is_walked_from_the_nearest_hop() {
        let trusted = [
            "172.28.0.10/32".parse().unwrap(),
            "10.0.0.0/24".parse().unwrap(),
        ];
        let source = resolve_client_source(
            &headers("192.0.2.123, 203.0.113.7, 10.0.0.8"),
            Some("172.28.0.10".parse().unwrap()),
            &trusted,
        );
        assert_eq!(source, "203.0.113.7");
    }

    #[test]
    fn malformed_forwarded_chain_falls_back_to_direct_peer() {
        let source = resolve_client_source(
            &headers("203.0.113.7, not-an-ip"),
            Some("172.28.0.10".parse().unwrap()),
            &["172.28.0.10/32".parse().unwrap()],
        );
        assert_eq!(source, "172.28.0.10");
    }

    #[test]
    fn in_memory_key_space_is_hard_bounded() {
        let now = Utc::now();
        let mut limits = HashMap::new();
        for index in 0..MAX_IN_MEMORY_RATE_LIMIT_KEYS {
            assert!(consume_in_memory(
                &mut limits,
                ("scope".into(), format!("source-{index}")),
                1,
                Duration::hours(1),
                now,
            ));
        }
        assert!(!consume_in_memory(
            &mut limits,
            ("scope".into(), "one-too-many".into()),
            1,
            Duration::hours(1),
            now,
        ));
        assert_eq!(limits.len(), MAX_IN_MEMORY_RATE_LIMIT_KEYS);
    }
}
