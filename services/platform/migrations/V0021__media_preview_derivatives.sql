-- Immutable preview derivatives for public display. Existing assets remain
-- readable through their original URL until an explicit backfill is added.

ALTER TABLE media_assets
    ADD COLUMN preview_storage_key text,
    ADD COLUMN preview_public_url text,
    ADD COLUMN original_width integer,
    ADD COLUMN original_height integer,
    ADD COLUMN preview_width integer,
    ADD COLUMN preview_height integer,
    ADD COLUMN preview_media_type text,
    ADD COLUMN preview_byte_size bigint,
    ADD CONSTRAINT media_asset_preview_storage_key_check CHECK (
        preview_storage_key IS NULL
        OR (
            length(preview_storage_key) BETWEEN 1 AND 512
            AND preview_storage_key !~ '(^/|/$|(^|/)\.\.?(/|$))'
        )
    ),
    ADD CONSTRAINT media_asset_preview_public_url_check CHECK (
        preview_public_url IS NULL
        OR (
            length(preview_public_url) BETWEEN 8 AND 4096
            AND preview_public_url ~ '^https?://'
        )
    ),
    ADD CONSTRAINT media_asset_original_dimensions_check CHECK (
        (original_width IS NULL AND original_height IS NULL)
        OR (
            original_width BETWEEN 1 AND 16384
            AND original_height BETWEEN 1 AND 16384
            AND original_width::bigint * original_height::bigint <= 40000000
        )
    ),
    ADD CONSTRAINT media_asset_preview_dimensions_check CHECK (
        (preview_width IS NULL AND preview_height IS NULL)
        OR (
            preview_width BETWEEN 1 AND 1600
            AND preview_height BETWEEN 1 AND 1600
        )
    ),
    ADD CONSTRAINT media_asset_preview_metadata_check CHECK (
        (preview_storage_key IS NULL
         AND preview_public_url IS NULL
         AND preview_width IS NULL
         AND preview_height IS NULL
         AND preview_media_type IS NULL
         AND preview_byte_size IS NULL)
        OR (
            preview_storage_key IS NOT NULL
            AND preview_public_url IS NOT NULL
            AND preview_width IS NOT NULL
            AND preview_height IS NOT NULL
            AND preview_media_type = 'image/webp'
            AND preview_byte_size > 0
            AND preview_byte_size <= 26214400
        )
    );

COMMENT ON COLUMN media_assets.preview_public_url IS
    'Immutable external display URL captured with the original media object.';
COMMENT ON COLUMN media_assets.public_url IS
    'Immutable external original URL; display clients prefer preview_public_url.';
