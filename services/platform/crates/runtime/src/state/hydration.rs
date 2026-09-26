use super::*;

impl AppState {
    /// Constant-time startup guard. Data repair and backfills belong to the
    /// one-shot `airtek-maintenance prepare-runtime` command.
    pub async fn verify_runtime_ready(&self) -> Result<(), ApiError> {
        crate::services::runtime_preparation::verify(&self.pool).await
    }
}
