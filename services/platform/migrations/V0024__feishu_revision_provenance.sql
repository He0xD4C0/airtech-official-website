-- Feishu revisions retain their encrypted source run for authorized private
-- pricing reads. Snapshots are run-specific evidence, so an unchanged source
-- revision may appear in multiple idempotent reconciliation runs.

ALTER TABLE source_snapshots
    DROP CONSTRAINT source_snapshots_connector_id_source_record_id_source_revis_key;

CREATE UNIQUE INDEX source_snapshots_run_record_unique
    ON source_snapshots (sync_run_id,source_record_id)
    WHERE sync_run_id IS NOT NULL;

ALTER TABLE products DROP CONSTRAINT products_verified_csv_import_check;
ALTER TABLE products ADD CONSTRAINT products_import_run_shape_check CHECK (
    (data_origin='verifiedCsv' AND product_import_run_id IS NOT NULL)
    OR (data_origin='feishu')
    OR (data_origin='developmentFixture' AND product_import_run_id IS NULL)
);

ALTER TABLE product_revisions
    DROP CONSTRAINT product_revisions_verified_csv_import_check;
ALTER TABLE product_revisions
    ADD CONSTRAINT product_revisions_import_run_shape_check CHECK (
        (data_origin='verifiedCsv' AND product_import_run_id IS NOT NULL)
        OR (data_origin='feishu')
        OR (data_origin='developmentFixture' AND product_import_run_id IS NULL)
    );

COMMENT ON COLUMN products.product_import_run_id IS
    'Current Product Master run; Feishu and verified CSV private source rows remain encrypted.';
