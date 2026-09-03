async fn analytics_summary(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    if let Some(pool) = &state.pool {
        let accepted_event_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM analytics_events")
                .fetch_one(pool)
                .await?;
        let rfq_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM rfq_submissions")
            .fetch_one(pool)
            .await?;
        let contact_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM contact_requests")
            .fetch_one(pool)
            .await?;
        return Ok(Json(json!({
            "acceptedEventCount": accepted_event_count,
            "rfqCount": rfq_count,
            "contactCount": contact_count,
            "containsPii": false,
            "source": "firstParty"
        })));
    }
    let data = state.data.read().await;
    Ok(Json(json!({
        "acceptedEventCount": data.analytics_receipts.values().filter(|event| event.accepted).count(),
        "rfqCount": data.rfqs.len(),
        "contactCount": data.contacts.len(),
        "containsPii": false,
        "source": "firstParty"
    })))
}
