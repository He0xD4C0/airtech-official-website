-- Feishu custom-application credentials are managed by the authenticated
-- Admin UI. Database operators and backups are part of the trusted boundary.

ALTER TABLE feishu_connector_settings
    ADD COLUMN app_id text,
    ADD COLUMN app_secret text;

ALTER TABLE feishu_connector_settings ADD CONSTRAINT feishu_credentials_shape_check CHECK (
    (app_id IS NULL AND app_secret IS NULL)
    OR (
        app_id IS NOT NULL
        AND app_secret IS NOT NULL
        AND length(trim(app_id)) BETWEEN 1 AND 100
        AND octet_length(app_secret) BETWEEN 1 AND 512
    )
);

COMMENT ON COLUMN feishu_connector_settings.app_id IS
    'GUI-managed Feishu custom application id; safe to return to administrators.';
COMMENT ON COLUMN feishu_connector_settings.app_secret IS
    'GUI-managed plaintext secret. Never return it from APIs, audits, errors, or logs.';
