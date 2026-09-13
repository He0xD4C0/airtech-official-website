use std::time::Duration;

/// Warning-only trigger for evaluating SQL pushdown. This does not reject or
/// truncate a request; it makes the existing in-memory cost observable.
pub(crate) const SQL_PUSHDOWN_RECORD_THRESHOLD: usize = 2_000;
pub(crate) const SQL_PUSHDOWN_DURATION_THRESHOLD_MS: u64 = 100;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ListObservation {
    pub(crate) request_started: std::time::Instant,
    pub(crate) db_fetch: Duration,
    pub(crate) json_decode: Duration,
    pub(crate) filter_sort: Duration,
    pub(crate) loaded: usize,
    pub(crate) matched: usize,
}

impl Default for ListObservation {
    fn default() -> Self {
        Self {
            request_started: std::time::Instant::now(),
            db_fetch: Duration::ZERO,
            json_decode: Duration::ZERO,
            filter_sort: Duration::ZERO,
            loaded: 0,
            matched: 0,
        }
    }
}

pub(crate) fn observe_list(
    list_name: &'static str,
    observation: ListObservation,
    pagination: Duration,
    returned: usize,
    estimated_payload_bytes: usize,
) {
    let total = observation.request_started.elapsed();
    tracing::info!(
        target: "airtek_platform::list_measurement",
        list_name,
        db_fetch_micros = duration_micros(observation.db_fetch),
        json_decode_micros = duration_micros(observation.json_decode),
        filter_sort_micros = duration_micros(observation.filter_sort),
        pagination_micros = duration_micros(pagination),
        total_micros = duration_micros(total),
        loaded = observation.loaded,
        matched = observation.matched,
        returned,
        estimated_payload_bytes,
        "server-side list phases measured"
    );
    observe_in_memory_filter(list_name, observation.loaded, observation.filter_sort);
}

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

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
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
