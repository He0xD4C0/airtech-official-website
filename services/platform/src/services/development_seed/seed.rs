/// Populate only development-owned CMS fixtures. Product Master records are
/// deliberately outside this service and can only enter through validated
/// import/publishing paths.
pub async fn seed(
    pool: &PgPool,
    actor: &str,
) -> Result<DevelopmentSeedReport, DevelopmentSeedError> {
    let fixtures = content_fixtures();
    let news_count = fixtures
        .iter()
        .filter(|fixture| fixture.news.is_some())
        .count();
    let route_count = fixtures
        .iter()
        .filter(|fixture| fixture.canonical_path.is_some())
        .count();
    let mut transaction = pool.begin().await?;
    // Serialize concurrent CLI invocations so the first revision and ledger
    // ownership checks remain deterministic instead of racing on unique keys.
    sqlx::query("SELECT pg_advisory_xact_lock(731020260902::bigint)")
        .execute(&mut *transaction)
        .await?;
    let mut changed_fixtures = 0;
    let mut unchanged_fixtures = 0;

    for fixture in &fixtures {
        match seed_content(&mut transaction, fixture, actor).await? {
            ChangeState::Changed => changed_fixtures += 1,
            ChangeState::Unchanged => unchanged_fixtures += 1,
        }
    }

    match seed_general_information(&mut transaction, actor).await? {
        ChangeState::Changed => changed_fixtures += 1,
        ChangeState::Unchanged => unchanged_fixtures += 1,
    }

    sqlx::query(
        r#"INSERT INTO audit_log
           (id, actor, action, entity_type, entity_id, before_value,
            after_value, reason, request_id, occurred_at)
           VALUES ($1,$2,'devtools.cli.seed.completed','developmentFixture',
                   NULL,NULL,$3,$4,$5,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(actor)
    .bind(json!({
        "seedVersion": SEED_VERSION,
        "fixtureCount": fixtures.len() + 1,
        "newsCount": news_count,
        "routeCount": route_count,
        "changedFixtures": changed_fixtures,
        "unchangedFixtures": unchanged_fixtures,
        "productsCreated": 0
    }))
    .bind("Idempotent development CMS fixtures loaded; Product Master was not modified.")
    .bind(Uuid::new_v4())
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(DevelopmentSeedReport {
        status: "completed",
        seed_version: SEED_VERSION,
        fixture_count: fixtures.len() + 1,
        news_count,
        route_count,
        changed_fixtures,
        unchanged_fixtures,
        products_created: 0,
    })
}
