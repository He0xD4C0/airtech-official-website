-- Database-owned S3 settings and immutable public media locations.
-- Credentials are deliberately stored as plaintext by owner decision. Runtime
-- APIs must never return them or copy them into audit records.

CREATE TABLE object_storage_settings (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    provider text NOT NULL DEFAULT 's3' CHECK (provider = 's3'),
    endpoint text NOT NULL CHECK (
        length(endpoint) BETWEEN 8 AND 2048
        AND endpoint ~ '^https?://'
        AND right(endpoint, 1) <> '/'
    ),
    region text NOT NULL CHECK (length(trim(region)) BETWEEN 1 AND 100),
    bucket text NOT NULL CHECK (length(trim(bucket)) BETWEEN 1 AND 255),
    access_key_id text NOT NULL CHECK (length(trim(access_key_id)) BETWEEN 1 AND 512),
    secret_access_key text NOT NULL CHECK (length(secret_access_key) BETWEEN 1 AND 2048),
    key_prefix text NOT NULL CHECK (length(trim(key_prefix)) BETWEEN 1 AND 512),
    path_style boolean NOT NULL DEFAULT false,
    public_base_url text NOT NULL CHECK (
        length(public_base_url) BETWEEN 8 AND 2048
        AND public_base_url ~ '^https?://'
        AND right(public_base_url, 1) <> '/'
    ),
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    updated_by text NOT NULL CHECK (length(trim(updated_by)) BETWEEN 1 AND 320)
);

ALTER TABLE media_assets
    ADD COLUMN public_url text,
    ADD CONSTRAINT media_asset_public_url_check CHECK (
        public_url IS NULL
        OR (
            length(public_url) BETWEEN 8 AND 4096
            AND public_url ~ '^https?://'
        )
    );

COMMENT ON COLUMN object_storage_settings.secret_access_key IS
    'Plaintext by explicit owner decision; never expose through APIs, logs, or audit JSON.';
COMMENT ON COLUMN media_assets.public_url IS
    'Immutable external URL captured when the media object is created or explicitly adopted.';
