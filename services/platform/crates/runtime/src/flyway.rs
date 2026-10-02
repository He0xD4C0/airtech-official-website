use sqlx::PgPool;

include!(concat!(env!("OUT_DIR"), "/flyway_version.rs"));

pub const LEGACY_SQLX_BASELINE_VERSION: i64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlywayStatus {
    pub current_version: Option<i64>,
    pub failed_migrations: i64,
    pub covered_required_versions: i64,
    pub legacy_baseline_present: bool,
}

impl FlywayStatus {
    pub fn is_current(&self) -> bool {
        self.is_current_through(REQUIRED_SCHEMA_VERSION)
    }

    /// True when the database was migrated by a newer application image. The
    /// binary must refuse to serve instead of running against a schema it does
    /// not know, and the operator must roll forward or restore the retained
    /// pre-migration dump.
    pub fn is_ahead_of(&self, required_version: i64) -> bool {
        self.current_version
            .is_some_and(|version| version > required_version)
    }

    pub fn is_current_through(&self, required_version: i64) -> bool {
        if !(1..=REQUIRED_SCHEMA_VERSION).contains(&required_version) {
            return false;
        }
        self.current_version
            .is_some_and(|version| version == required_version)
            && self.failed_migrations == 0
            && self.covered_required_versions >= required_version
    }
}

pub async fn read_status(pool: &PgPool) -> Result<FlywayStatus, sqlx::Error> {
    let (
        current_version,
        failed_migrations,
        covered_required_versions,
        legacy_baseline_present,
    ) = sqlx::query_as::<_, (Option<i64>, i64, i64, bool)>(
            r#"WITH normalized AS (
                   SELECT success,
                          "type" AS migration_type,
                          CASE
                              WHEN version ~ '^[0-9]+$' THEN version::bigint
                              ELSE NULL
                          END AS numeric_version
                   FROM public.flyway_schema_history
               ), summary AS (
                   SELECT max(numeric_version) FILTER (WHERE success IS TRUE) AS current_version,
                          count(*) FILTER (WHERE success IS DISTINCT FROM TRUE) AS failed_migrations,
                          coalesce(
                              bool_or(
                                  success IS TRUE
                                  AND migration_type = 'BASELINE'
                                  AND numeric_version = $2
                              ),
                              false
                          ) AS legacy_baseline_present
                   FROM normalized
               )
               SELECT summary.current_version,
                      summary.failed_migrations,
                      CASE
                          WHEN summary.legacy_baseline_present THEN
                              least($1, $2)
                              + count(DISTINCT normalized.numeric_version) FILTER (
                                  WHERE normalized.success IS TRUE
                                    AND normalized.migration_type = 'SQL'
                                    AND normalized.numeric_version > $2
                                    AND normalized.numeric_version <= $1
                              )
                          ELSE count(DISTINCT normalized.numeric_version) FILTER (
                              WHERE normalized.success IS TRUE
                                AND normalized.migration_type = 'SQL'
                                AND normalized.numeric_version BETWEEN 1 AND $1
                          )
                      END,
                      summary.legacy_baseline_present
               FROM summary
               LEFT JOIN normalized ON true
               GROUP BY summary.current_version,
                        summary.failed_migrations,
                        summary.legacy_baseline_present"#,
        )
        .bind(REQUIRED_SCHEMA_VERSION)
        .bind(LEGACY_SQLX_BASELINE_VERSION)
        .fetch_one(pool)
        .await?;

    Ok(FlywayStatus {
        current_version,
        failed_migrations,
        covered_required_versions,
        legacy_baseline_present,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_complete_successful_history_is_current() {
        let current = FlywayStatus {
            current_version: Some(REQUIRED_SCHEMA_VERSION),
            failed_migrations: 0,
            covered_required_versions: REQUIRED_SCHEMA_VERSION,
            legacy_baseline_present: false,
        };
        assert!(current.is_current());

        assert!(FlywayStatus {
            legacy_baseline_present: true,
            ..current.clone()
        }
        .is_current());

        assert!(!FlywayStatus {
            current_version: Some(0),
            failed_migrations: 0,
            covered_required_versions: 0,
            legacy_baseline_present: false,
        }
        .is_current());
        assert!(!FlywayStatus {
            failed_migrations: 1,
            ..current.clone()
        }
        .is_current());
        assert!(!FlywayStatus {
            covered_required_versions: REQUIRED_SCHEMA_VERSION - 1,
            ..current
        }
        .is_current());
        assert!(FlywayStatus {
            current_version: Some(REQUIRED_SCHEMA_VERSION + 1),
            covered_required_versions: REQUIRED_SCHEMA_VERSION + 1,
            ..current.clone()
        }
        .is_ahead_of(REQUIRED_SCHEMA_VERSION));
        assert!(!FlywayStatus {
            current_version: Some(REQUIRED_SCHEMA_VERSION + 1),
            covered_required_versions: REQUIRED_SCHEMA_VERSION + 1,
            failed_migrations: 0,
            legacy_baseline_present: false,
        }
        .is_current());
    }
}
