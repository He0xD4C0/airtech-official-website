use super::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DeleteRoleQuery {
    reason: String,
}

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
    Extension(principal): Extension<AdminPrincipal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(update): Json<UpdateAdminRole>,
) -> Result<Response, ApiError> {
    validate_reason(&update.reason)?;
    if !principal.is_super_admin() {
        return Err(ApiError::forbidden(
            "Only a Super Admin can change administrator roles.",
        ));
    }
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

pub(super) async fn create_role(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(input): Json<CreateAdminRole>,
) -> Result<Response, ApiError> {
    if !principal.is_super_admin() {
        return Err(ApiError::forbidden(
            "Only a Super Admin can create administrator roles.",
        ));
    }
    validate_reason(&input.reason)?;
    let key = input.key.trim().to_ascii_lowercase();
    if key.len() < 2
        || key.len() > 64
        || !key
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(ApiError::bad_request(
            "key must be 2 to 64 lowercase letters, digits or dashes and start with a letter.",
        ));
    }
    let display_name = input.display_name.trim().to_owned();
    if display_name.is_empty() || display_name.len() > 120 {
        return Err(ApiError::bad_request(
            "displayName must contain 1 to 120 characters.",
        ));
    }
    let idempotency_request =
        json!({"key": key, "displayName": display_name, "permissions": &input.permissions});
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.role.create",
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
    let role_id = Uuid::new_v4();
    let mut transaction = state.pool.begin().await?;
    airtek_runtime::services::identity::validate_permissions(&mut transaction, &input.permissions)
        .await?;
    let existing =
        airtek_runtime::services::identity::role_key_exists(&mut transaction, &key).await?;
    if existing {
        transaction.rollback().await?;
        return Err(ApiError::conflict("A role with this key already exists."));
    }
    airtek_runtime::services::identity::insert_role(
        &mut transaction,
        role_id,
        &key,
        &display_name,
        &input.permissions,
        principal.user_id,
    )
    .await?;
    let created = load_admin_role_in_transaction(&mut transaction, role_id).await?;
    let audit = mutation_audit_event(
        &headers,
        "identity.role.create",
        "role",
        Some(role_id),
        None,
        Some(json!(created)),
        Some(input.reason),
    );
    airtek_runtime::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &created, StatusCode::CREATED)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    Ok(entity_response(
        StatusCode::CREATED,
        &created,
        created.revision,
    ))
}

pub(super) async fn delete_role(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(id): Path<Uuid>,
    Query(query): Query<DeleteRoleQuery>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if !principal.is_super_admin() {
        return Err(ApiError::forbidden(
            "Only a Super Admin can delete administrator roles.",
        ));
    }
    validate_reason(&query.reason)?;
    let expected = parse_if_match(&headers)?;
    let before = load_admin_role(&state, id).await?;
    if before.revision != expected {
        return Err(ApiError::conflict(
            "The role changed; reload before deleting.",
        ));
    }
    if before.is_preset || before.key == "super-admin" {
        return Err(ApiError::forbidden(
            "Preset administrator roles cannot be deleted.",
        ));
    }
    let mut transaction = state.pool.begin().await?;
    let assigned =
        airtek_runtime::services::identity::role_assignment_count(&mut transaction, id).await?;
    if assigned > 0 {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "Reassign every administrator before deleting this role.",
        ));
    }
    if airtek_runtime::services::identity::delete_role_record(&mut transaction, id, expected)
        .await?
        != 1
    {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "The role changed; reload before deleting.",
        ));
    }
    let audit = mutation_audit_event(
        &headers,
        "identity.role.delete",
        "role",
        Some(id),
        Some(json!(before)),
        None,
        Some(query.reason),
    );
    airtek_runtime::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
