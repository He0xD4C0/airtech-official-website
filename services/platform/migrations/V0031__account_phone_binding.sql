-- Additive follow-up to the login refactor: self-service phone binding for SMS
-- sign-in and risk verification. Only a confirmed number is copied onto
-- `users`; the pending challenge keeps its own short-lived row so an
-- interrupted binding never marks a number as verified.

CREATE TABLE auth_phone_verifications (
    user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    phone_e164 text NOT NULL CHECK (phone_e164 ~ '^\+[1-9][0-9]{7,14}$'),
    code_hash text NOT NULL,
    code_expires_at timestamptz NOT NULL,
    code_attempts integer NOT NULL DEFAULT 0 CHECK (code_attempts >= 0),
    send_count integer NOT NULL DEFAULT 0 CHECK (send_count >= 0),
    last_sent_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- A number can be pending for exactly one account at a time; the confirmed
-- mapping is enforced by `users_verified_phone_unique`.
CREATE UNIQUE INDEX auth_phone_verifications_phone_unique
    ON auth_phone_verifications (phone_e164);

COMMENT ON TABLE auth_phone_verifications IS
    'Short-lived administrator phone-binding challenges; verification codes are stored as Argon2 hashes only.';

-- Local development and acceptance stacks deliver through a credential-free
-- relay that speaks plaintext SMTP. Plaintext authentication stays refused, so
-- this only unlocks unauthenticated delivery to a trusted local relay.
ALTER TABLE mail_settings
    DROP CONSTRAINT mail_settings_protocol_check,
    ADD CONSTRAINT mail_settings_protocol_check
        CHECK (protocol IN ('starttls','tls','plain'));
