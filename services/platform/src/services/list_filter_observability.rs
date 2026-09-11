use std::time::Duration;

/// Warning-only trigger for evaluating SQL pushdown. This does not reject or
/// truncate a request; it makes the existing in-memory cost observable.
pub(crate) const SQL_PUSHDOWN_RECORD_THRESHOLD: usize = 2_000;
pub(crate) const SQL_PUSHDOWN_DURATION_THRESHOLD_MS: u64 = 100;

pub(crate) fn observe_in_memory_filter(
    list_name: &'static str,
    input_records: usize,
    elapsed: Duration,
) {
    let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    if crosses_sql_pushdown_threshold(input_records, elapsed_ms) {
        tracing::warn!(
            target: "airtek_platform::list_filter",
            list_name,
            input_records,
            elapsed_ms,
            record_threshold = SQL_PUSHDOWN_RECORD_THRESHOLD,
            duration_threshold_ms = SQL_PUSHDOWN_DURATION_THRESHOLD_MS,
            "in-memory list filtering crossed the SQL pushdown evaluation threshold"
        );
    } else {
        tracing::debug!(
            target: "airtek_platform::list_filter",
            list_name,
            input_records,
            elapsed_ms,
            "in-memory list filtering measured"
        );
    }
}

fn crosses_sql_pushdown_threshold(input_records: usize, elapsed_ms: u64) -> bool {
    input_records >= SQL_PUSHDOWN_RECORD_THRESHOLD
        || elapsed_ms >= SQL_PUSHDOWN_DURATION_THRESHOLD_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_pushdown_warning_thresholds_are_explicit_and_independent() {
        assert_eq!(SQL_PUSHDOWN_RECORD_THRESHOLD, 2_000);
        assert_eq!(SQL_PUSHDOWN_DURATION_THRESHOLD_MS, 100);
        assert!(!crosses_sql_pushdown_threshold(1_999, 99));
        assert!(crosses_sql_pushdown_threshold(2_000, 1));
        assert!(crosses_sql_pushdown_threshold(1, 100));
    }
}
