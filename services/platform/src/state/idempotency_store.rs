use super::*;

impl AppState {
    pub async fn idempotency_replay(
        &self,
        scope: &str,
        key: &str,
        request_hash: &str,
    ) -> Result<Option<IdempotencyReplay>, ApiError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(
                "SELECT request_hash, response_body, response_status FROM idempotency_keys WHERE scope=$1 AND key_hash=$2 AND expires_at > now()",
            )
            .bind(scope)
            .bind(text_hash(key))
            .fetch_optional(pool)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let stored_hash: String = row.try_get("request_hash")?;
            if stored_hash != request_hash {
                return Err(ApiError::conflict(
                    "This Idempotency-Key was already used with a different request body.",
                ));
            }
            let status: i32 = row.try_get("response_status")?;
            let response_status = u16::try_from(status).map_err(|_| {
                ApiError::service_unavailable("Stored idempotency response status is invalid.")
            })?;
            return Ok(Some(IdempotencyReplay {
                response: row.try_get("response_body")?,
                response_status,
            }));
        }

        let mut data = self.data.write().await;
        let record_key = (scope.into(), key.into());
        if data.idempotency.get(&record_key).is_some_and(|stored| {
            stored.created_at + chrono::Duration::hours(IDEMPOTENCY_TTL_HOURS) <= Utc::now()
        }) {
            data.idempotency.remove(&record_key);
        }
        let Some(stored) = data.idempotency.get(&record_key) else {
            return Ok(None);
        };
        if stored.request_hash != request_hash {
            return Err(ApiError::conflict(
                "This Idempotency-Key was already used with a different request body.",
            ));
        }
        Ok(Some(IdempotencyReplay {
            response: stored.response.clone(),
            response_status: stored.response_status,
        }))
    }

    pub(crate) async fn acquire_idempotency_guard(
        &self,
        scope: &str,
        key: &str,
    ) -> Result<IdempotencyGuard, ApiError> {
        let lock_key = text_hash(&format!("{scope}|{key}"));
        let lock = {
            let mut locks = self.idempotency_locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(&lock_key).and_then(Weak::upgrade) {
                lock
            } else {
                if locks.len() >= MAX_IN_MEMORY_IDEMPOTENCY_KEYS {
                    return Err(ApiError::too_many_requests(
                        "Too many idempotent mutations are currently in progress.",
                    ));
                }
                let lock = Arc::new(Mutex::new(()));
                locks.insert(lock_key.clone(), Arc::downgrade(&lock));
                lock
            }
        };
        let memory_guard = lock.lock_owned().await;
        let database_slot = if self.pool.is_some() {
            Some(
                self.idempotency_database_slots
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| {
                        ApiError::service_unavailable("Idempotency coordination stopped.")
                    })?,
            )
        } else {
            None
        };
        let database_transaction = if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(&lock_key)
                .execute(&mut *transaction)
                .await?;
            Some(transaction)
        } else {
            None
        };
        Ok(IdempotencyGuard {
            _memory_guard: memory_guard,
            database_transaction,
            _database_slot: database_slot,
        })
    }

    pub async fn save_idempotency(
        &self,
        scope: &str,
        key: String,
        request_hash: String,
        response: Value,
        response_status: u16,
    ) -> Result<(), ApiError> {
        if let Some(pool) = &self.pool {
            let result = sqlx::query(
                r#"INSERT INTO idempotency_keys
                   (scope, key_hash, request_hash, response_status, response_body)
                   VALUES ($1,$2,$3,$4,$5)
                   ON CONFLICT (scope, key_hash) DO UPDATE SET
                   request_hash=EXCLUDED.request_hash,
                   response_status=EXCLUDED.response_status,
                   response_body=EXCLUDED.response_body,
                   created_at=now(), expires_at=now() + interval '24 hours'
                   WHERE idempotency_keys.expires_at <= now()"#,
            )
            .bind(scope)
            .bind(text_hash(&key))
            .bind(&request_hash)
            .bind(response_status as i32)
            .bind(&response)
            .execute(pool)
            .await?;
            if result.rows_affected() != 1 {
                return Err(ApiError::conflict(
                    "This Idempotency-Key is already active for another request.",
                ));
            }
            return Ok(());
        }
        let mut data = self.data.write().await;
        let now = Utc::now();
        data.idempotency.retain(|_, record| {
            record.created_at + chrono::Duration::hours(IDEMPOTENCY_TTL_HOURS) > now
        });
        if !data.idempotency.contains_key(&(scope.into(), key.clone()))
            && data.idempotency.len() >= MAX_IN_MEMORY_IDEMPOTENCY_KEYS
        {
            return Err(ApiError::too_many_requests(
                "The in-memory idempotency store is at capacity.",
            ));
        }
        data.idempotency.insert(
            (scope.into(), key),
            IdempotencyRecord {
                request_hash,
                response,
                response_status,
                created_at: Utc::now(),
            },
        );
        Ok(())
    }
}
