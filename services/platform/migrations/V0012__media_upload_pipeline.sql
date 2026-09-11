-- Media upload pipeline: object-storage upload metadata plus the human review
-- gate that decides whether an immutable object may reach the public origin.
--
-- The columns are additive on purpose. Existing media rows keep working as
-- read-only catalogue entries, and legacy columns/views stay in place so a
-- rollback does not need a destructive migration.

ALTER TABLE media_assets
    ADD COLUMN storage_backend text,
    ADD COLUMN content_type text,
    ADD COLUMN uploaded_by text,
    ADD COLUMN reviewed_by text,
    ADD COLUMN reviewed_at timestamptz,
    ADD COLUMN review_reason text;

-- V1 already allowed quarantined assets. Preserve that fail-closed state and
-- explicitly record that their original human rationale predates this column;
-- never make them clean merely to satisfy the new integrity constraint.
UPDATE media_assets
SET review_reason = 'Legacy quarantined asset retained by V0012; original review reason unavailable.'
WHERE scan_status = 'quarantined'
  AND review_reason IS NULL;

ALTER TABLE media_assets
    ADD CONSTRAINT media_asset_storage_backend_check
        CHECK (storage_backend IS NULL OR storage_backend IN ('local','s3')),
    ADD CONSTRAINT media_asset_content_type_check
        CHECK (content_type IS NULL OR length(trim(content_type)) BETWEEN 3 AND 120),
    -- A reviewed asset always records who reviewed it and when; quarantine
    -- additionally records why, so the decision is never anonymous.
    ADD CONSTRAINT media_asset_review_check
        CHECK (
            (reviewed_at IS NULL AND reviewed_by IS NULL)
            OR (reviewed_at IS NOT NULL AND reviewed_by IS NOT NULL)
        ),
    ADD CONSTRAINT media_asset_quarantine_reason_check
        CHECK (
            scan_status <> 'quarantined'
            OR NULLIF(trim(review_reason), '') IS NOT NULL
        );

-- Public delivery reads at most one row per request and always filters on the
-- same tuple. The partial index keeps that lookup independent of library size.
CREATE INDEX media_assets_public_delivery_idx
    ON media_assets (id)
    WHERE deleted_at IS NULL AND scan_status = 'clean' AND access_level = 'public';

CREATE INDEX media_assets_review_queue_idx
    ON media_assets (created_at DESC)
    WHERE deleted_at IS NULL AND scan_status = 'pending';
