use super::*;

pub(super) async fn create_session(
    state: &AppState,
    user: StoredUser,
) -> Result<SessionIssue, ApiError> {
    let session_token = random_token();
    let csrf_token = random_token();
    let session_id = Uuid::new_v4();
    let session_token_hash = token_hash(&session_token);
    let csrf_hash = token_hash(&csrf_token);
    let expires_at = Utc::now() + Duration::hours(SESSION_HOURS);
    let pool = &state.pool;
    sqlx::query(
        r#"INSERT INTO sessions
               (id, user_id, token_hash, csrf_hash, created_at, expires_at, last_seen_at)
               VALUES ($1,$2,$3,$4,now(),$5,now())"#,
    )
    .bind(session_id)
    .bind(user.id)
    .bind(&session_token_hash)
    .bind(&csrf_hash)
    .bind(expires_at)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE users SET last_login_at=now(), updated_at=now() WHERE id=$1")
        .bind(user.id)
        .execute(pool)
        .await?;
    Ok(SessionIssue {
        principal: AdminPrincipal {
            user_id: user.id,
            display_name: user.display_name,
            email: user.email,
            role: user.role,
            permissions: user.permissions,
            session_id,
            session_token_hash,
            csrf_hash,
            totp_enabled: user.totp_enabled,
        },
        session_token,
        csrf_token,
    })
}

pub(super) async fn rotate_csrf(
    state: &AppState,
    principal: &AdminPrincipal,
    csrf_token: &str,
) -> Result<(), ApiError> {
    let csrf_hash = token_hash(csrf_token);
    sqlx::query("UPDATE sessions SET csrf_hash=$1, last_seen_at=now() WHERE id=$2")
        .bind(&csrf_hash)
        .bind(principal.session_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

pub(super) async fn revoke_session(
    state: &AppState,
    principal: &AdminPrincipal,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE sessions SET revoked_at=now() WHERE id=$1")
        .bind(principal.session_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

pub(super) fn session_response(
    state: &AppState,
    issue: SessionIssue,
    status: StatusCode,
) -> Response {
    let user = issue.principal.session_user(state);
    let mut response = (status, Json(user)).into_response();
    append_session_cookies(
        state,
        response.headers_mut(),
        &issue.session_token,
        &issue.csrf_token,
    );
    response.headers_mut().insert(
        CSRF_HEADER,
        HeaderValue::from_str(&issue.csrf_token).expect("generated CSRF token is a header value"),
    );
    append_no_store(response.headers_mut());
    response
}

pub(super) fn session_refresh_response(
    state: &AppState,
    principal: AdminPrincipal,
    csrf_token: String,
) -> Response {
    let mut response = Json(principal.session_user(state)).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&csrf_cookie(state, &csrf_token, SESSION_HOURS * 3600))
            .expect("generated CSRF cookie is valid"),
    );
    response.headers_mut().insert(
        CSRF_HEADER,
        HeaderValue::from_str(&csrf_token).expect("generated CSRF token is a header value"),
    );
    append_no_store(response.headers_mut());
    response
}

pub(super) fn sensitive_json<T: Serialize>(value: T) -> Response {
    let mut response = Json(value).into_response();
    append_no_store(response.headers_mut());
    response
}

pub(super) fn append_no_store(headers: &mut HeaderMap) {
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
}

pub(super) fn append_session_cookies(
    state: &AppState,
    headers: &mut HeaderMap,
    session_token: &str,
    csrf_token: &str,
) {
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie(state, session_token, SESSION_HOURS * 3600))
            .expect("generated session cookie is valid"),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&csrf_cookie(state, csrf_token, SESSION_HOURS * 3600))
            .expect("generated CSRF cookie is valid"),
    );
}

pub(super) fn append_clear_cookies(state: &AppState, headers: &mut HeaderMap) {
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie(state, "", 0)).expect("clear cookie is valid"),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&csrf_cookie(state, "", 0)).expect("clear cookie is valid"),
    );
}

pub(super) fn session_cookie(state: &AppState, value: &str, max_age: i64) -> String {
    format!(
        "{SESSION_COOKIE}={value}; Path=/api; Max-Age={max_age}; HttpOnly; SameSite=Strict{}",
        secure_attribute(state)
    )
}

pub(super) fn csrf_cookie(state: &AppState, value: &str, max_age: i64) -> String {
    format!(
        "{CSRF_COOKIE}={value}; Path=/; Max-Age={max_age}; SameSite=Strict{}",
        secure_attribute(state)
    )
}

pub(super) fn secure_attribute(state: &AppState) -> &'static str {
    if state.config.production || state.config.admin_origin.starts_with("https://") {
        "; Secure"
    } else {
        ""
    }
}

pub(super) fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|header| header.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(key, value)| (key == name).then(|| value.to_owned()))
}

pub(super) fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|value| value.to_string())
        .map_err(|_| ApiError::internal("Password hashing failed."))
}

pub(super) fn verify_password(encoded: &str, password: &str) -> bool {
    PasswordHash::new(encoded).ok().is_some_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

pub(super) fn token_hash(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

pub(super) fn hex_digest(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn request_id(headers: &HeaderMap) -> Uuid {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4)
}

pub(super) fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && bool::from(left.ct_eq(right))
}

pub(super) fn random_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub(super) fn request_source(
    state: &AppState,
    headers: &HeaderMap,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
) -> String {
    crate::rate_limit::resolve_client_source(
        headers,
        peer.map(|Extension(ConnectInfo(address))| address.ip()),
        &state.config.trusted_proxy_cidrs,
    )
}
