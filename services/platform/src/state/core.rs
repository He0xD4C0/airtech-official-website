use super::*;

impl AppState {
    pub fn new(config: Config) -> Result<Self, ApiError> {
        let database_url = config
            .database_url
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("PostgreSQL persistence is required."))?;
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect_lazy(database_url)
            .map_err(|_| ApiError::bad_request("DATABASE_URL is invalid."))?;

        Ok(Self::with_pool(config, pool))
    }

    pub fn with_pool(config: Config, pool: PgPool) -> Self {
        let feishu_client = Arc::new(crate::services::feishu::FeishuClient::new(&config));
        Self {
            config: Arc::new(config),
            pool,
            auth_hash_slots: Arc::new(Semaphore::new(4)),
            request_metrics: Arc::new(RequestMetrics::default()),
            feishu_client,
            idempotency_locks: Arc::new(Mutex::new(HashMap::new())),
            // A guard holds one PostgreSQL transaction while the business
            // mutation uses other pool connections. Keep ample pool headroom.
            idempotency_database_slots: Arc::new(Semaphore::new(4)),
            #[cfg(feature = "devtools")]
            devtool_tokens: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "devtools")]
            devtool_session_slots: Arc::new(Semaphore::new(4)),
        }
    }

    pub fn environment_label(&self) -> &'static str {
        if self.config.production {
            "production"
        } else {
            "development"
        }
    }

    pub async fn check_persistence(&self) -> Result<String, ApiError> {
        crate::services::runtime_preparation::verify(&self.pool).await?;
        Ok("postgresql".into())
    }
}
