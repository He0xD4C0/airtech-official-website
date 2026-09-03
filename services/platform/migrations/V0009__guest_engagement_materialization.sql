-- Flyway versioned migration. Never edit after release; add a new migration.
-- Event retention is timestamp-based, while engaged visits are defined once
-- per guest visit and UTC day. Keep the small amount of state needed to carry
-- that threshold across retention batches without retaining event payloads.
-- Rows disappear as soon as the UTC day has no remaining raw events, or when
-- the parent visit reaches its own retention boundary.
CREATE TABLE guest_visit_daily_event_materializations (
    guest_visit_id uuid NOT NULL
        REFERENCES guest_visits(id) ON DELETE CASCADE,
    bucket_date date NOT NULL,
    page_views bigint NOT NULL DEFAULT 0 CHECK (page_views >= 0),
    rfq_starts bigint NOT NULL DEFAULT 0 CHECK (rfq_starts >= 0),
    rfq_submissions bigint NOT NULL DEFAULT 0 CHECK (rfq_submissions >= 0),
    engaged_materialized boolean NOT NULL DEFAULT false,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (guest_visit_id, bucket_date)
);
