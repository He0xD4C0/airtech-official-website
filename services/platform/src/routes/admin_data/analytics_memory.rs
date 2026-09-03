async fn aggregate_memory_sources(
    state: &AppState,
    from: Option<chrono::DateTime<Utc>>,
    to: Option<chrono::DateTime<Utc>>,
) -> Vec<GuestSourceDaily> {
    let mut values = BTreeMap::new();
    for visit in state.data.read().await.guest_visits.values() {
        let bucket_date = visit.first_seen_at.date_naive();
        if from
            .map(|from| bucket_date < from.date_naive())
            .unwrap_or(false)
            || to.map(|to| bucket_date >= to.date_naive()).unwrap_or(false)
        {
            continue;
        }
        *values
            .entry((
                bucket_date,
                visit.source.clone(),
                visit.referrer_domain.clone(),
                visit.medium.clone(),
                visit.campaign.clone(),
                visit.landing_path.clone(),
                visit
                    .landing_path
                    .trim_start_matches('/')
                    .split('/')
                    .next()
                    .unwrap_or("en")
                    .to_owned(),
            ))
            .or_default() += 1;
    }
    values
        .into_iter()
        .map(
            |(
                (bucket_date, source, referrer_domain, medium, campaign, landing_path, locale),
                visits,
            )| GuestSourceDaily {
                bucket_date,
                source_name: Some(referrer_domain.clone().unwrap_or_else(|| source.clone())),
                referrer_domain,
                source,
                utm_source: None,
                medium,
                campaign,
                landing_path,
                locale,
                visits,
                page_views: 0,
                rfq_starts: 0,
                rfq_submissions: 0,
            },
        )
        .collect()
}
