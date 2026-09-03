-- Flyway versioned migration. Never edit after release; add a new migration.
-- Make the runtime Product publication evidence chain enforceable at rest.
ALTER TABLE source_snapshots
    ALTER COLUMN connector_id SET NOT NULL,
    ALTER COLUMN sync_run_id SET NOT NULL,
    ADD CONSTRAINT source_snapshot_revision_not_blank
        CHECK (length(trim(source_revision)) > 0),
    ADD CONSTRAINT source_snapshot_checksum_not_blank
        CHECK (length(trim(checksum)) > 0);

CREATE UNIQUE INDEX source_snapshot_staging_identity_unique
    ON source_snapshots (id, sync_run_id, source_record_id);

ALTER TABLE staging_records
    ADD CONSTRAINT staging_snapshot_identity_fk
        FOREIGN KEY (source_snapshot_id, sync_run_id, source_record_id)
        REFERENCES source_snapshots (id, sync_run_id, source_record_id),
    ADD CONSTRAINT staging_validation_errors_array
        CHECK (jsonb_typeof(validation_errors) = 'array'),
    ADD CONSTRAINT valid_staging_requires_clean_normalized_object
        CHECK (
            validation_status <> 'valid'
            OR (
                normalized_payload IS NOT NULL
                AND jsonb_typeof(normalized_payload) = 'object'
                AND validation_errors = '[]'::jsonb
            )
        );

CREATE INDEX staging_source_snapshot_status_idx
    ON staging_records (source_snapshot_id, validation_status, created_at DESC);

CREATE INDEX sync_conflicts_product_open_idx
    ON sync_conflicts (product_id)
    WHERE resolved_at IS NULL;

CREATE INDEX sync_conflicts_source_record_open_idx
    ON sync_conflicts (source_record_id)
    WHERE resolved_at IS NULL;
