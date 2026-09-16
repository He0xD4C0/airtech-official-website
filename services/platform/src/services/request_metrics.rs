use std::{
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    time::Instant,
};

const LEGACY_CURSOR_ENDPOINT_COUNT: usize = 15;

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum LegacyCursorEndpoint {
    AdminAudit,
    AdminProducts,
    AdminProductImports,
    AdminRoles,
    AdminUsers,
    BusinessInbox,
    Analytics,
    MediaAssets,
    SyncRuns,
    FeishuMappings,
    FeishuStaging,
    FeishuConflicts,
    ProductOverrides,
    PublicNews,
    PublicProducts,
}

impl LegacyCursorEndpoint {
    const ALL: [Self; LEGACY_CURSOR_ENDPOINT_COUNT] = [
        Self::AdminAudit,
        Self::AdminProducts,
        Self::AdminProductImports,
        Self::AdminRoles,
        Self::AdminUsers,
        Self::BusinessInbox,
        Self::Analytics,
        Self::MediaAssets,
        Self::SyncRuns,
        Self::FeishuMappings,
        Self::FeishuStaging,
        Self::FeishuConflicts,
        Self::ProductOverrides,
        Self::PublicNews,
        Self::PublicProducts,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::AdminAudit => "admin_audit",
            Self::AdminProducts => "admin_products",
            Self::AdminProductImports => "admin_product_imports",
            Self::AdminRoles => "admin_roles",
            Self::AdminUsers => "admin_users",
            Self::BusinessInbox => "admin_business_inbox",
            Self::Analytics => "admin_analytics",
            Self::MediaAssets => "admin_media_assets",
            Self::SyncRuns => "admin_sync_runs",
            Self::FeishuMappings => "admin_feishu_mappings",
            Self::FeishuStaging => "admin_feishu_staging",
            Self::FeishuConflicts => "admin_feishu_conflicts",
            Self::ProductOverrides => "admin_product_overrides",
            Self::PublicNews => "public_news",
            Self::PublicProducts => "public_products",
        }
    }
}

#[derive(Debug)]
pub struct RequestMetrics {
    started_at: Instant,
    total: AtomicU64,
    active: AtomicUsize,
    server_errors: AtomicU64,
    legacy_cursor_requests: [AtomicU64; LEGACY_CURSOR_ENDPOINT_COUNT],
}

impl Default for RequestMetrics {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            total: AtomicU64::new(0),
            active: AtomicUsize::new(0),
            server_errors: AtomicU64::new(0),
            legacy_cursor_requests: std::array::from_fn(|_| AtomicU64::new(0)),
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

    pub fn record_legacy_cursor(&self, endpoint: LegacyCursorEndpoint) {
        self.legacy_cursor_requests[endpoint as usize].fetch_add(1, Ordering::Relaxed);
    }

    pub fn render(&self) -> String {
        let mut metrics = format!(
            concat!(
                "# TYPE airtek_http_requests_total counter\n",
                "airtek_http_requests_total {}\n",
                "# TYPE airtek_http_requests_active gauge\n",
                "airtek_http_requests_active {}\n",
                "# TYPE airtek_http_server_errors_total counter\n",
                "airtek_http_server_errors_total {}\n",
                "# TYPE airtek_process_uptime_seconds gauge\n",
                "airtek_process_uptime_seconds {}\n"
            ),
            self.total.load(Ordering::Relaxed),
            self.active.load(Ordering::Relaxed),
            self.server_errors.load(Ordering::Relaxed),
            self.started_at.elapsed().as_secs()
        );
        metrics.push_str("# TYPE airtek_legacy_cursor_requests_total counter\n");
        for endpoint in LegacyCursorEndpoint::ALL {
            metrics.push_str(&format!(
                "airtek_legacy_cursor_requests_total{{endpoint=\"{}\"}} {}\n",
                endpoint.label(),
                self.legacy_cursor_requests[endpoint as usize].load(Ordering::Relaxed)
            ));
        }
        metrics.push_str("# EOF\n");
        metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_cursor_metrics_use_only_fixed_endpoint_labels() {
        let metrics = RequestMetrics::default();
        metrics.record_legacy_cursor(LegacyCursorEndpoint::AdminAudit);
        let rendered = metrics.render();
        assert!(
            rendered.contains("airtek_legacy_cursor_requests_total{endpoint=\"admin_audit\"} 1")
        );
        assert_eq!(
            rendered
                .lines()
                .filter(|line| line.starts_with("airtek_legacy_cursor_requests_total{"))
                .count(),
            LEGACY_CURSOR_ENDPOINT_COUNT
        );
    }
}
