use airtek_runtime::{
    auth::{
        accept_invitation, confirm_totp_enrollment, list_sessions, login, logout,
        regenerate_recovery_codes, revoke_session_by_id, session, setup, start_totp_enrollment,
    },
    AppState,
};
use axum::{
    routing::{get, post},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/setup", post(setup))
        .route("/auth/login", post(login))
        .route("/auth/invitations/accept", post(accept_invitation))
        .route("/auth/session", get(session))
        .route("/auth/logout", post(logout))
        .route("/auth/totp/enrollment", post(start_totp_enrollment))
        .route("/auth/totp/confirm", post(confirm_totp_enrollment))
        .route(
            "/auth/recovery-codes/regenerate",
            post(regenerate_recovery_codes),
        )
        .route("/auth/sessions", get(list_sessions))
        .route(
            "/auth/sessions/{id}",
            axum::routing::delete(revoke_session_by_id),
        )
}
