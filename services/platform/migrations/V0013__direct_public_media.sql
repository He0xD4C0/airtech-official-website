-- Direct-public media contract. V1/V12 compatibility columns stay in place so
-- a database-first deployment does not break the previous API binary, but all
-- new rows use public/clean defaults and runtime code no longer reads them.

UPDATE media_assets
SET scan_status='clean', access_level='public'
WHERE deleted_at IS NULL;

ALTER TABLE media_assets
    ALTER COLUMN scan_status SET DEFAULT 'clean',
    ALTER COLUMN access_level SET DEFAULT 'public',
    DROP CONSTRAINT IF EXISTS media_asset_review_check,
    DROP CONSTRAINT IF EXISTS media_asset_quarantine_reason_check;

ALTER TABLE product_assets
    ALTER COLUMN scan_status SET DEFAULT 'clean',
    ALTER COLUMN access_level SET DEFAULT 'public';

DROP INDEX IF EXISTS media_assets_review_queue_idx;
DROP INDEX IF EXISTS media_assets_public_delivery_idx;

CREATE INDEX media_assets_public_delivery_idx
    ON media_assets (id)
    WHERE deleted_at IS NULL;
