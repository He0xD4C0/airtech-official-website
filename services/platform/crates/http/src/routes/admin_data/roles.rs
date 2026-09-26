use super::*;

pub(super) async fn list_roles(
    State(state): State<AppState>,
    Query(query): Query<IdentityListQuery>,
) -> Result<Json<airtek_domain::models::AdminRolePage>, ApiError> {
    let search = identity_query_text(query.q)?.map(|value| value.to_lowercase());
    Ok(Json(
        airtek_runtime::services::identity::list_roles(
            &state,
            search.as_deref(),
            CursorQuery {
                cursor: query.cursor,
                limit: query.limit,
            },
        )
        .await?,
    ))
}

pub(super) async fn get_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let role = load_admin_role(&state, id).await?;
    Ok(entity_response(StatusCode::OK, &role, role.revision))
}

pub(super) async fn update_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(update): Json<UpdateAdminRole>,
) -> Result<Response, ApiError> {
    validate_reason(&update.reason)?;
    let expected = parse_if_match(&headers)?;
    if let Some(display_name) = update.display_name.as_deref() {
        if display_name.trim().is_empty() || display_name.len() > 120 {
            return Err(ApiError::bad_request(
                "displayName must contain 1 to 120 characters.",
            ));
        }
    }
    let idempotency_request = json!({"roleId": id, "update": &update});
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.role.update",
        &headers,
        &idempotency_request,
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let role: AdminRoleRecord = replay.decode()?;
            return Ok(entity_response(status, &role, role.revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_admin_role(&state, id).await?;
    if before.revision != expected {
        return Err(ApiError::conflict(
            "The role changed; reload before saving.",
        ));
    }
    if before.key == "super-admin" {
        return Err(ApiError::forbidden(
            "The Super Admin role template is protected from permission edits.",
        ));
    }
    let pool = &state.pool;
    let mut transaction = pool.begin().await?;
    if let Some(permissions) = &update.permissions {
        airtek_runtime::services::identity::validate_permissions(&mut transaction, permissions)
            .await?;
    }
    if airtek_runtime::services::identity::update_role_record(
        &mut transaction,
        id,
        &update,
        expected,
    )
    .await?
        != 1
    {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "The role changed; reload before saving.",
        ));
    }
    if let Some(permissions) = &update.permissions {
        airtek_runtime::services::identity::replace_role_permissions(
            &mut transaction,
            id,
            permissions,
        )
        .await?;
    }
    let after = load_admin_role_in_transaction(&mut transaction, id).await?;
    let audit = mutation_audit_event(
        &headers,
        "identity.role.update",
        "role",
        Some(id),
        Some(json!(before)),
        Some(json!(after)),
        Some(update.reason),
    );
    airtek_runtime::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &after, StatusCode::OK)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    Ok(entity_response(StatusCode::OK, &after, after.revision))
}
