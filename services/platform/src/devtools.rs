mod access;
mod audit;
mod limits;
mod protocol;
mod routes;
mod session;
mod token;

use axum::{
    routing::{get, post},
    Router,
};

use crate::{error::ApiError, state::AppState};

pub fn protected_router() -> Router<AppState> {
    Router::new().route("/sessions/token", post(routes::create_terminal_token))
}

pub fn terminal_router() -> Router<AppState> {
    Router::new().route("/terminal", get(routes::open_terminal))
}

pub fn ensure_non_root() -> Result<(), ApiError> {
    access::ensure_non_root()
}
