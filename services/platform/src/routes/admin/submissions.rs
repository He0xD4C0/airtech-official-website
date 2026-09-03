async fn list_rfqs(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<crate::models::RfqSubmission>>, ApiError> {
    let mut values = state.list_stored_rfqs().await?;
    values.sort_by_key(|submission| Reverse(submission.submitted_at));
    let mut page = paginate_by_id("admin.rfqs", values, query, |entry| entry.id)?;
    if !principal.has_permission("rfq.read_pii") {
        for value in &mut page.items {
            redact_business_contact(&mut value.request.contact);
        }
    }
    Ok(Json(page))
}

async fn list_contacts(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<crate::models::ContactRequest>>, ApiError> {
    let mut values = state.list_stored_contacts().await?;
    values.sort_by_key(|contact| Reverse(contact.submitted_at));
    let mut page = paginate_by_id("admin.contacts", values, query, |entry| entry.id)?;
    if !principal.has_permission("rfq.read_pii") {
        for value in &mut page.items {
            redact_business_contact(&mut value.request.contact);
            value.request.message = "[restricted]".into();
        }
    }
    Ok(Json(page))
}

fn redact_business_contact(contact: &mut crate::models::BusinessContact) {
    contact.name = "[restricted]".into();
    contact.email = "[restricted]".into();
    contact.phone = None;
}
