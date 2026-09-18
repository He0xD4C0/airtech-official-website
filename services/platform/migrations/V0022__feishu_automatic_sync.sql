-- Automatic, one-way Feishu Product Master synchronization.
-- Feishu credentials remain deployment secrets; this schema stores only
-- non-secret source identifiers, scheduling policy, and durable run evidence.

ALTER TABLE sync_runs
    ADD COLUMN connector_id uuid REFERENCES source_connectors(id) ON DELETE RESTRICT,
    ADD COLUMN run_kind text NOT NULL DEFAULT 'full'
        CHECK (run_kind IN ('incremental','full')),
    ADD COLUMN records_applied bigint NOT NULL DEFAULT 0 CHECK (records_applied >= 0),
    ADD COLUMN records_failed bigint NOT NULL DEFAULT 0 CHECK (records_failed >= 0),
    ADD COLUMN assets_seen bigint NOT NULL DEFAULT 0 CHECK (assets_seen >= 0),
    ADD COLUMN assets_copied bigint NOT NULL DEFAULT 0 CHECK (assets_copied >= 0),
    ADD COLUMN assets_reused bigint NOT NULL DEFAULT 0 CHECK (assets_reused >= 0),
    ADD COLUMN assets_failed bigint NOT NULL DEFAULT 0 CHECK (assets_failed >= 0);

ALTER TABLE sync_runs DROP CONSTRAINT sync_runs_status_check;
ALTER TABLE sync_runs ADD CONSTRAINT sync_runs_status_check CHECK (
    status IN (
        'queued','fetching','validating','awaitingResolution','readyToPublish',
        'completed','completedWithErrors','failed'
    )
);

CREATE UNIQUE INDEX sync_runs_one_active_connector_idx
    ON sync_runs (connector_id)
    WHERE connector_id IS NOT NULL
      AND status IN ('queued','fetching','validating','readyToPublish');

CREATE TABLE feishu_connector_settings (
    connector_id uuid PRIMARY KEY REFERENCES source_connectors(id) ON DELETE CASCADE,
    interval_minutes integer NOT NULL DEFAULT 15
        CHECK (interval_minutes BETWEEN 5 AND 1440),
    full_reconcile_enabled boolean NOT NULL DEFAULT true,
    full_reconcile_local_time time NOT NULL DEFAULT '02:00:00',
    timezone text NOT NULL DEFAULT 'Asia/Shanghai'
        CHECK (timezone = 'Asia/Shanghai'),
    mapping_version text NOT NULL DEFAULT 'feishu-product-v1'
        CHECK (length(trim(mapping_version)) BETWEEN 1 AND 100),
    sources jsonb NOT NULL CHECK (jsonb_typeof(sources) = 'array'),
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    last_incremental_at timestamptz,
    last_full_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    updated_by text NOT NULL CHECK (length(trim(updated_by)) BETWEEN 1 AND 320)
);

INSERT INTO source_connectors (id,connector_type,display_name,enabled,created_at,updated_at)
VALUES (
    '63e3d923-632a-4e47-a31b-16f3ec81690e',
    'feishu',
    'AIRTEKPOWER Product Master',
    false,
    now(),
    now()
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO feishu_connector_settings (
    connector_id,sources,updated_by
)
VALUES (
    '63e3d923-632a-4e47-a31b-16f3ec81690e',
    '[
      {"wikiToken":"U0XawQUVJiXzzzkTRGIc6MLDnBf","tableId":"tblzksABQBk6rdB6","name":"Centrifugal Fans","family":"centrifugal"},
      {"wikiToken":"CVf1wStMgi67xgkfMDCcQbO0nZf","tableId":"tblJjxOgBFL0FD0N","name":"Axial Fans","family":"axial"},
      {"wikiToken":"EtCXwTlPWiCREGk9dcectiCon1d","tableId":"tblOtUU5MaxEnZI7","name":"Agriculture & Livestock Fans","family":"axial","application":"agriculture-livestock"},
      {"wikiToken":"HwiswYvnfiNTsMkigGVc2EPZnxc","tableId":"tbl3hCDkvVIs2ZSi","name":"Cross flow fans","family":"crossFlow"}
    ]'::jsonb,
    'migration'
)
ON CONFLICT (connector_id) DO NOTHING;

CREATE TABLE feishu_asset_bindings (
    id uuid PRIMARY KEY,
    connector_id uuid NOT NULL REFERENCES source_connectors(id) ON DELETE CASCADE,
    source_token_hash text NOT NULL CHECK (source_token_hash ~ '^[0-9a-f]{64}$'),
    source_revision text NOT NULL CHECK (length(trim(source_revision)) > 0),
    media_asset_id uuid NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    checksum text NOT NULL CHECK (checksum ~ '^[0-9a-f]{64}$'),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (connector_id,source_token_hash,source_revision)
);

CREATE INDEX feishu_asset_bindings_media_idx
    ON feishu_asset_bindings (media_asset_id);

CREATE TABLE feishu_sync_changes (
    sync_run_id uuid NOT NULL REFERENCES sync_runs(id) ON DELETE CASCADE,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE RESTRICT,
    before_revision bigint,
    after_revision bigint NOT NULL CHECK (after_revision > 0),
    change_kind text NOT NULL CHECK (change_kind IN ('created','updated','archived')),
    rolled_back_at timestamptz,
    rolled_back_by text,
    PRIMARY KEY (sync_run_id,product_id),
    CHECK (before_revision IS NULL OR before_revision > 0),
    CHECK ((rolled_back_at IS NULL) = (rolled_back_by IS NULL))
);

CREATE INDEX feishu_sync_changes_product_idx
    ON feishu_sync_changes (product_id,after_revision);
