-- Durable per-table reconciliation and explicit Product ownership. Products
-- pending source deletion are immediately unpublished, then physically purged.

ALTER TABLE sync_runs
    ADD COLUMN records_deleted bigint NOT NULL DEFAULT 0
        CHECK (records_deleted >= 0);

UPDATE sync_runs SET status='completedWithErrors'
WHERE status='awaitingResolution';

ALTER TABLE sync_runs
    DROP CONSTRAINT sync_runs_status_check,
    ADD CONSTRAINT sync_runs_status_check CHECK (
        status IN (
            'queued','fetching','validating','readyToPublish',
            'completed','completedWithErrors','failed'
        )
    ),
    DROP COLUMN run_kind,
    DROP COLUMN conflict_count;

UPDATE staging_records SET validation_status='invalid'
WHERE validation_status='conflicted';

ALTER TABLE staging_records
    DROP CONSTRAINT staging_records_validation_status_check,
    ADD CONSTRAINT staging_records_validation_status_check CHECK (
        validation_status IN ('pending','valid','invalid')
    );

CREATE TABLE feishu_product_ownership (
    product_id uuid PRIMARY KEY REFERENCES products(id) ON DELETE CASCADE,
    connector_id uuid NOT NULL REFERENCES source_connectors(id) ON DELETE CASCADE,
    wiki_token text NOT NULL CHECK (length(trim(wiki_token)) BETWEEN 1 AND 200),
    table_id text NOT NULL CHECK (length(trim(table_id)) BETWEEN 1 AND 100),
    record_id text NOT NULL CHECK (length(trim(record_id)) BETWEEN 1 AND 200),
    generation bigint NOT NULL DEFAULT 1 CHECK (generation > 0),
    state text NOT NULL DEFAULT 'active'
        CHECK (state IN ('active','pendingDelete')),
    pending_delete_at timestamptz,
    deletion_run_id uuid REFERENCES sync_runs(id) ON DELETE SET NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (connector_id,wiki_token,table_id,record_id),
    CHECK (
        (state='active' AND pending_delete_at IS NULL)
        OR (state='pendingDelete' AND pending_delete_at IS NOT NULL)
    )
);

CREATE INDEX feishu_product_ownership_source_idx
    ON feishu_product_ownership(connector_id,wiki_token,table_id,state,record_id);

CREATE TABLE feishu_run_table_results (
    sync_run_id uuid NOT NULL REFERENCES sync_runs(id) ON DELETE CASCADE,
    wiki_token text NOT NULL CHECK (length(trim(wiki_token)) BETWEEN 1 AND 200),
    table_id text NOT NULL CHECK (length(trim(table_id)) BETWEEN 1 AND 100),
    source_name text NOT NULL CHECK (length(trim(source_name)) BETWEEN 1 AND 120),
    status text NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','fetching','completed','failed')),
    records_seen bigint NOT NULL DEFAULT 0 CHECK (records_seen >= 0),
    records_applied bigint NOT NULL DEFAULT 0 CHECK (records_applied >= 0),
    records_failed bigint NOT NULL DEFAULT 0 CHECK (records_failed >= 0),
    records_deleted bigint NOT NULL DEFAULT 0 CHECK (records_deleted >= 0),
    assets_seen bigint NOT NULL DEFAULT 0 CHECK (assets_seen >= 0),
    assets_copied bigint NOT NULL DEFAULT 0 CHECK (assets_copied >= 0),
    assets_reused bigint NOT NULL DEFAULT 0 CHECK (assets_reused >= 0),
    assets_failed bigint NOT NULL DEFAULT 0 CHECK (assets_failed >= 0),
    error text,
    completed_at timestamptz,
    PRIMARY KEY(sync_run_id,wiki_token,table_id)
);

ALTER TABLE jobs
    ADD COLUMN connector_id uuid REFERENCES source_connectors(id) ON DELETE CASCADE;

UPDATE jobs job
SET connector_id=run.connector_id
FROM sync_runs run
WHERE job.job_type='feishuSync'
  AND job.payload->>'syncRunId'=run.id::text;

CREATE UNIQUE INDEX jobs_one_running_connector_integration_idx
    ON jobs(connector_id)
    WHERE connector_id IS NOT NULL
      AND job_type IN ('feishuSync','feishuSourcePurge')
      AND status='running';

ALTER TABLE feishu_asset_bindings
    ADD COLUMN created_by_sync_run_id uuid REFERENCES sync_runs(id) ON DELETE SET NULL;

CREATE TABLE feishu_object_compensations (
    id uuid PRIMARY KEY,
    media_asset_id uuid UNIQUE REFERENCES media_assets(id) ON DELETE SET NULL,
    storage_key text NOT NULL CHECK (length(trim(storage_key)) > 0),
    preview_storage_key text,
    reason text NOT NULL CHECK (length(trim(reason)) > 0),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE feishu_object_compensations IS
    'Durable cleanup intent for unreferenced or orphaned Feishu objects; rows remain until object and optional catalogue deletion both succeed.';

ALTER TABLE feishu_sync_changes
    DROP COLUMN rolled_back_at,
    DROP COLUMN rolled_back_by,
    DROP CONSTRAINT feishu_sync_changes_product_id_fkey;

ALTER TABLE feishu_sync_changes
    ADD CONSTRAINT feishu_sync_changes_product_id_fkey
    FOREIGN KEY(product_id) REFERENCES products(id) ON DELETE CASCADE;

DROP TABLE sync_conflicts;

COMMENT ON TABLE feishu_product_ownership IS
    'Current Feishu table/record authority for each live Product row.';
