use std::{
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    time::Instant,
};

#[derive(Debug)]
pub struct RequestMetrics {
    started_at: Instant,
    total: AtomicU64,
    active: AtomicUsize,
    server_errors: AtomicU64,
}

impl Default for RequestMetrics {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            total: AtomicU64::new(0),
            active: AtomicUsize::new(0),
            server_errors: AtomicU64::new(0),
        }
    }
}

impl RequestMetrics {
    pub fn begin(&self) {
        self.total.fetch_add(1, Ordering::Relaxed);
        self.active.fetch_add(1, Ordering::Relaxed);
    }

    pub fn finish(&self, server_error: bool) {
        self.active.fetch_sub(1, Ordering::Relaxed);
        if server_error {
            self.server_errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn render(&self) -> String {
        format!(
            concat!(
                "# TYPE airtek_http_requests_total counter\n",
                "airtek_http_requests_total {}\n",
                "# TYPE airtek_http_requests_active gauge\n",
                "airtek_http_requests_active {}\n",
                "# TYPE airtek_http_server_errors_total counter\n",
                "airtek_http_server_errors_total {}\n",
                "# TYPE airtek_process_uptime_seconds gauge\n",
                "airtek_process_uptime_seconds {}\n",
                "# EOF\n"
            ),
            self.total.load(Ordering::Relaxed),
            self.active.load(Ordering::Relaxed),
            self.server_errors.load(Ordering::Relaxed),
            self.started_at.elapsed().as_secs()
        )
    }
}
