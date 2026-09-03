fn shell_fixture(
    suffix: u128,
    fixture_key: &'static str,
    kind: &'static str,
    slug: &'static str,
    title: &'static str,
    attrs: Value,
) -> ContentFixture {
    ContentFixture {
        fixture_key,
        ledger_entity_type: "content",
        id: fixture_id(suffix),
        kind,
        slug,
        title,
        summary: "Development-only site shell configuration.",
        canonical_path: None,
        page_slots: attrs,
        news: None,
    }
}

fn fixture_id(suffix: u128) -> Uuid {
    Uuid::from_u128(0xd300_0000_0000_4000_8000_0000_0000_0000 | suffix)
}

fn route_id(entity_id: Uuid) -> Uuid {
    Uuid::from_u128(entity_id.as_u128() ^ 0x0000_0000_0000_0000_1000_0000_0000_0000)
}

fn checksum(value: &Value) -> String {
    let digest = Sha256::digest(serde_json::to_vec(value).expect("fixture JSON is serializable"));
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn fixture_timestamp() -> Result<DateTime<Utc>, DevelopmentSeedError> {
    parse_timestamp(FIXTURE_UPDATED_AT)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, DevelopmentSeedError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| DevelopmentSeedError::InvalidFixtureTimestamp)
}
