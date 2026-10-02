-- Release 1 of the login refactor. Additive only: new columns, preset-role
-- metadata, integration settings, one-time login flows and new permission
-- keys. No table is dropped, so the previously released application image can
-- keep serving this schema during the migration window.

ALTER TABLE roles
    ADD COLUMN is_preset boolean NOT NULL DEFAULT false,
    ADD COLUMN created_by uuid REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN created_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN updated_at timestamptz NOT NULL DEFAULT now();

UPDATE roles
SET is_preset = true
WHERE key IN (
    'super-admin',
    'content-editor',
    'publisher',
    'product-manager',
    'integration-operator',
    'rfq-operator',
    'analyst',
    'auditor',
    'developer'
);

ALTER TABLE users
    ADD COLUMN must_change_password boolean NOT NULL DEFAULT false,
    ADD COLUMN password_changed_at timestamptz,
    ADD COLUMN phone_e164 text,
    ADD COLUMN phone_verified_at timestamptz,
    ADD COLUMN recovery_key_hash text,
    ADD COLUMN recovery_key_origin text,
    ADD COLUMN recovery_key_confirmed_at timestamptz,
    ADD COLUMN recovery_key_rotated_at timestamptz,
    ADD CONSTRAINT users_phone_e164_check
        CHECK (phone_e164 IS NULL OR phone_e164 ~ '^\+[1-9][0-9]{7,14}$'),
    ADD CONSTRAINT users_recovery_key_origin_check
        CHECK (recovery_key_origin IS NULL OR recovery_key_origin IN ('generated','provided')),
    ADD CONSTRAINT users_phone_verified_check
        CHECK (phone_verified_at IS NULL OR phone_e164 IS NOT NULL),
    ADD CONSTRAINT users_recovery_key_confirmed_check
        CHECK (recovery_key_confirmed_at IS NULL OR recovery_key_hash IS NOT NULL);

-- A verified phone number identifies exactly one administrator account. The
-- partial index keeps unverified rows (phone_e164 IS NULL) unconstrained.
CREATE UNIQUE INDEX users_verified_phone_unique
    ON users (phone_e164)
    WHERE phone_e164 IS NOT NULL;

-- Integration settings follow the object_storage_settings precedent: secrets
-- live in PostgreSQL as plaintext by explicit owner decision, are never
-- returned by an API and never copied into audit records or logs.
CREATE TABLE mail_settings (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    host text NOT NULL CHECK (length(btrim(host)) BETWEEN 1 AND 255),
    port integer NOT NULL CHECK (port BETWEEN 1 AND 65535),
    protocol text NOT NULL CHECK (protocol IN ('starttls','tls')),
    username text NOT NULL DEFAULT '' CHECK (length(username) <= 320),
    password text NOT NULL DEFAULT '' CHECK (length(password) <= 2048),
    from_address text NOT NULL CHECK (length(btrim(from_address)) BETWEEN 3 AND 320),
    from_name text NOT NULL DEFAULT '' CHECK (length(from_name) <= 200),
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    updated_by text NOT NULL CHECK (length(btrim(updated_by)) BETWEEN 1 AND 320)
);

CREATE TABLE sms_settings (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    provider text NOT NULL DEFAULT 'aliyun' CHECK (provider IN ('aliyun')),
    access_key_id text NOT NULL DEFAULT '' CHECK (length(access_key_id) <= 512),
    access_key_secret text NOT NULL DEFAULT '' CHECK (length(access_key_secret) <= 2048),
    sign_name text NOT NULL DEFAULT '' CHECK (length(sign_name) <= 200),
    template_code text NOT NULL DEFAULT '' CHECK (length(template_code) <= 200),
    region text NOT NULL DEFAULT 'cn-hangzhou' CHECK (length(btrim(region)) BETWEEN 1 AND 100),
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    updated_by text NOT NULL CHECK (length(btrim(updated_by)) BETWEEN 1 AND 320)
);

