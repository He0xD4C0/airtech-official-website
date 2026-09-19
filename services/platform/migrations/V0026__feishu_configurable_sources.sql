-- Configurable source selection, independent schedules, and immutable run
-- configuration snapshots. Every Feishu run is a full scan of enabled tables.

ALTER TABLE feishu_connector_settings
    ADD COLUMN interval_enabled boolean NOT NULL DEFAULT true,
    ADD COLUMN daily_enabled boolean NOT NULL DEFAULT true,
    ADD COLUMN daily_local_time time NOT NULL DEFAULT '02:00:00',
    ADD COLUMN last_interval_at timestamptz,
    ADD COLUMN last_daily_at timestamptz,
    ADD COLUMN connection_revision bigint NOT NULL DEFAULT 1
        CHECK (connection_revision > 0),
    ADD COLUMN tested_connection_revision bigint,
    ADD COLUMN last_connection_test_at timestamptz;

UPDATE feishu_connector_settings
SET daily_enabled=full_reconcile_enabled,
    daily_local_time=full_reconcile_local_time,
    last_interval_at=last_incremental_at,
    last_daily_at=last_full_at,
    sources=(
        SELECT jsonb_agg(
            CASE WHEN item ? 'enabled' THEN item
                 ELSE jsonb_build_object('enabled',true) || item END
            ORDER BY ordinal
        )
        FROM jsonb_array_elements(sources) WITH ORDINALITY AS entry(item,ordinal)
    );

ALTER TABLE feishu_connector_settings
    DROP COLUMN full_reconcile_enabled,
    DROP COLUMN full_reconcile_local_time,
    DROP COLUMN last_incremental_at,
    DROP COLUMN last_full_at;

ALTER TABLE feishu_connector_settings ADD CONSTRAINT feishu_test_revision_check CHECK (
    tested_connection_revision IS NULL
    OR tested_connection_revision BETWEEN 1 AND connection_revision
);

ALTER TABLE sync_runs
    ADD COLUMN trigger text NOT NULL DEFAULT 'manual'
        CHECK (trigger IN ('manual','interval','daily','initial')),
    ADD COLUMN settings_revision bigint NOT NULL DEFAULT 1
        CHECK (settings_revision > 0),
    ADD COLUMN source_config jsonb NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_typeof(source_config)='array');

COMMENT ON COLUMN sync_runs.source_config IS
    'Immutable enabled Feishu source list captured when the full run is queued.';
