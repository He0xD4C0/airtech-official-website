use airtek_runtime::{
    auth::{
        accept_invitation, attempt, change_password, confirm_phone_verification,
        confirm_recovery_key, confirm_totp_enrollment, identify, list_sessions, login, logout,
        recover_with_key, recovery_key_state, revoke_session_by_id, session,
        start_phone_verification, start_totp_enrollment, verify,
    },
    AppState,
};
use axum::{
    routing::{get, post},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/identify", post(identify))
        .route("/auth/attempt", post(attempt))
        .route("/auth/verify", post(verify))
        .route("/auth/password", post(change_password))
        .route("/auth/recovery", post(recover_with_key))
        .route("/auth/recovery-key", get(recovery_key_state))
        .route("/auth/recovery-key/confirm", post(confirm_recovery_key))
        .route("/auth/phone/verification", post(start_phone_verification))
        .route("/auth/phone/confirm", post(confirm_phone_verification))
        .route("/auth/invitations/accept", post(accept_invitation))
        .route("/auth/session", get(session))
        .route("/auth/logout", post(logout))
        .route("/auth/totp/enrollment", post(start_totp_enrollment))
        .route("/auth/totp/confirm", post(confirm_totp_enrollment))
        .route("/auth/sessions", get(list_sessions))
        .route(
            "/auth/sessions/{id}",
            axum::routing::delete(revoke_session_by_id),
        )
}
