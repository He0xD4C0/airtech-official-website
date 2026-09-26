pub mod admin;
pub mod admin_data;
pub mod auth;
pub mod public;
pub mod public_data;
pub mod system;

use axum::{
    extract::{Request, State},
    http::{header, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use airtek_runtime::error::ApiError;
use airtek_runtime::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let request_id = HeaderName::from_static("x-request-id");
    let public_origin = state
        .config
        .public_origin
        .parse::<HeaderValue>()
        .expect("validated public origin");
    let admin_origin = state
        .config
        .admin_origin
        .parse::<HeaderValue>()
        .expect("validated admin origin");

    let protected_admin =
        admin::router().route_layer(middleware::from_fn_with_state(state.clone(), require_admin));
    let admin_routes =
        auth::router()
            .merge(protected_admin)
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                require_admin_origin,
            ));

    let router = Router::new()
        .merge(system::router())
        .nest("/api/public/v1", public::router())
        .nest("/api/admin/v1", admin_routes);

    #[cfg(feature = "devtools")]
    let router = router.nest(
        "/api/devtools/v1",
        crate::devtools::protected_router()
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                require_devtools,
            ))
            .merge(crate::devtools::terminal_router()),
    );

    router
        .fallback(system::not_found)
        .layer(
            CorsLayer::new()
                .allow_origin([public_origin, admin_origin])
                .allow_credentials(true)
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PATCH,
                    Method::PUT,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([
                    header::ACCEPT,
                    header::AUTHORIZATION,
                    header::CONTENT_TYPE,
                    header::IF_MATCH,
                    HeaderName::from_static("idempotency-key"),
                    HeaderName::from_static(airtek_runtime::auth::CSRF_HEADER),
                    HeaderName::from_static("x-totp-code"),
                ])
                .expose_headers([
                    HeaderName::from_static(airtek_runtime::auth::CSRF_HEADER),
                    header::ETAG,
                    header::LOCATION,
                    HeaderName::from_static("x-request-id"),
                ]),
        )
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow, noarchive"),
        ))
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
        .layer(middleware::from_fn_with_state(state.clone(), track_request))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn track_request(State(state): State<AppState>, request: Request, next: Next) -> Response {
    state.request_metrics.begin();
    let response = next.run(request).await;
    state
        .request_metrics
        .finish(response.status().is_server_error());
    response
}

async fn require_admin(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let principal = airtek_runtime::auth::authenticate(&state, request.headers()).await?;
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        airtek_runtime::auth::verify_csrf(request.headers(), &principal)?;
    }
    let permission =
        airtek_runtime::auth::permission_policy(request.uri().path(), request.method())
            .ok_or_else(|| {
                ApiError::forbidden("This admin route has no RBAC policy and is denied by default.")
            })?;
    if !principal.business_access_enabled() {
        return Err(ApiError::forbidden(
            "This account must enable TOTP before accessing Admin business data.",
        ));
    }
    if !permission.allows(&principal) {
        return Err(ApiError::forbidden(permission.denied_detail()));
    }
    request.headers_mut().remove("x-airtek-actor");
    request.headers_mut().insert(
        HeaderName::from_static("x-airtek-authenticated-actor"),
        HeaderValue::from_str(&principal.email)
            .map_err(|_| ApiError::internal("Authenticated actor is not a valid header value."))?,
    );
    request.extensions_mut().insert(principal);
    Ok(next.run(request).await)
}

#[cfg(feature = "devtools")]
async fn require_devtools(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let principal = airtek_runtime::auth::authenticate(&state, request.headers()).await?;
    airtek_runtime::auth::verify_csrf(request.headers(), &principal)?;
    if !principal.has_permission("devtools.shell") {
        return Err(ApiError::forbidden(
            "The `devtools.shell` permission is required.",
        ));
    }
    request.extensions_mut().insert(principal);
    Ok(next.run(request).await)
}

async fn require_admin_origin(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or_default();
        if origin != state.config.admin_origin {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "Origin not allowed",
                "Admin mutations are accepted only from the configured admin origin.",
            ));
        }
    }

    Ok(next.run(request).await)
}

pub fn etag(revision: i64) -> String {
    format!("\"revision-{revision}\"")
}

pub fn parse_if_match(headers: &axum::http::HeaderMap) -> Result<i64, ApiError> {
    let raw = headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::precondition_required("If-Match is required for updates."))?;
    raw.trim_matches('"')
        .strip_prefix("revision-")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| ApiError::bad_request("If-Match must use the current revision ETag."))
}

pub fn actor(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-airtek-authenticated-actor")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("authenticated-admin")
        .to_owned()
}

pub fn accepted<T: serde::Serialize>(status: StatusCode, value: T) -> Response {
    (status, axum::Json(value)).into_response()
}
