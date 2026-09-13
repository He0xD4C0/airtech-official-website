use super::*;

impl AppState {
    pub fn new(config: Config) -> Result<Self, ApiError> {
        if config.production && config.database_url.is_none() {
            return Err(ApiError::service_unavailable(
                "PostgreSQL persistence is required in production.",
            ));
        }
        let pool = match &config.database_url {
            Some(database_url) => Some(
                PgPoolOptions::new()
                    .max_connections(10)
                    .connect_lazy(database_url)
                    .map_err(|_| ApiError::bad_request("DATABASE_URL is invalid."))?,
            ),
            None => None,
        };

        Ok(Self {
            config: Arc::new(config),
            pool,
            data: Arc::new(RwLock::new(PlatformData::default())),
            auth_hash_slots: Arc::new(Semaphore::new(4)),
            request_metrics: Arc::new(RequestMetrics::default()),
            idempotency_locks: Arc::new(Mutex::new(HashMap::new())),
            // A guard holds one PostgreSQL transaction while the business
            // mutation uses other pool connections. Keep ample pool headroom.
            idempotency_database_slots: Arc::new(Semaphore::new(4)),
            #[cfg(feature = "devtools")]
            devtool_tokens: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "devtools")]
            devtool_session_slots: Arc::new(Semaphore::new(4)),
        })
    }

    pub fn for_test() -> Self {
        Self::new(Config::for_test()).expect("test configuration is valid")
    }

    pub fn environment_label(&self) -> &'static str {
        if self.config.production {
            "production"
        } else {
            "development"
        }
    }

    pub async fn check_persistence(&self) -> Result<String, ApiError> {
        match &self.pool {
            Some(pool) => {
                let (schema_ready, history_ready) = sqlx::query_as::<_, (bool, bool)>(
                    r#"SELECT to_regclass('public.content_entries') IS NOT NULL,
                              to_regclass('public.flyway_schema_history') IS NOT NULL"#,
                )
                .fetch_one(pool)
                .await?;
                if !schema_ready || !history_ready {
                    return Err(ApiError::service_unavailable(
                        "PostgreSQL is reachable but Flyway migrations are not current.",
                    ));
                }
                let migration_status = crate::flyway::read_status(pool).await?;
                if !migration_status.is_current() {
                    return Err(ApiError::service_unavailable(
                        "PostgreSQL is reachable but Flyway migrations are not current.",
                    ));
                }
                Ok("postgresql".into())
            }
            None if self.config.production => Err(ApiError::service_unavailable(
                "PostgreSQL persistence is required in production.",
            )),
            None => Ok("inMemory".into()),
        }
    }
}