CREATE TABLE captcha_settings (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    provider text NOT NULL CHECK (provider IN ('turnstile','recaptcha','hcaptcha')),
    site_key text NOT NULL DEFAULT '' CHECK (length(site_key) <= 512),
    secret_key text NOT NULL DEFAULT '' CHECK (length(secret_key) <= 2048),
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    updated_by text NOT NULL CHECK (length(btrim(updated_by)) BETWEEN 1 AND 320)
);

COMMENT ON COLUMN mail_settings.password IS
    'Plaintext by explicit owner decision; never expose through APIs, logs, or audit JSON.';
COMMENT ON COLUMN sms_settings.access_key_secret IS
    'Plaintext by explicit owner decision; never expose through APIs, logs, or audit JSON.';
COMMENT ON COLUMN captcha_settings.secret_key IS
    'Plaintext by explicit owner decision; never expose through APIs, logs, or audit JSON.';

-- One row per multi-step login attempt. Unknown accounts still produce a row so
-- the flow never reveals whether an account exists; `user_id` stays NULL and
-- code delivery is silently skipped.
CREATE TABLE auth_login_flows (
    id uuid PRIMARY KEY,
    flow_token_hash bytea NOT NULL UNIQUE CHECK (octet_length(flow_token_hash) = 32),
    email text NOT NULL CHECK (length(btrim(email)) BETWEEN 3 AND 254),
    user_id uuid REFERENCES users(id) ON DELETE CASCADE,
    source_hash text NOT NULL CHECK (length(btrim(source_hash)) BETWEEN 1 AND 128),
    primary_method text CHECK (primary_method IN ('password','emailCode','smsCode')),
    pending_factor text CHECK (pending_factor IN ('emailCode','smsCode','riskSms','totp')),
    pending_destination text,
    code_hash text,
    code_expires_at timestamptz,
    code_attempts integer NOT NULL DEFAULT 0 CHECK (code_attempts >= 0),
    send_count integer NOT NULL DEFAULT 0 CHECK (send_count >= 0),
    last_sent_at timestamptz,
    risk_required boolean NOT NULL DEFAULT false,
    status text NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','verifiedPrimary','authenticated','failed','expired')),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz
);

CREATE INDEX auth_login_flows_pending_expiry_idx
    ON auth_login_flows (expires_at)
    WHERE status IN ('pending','verifiedPrimary');
CREATE INDEX auth_login_flows_user_idx
    ON auth_login_flows (user_id, created_at DESC);

COMMENT ON TABLE auth_login_flows IS
    'Short-lived multi-step administrator login attempts; verification codes are stored as Argon2 hashes only.';

INSERT INTO permissions (key, description) VALUES
    ('mail.manage', 'Configure and test outbound email delivery'),
    ('sms.manage', 'Configure and test outbound SMS delivery'),
    ('captcha.manage', 'Configure and test the CAPTCHA provider'),
    ('identity.roles.manage', 'Create, edit and delete administrator roles')
ON CONFLICT (key) DO NOTHING;

INSERT INTO role_permissions (role_id, permission_key)
SELECT role.id, template.permission_key
FROM (
    VALUES
        ('super-admin', 'mail.manage'),
        ('super-admin', 'sms.manage'),
        ('super-admin', 'captcha.manage'),
        ('super-admin', 'identity.roles.manage'),
        ('integration-operator', 'mail.manage'),
        ('integration-operator', 'sms.manage'),
        ('integration-operator', 'captcha.manage'),
        ('auditor', 'analytics.read')
) AS template(role_key, permission_key)
JOIN roles AS role ON role.key = template.role_key
ON CONFLICT (role_id, permission_key) DO NOTHING;

-- airtek:destructive: the developer role loses platform-settings authority; reversible by re-inserting the grant.
DELETE FROM role_permissions
WHERE permission_key = 'settings.manage'
  AND role_id IN (SELECT id FROM roles WHERE key = 'developer');
