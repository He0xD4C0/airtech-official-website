-- Harden operational data introduced by 0005 without rebuilding or deleting
-- any accepted business records.

-- A wide text primary key can exceed PostgreSQL's btree tuple limit. Retain
-- every readable dimension, but identify the tuple with a deterministic
-- fixed-length digest. The trigger rejects tampered digests and turns the
-- cryptographically improbable collision case into an explicit failure.
CREATE FUNCTION guest_source_dimension_hash(
    p_source_type text,
    p_source_name text,
    p_utm_source text,
    p_utm_medium text,
    p_utm_campaign text,
    p_landing_path text,
    p_locale text
) RETURNS bytea
LANGUAGE sql
IMMUTABLE
STRICT
PARALLEL SAFE
AS $$
    SELECT sha256(convert_to(jsonb_build_array(
        p_source_type,
        p_source_name,
        p_utm_source,
        p_utm_medium,
        p_utm_campaign,
        p_landing_path,
        p_locale
    )::text, 'UTF8'))
$$;

ALTER TABLE guest_source_daily
    ADD COLUMN dimension_hash bytea;

UPDATE guest_source_daily
SET dimension_hash=guest_source_dimension_hash(
    source_type,
    source_name,
    utm_source,
    utm_medium,
    utm_campaign,
    landing_path,
    locale
);

DO $$
DECLARE
    collision_count bigint;
BEGIN
    SELECT count(*) INTO collision_count
    FROM (
        SELECT bucket_date, dimension_hash
        FROM guest_source_daily
        GROUP BY bucket_date, dimension_hash
        HAVING count(*) > 1
    ) collisions;
    IF collision_count > 0 THEN
        RAISE EXCEPTION USING
            ERRCODE='23505',
            MESSAGE=format(
                'Migration 0008 detected %s guest-source dimension hash collision(s); no aggregate rows were changed.',
                collision_count
            );
    END IF;
END
$$;

ALTER TABLE guest_source_daily
    DROP CONSTRAINT guest_source_daily_pkey,
    ALTER COLUMN dimension_hash SET NOT NULL,
    ADD CONSTRAINT guest_source_daily_dimension_hash_length_check
        CHECK (octet_length(dimension_hash) = 32),
    ADD CONSTRAINT guest_source_daily_pkey
        PRIMARY KEY (bucket_date, dimension_hash);

CREATE FUNCTION enforce_guest_source_daily_dimension_hash()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    expected_hash bytea;
BEGIN
    expected_hash := guest_source_dimension_hash(
        NEW.source_type,
        NEW.source_name,
        NEW.utm_source,
        NEW.utm_medium,
        NEW.utm_campaign,
        NEW.landing_path,
        NEW.locale
    );
    IF NEW.dimension_hash IS NULL THEN
        NEW.dimension_hash := expected_hash;
    ELSIF NEW.dimension_hash <> expected_hash THEN
        RAISE EXCEPTION USING
            ERRCODE='23514',
            MESSAGE='guest_source_daily.dimension_hash does not match its source dimensions';
    END IF;
    IF TG_OP = 'UPDATE'
       AND OLD.dimension_hash = NEW.dimension_hash
       AND ROW(
           OLD.source_type,
           OLD.source_name,
           OLD.utm_source,
           OLD.utm_medium,
           OLD.utm_campaign,
           OLD.landing_path,
           OLD.locale
       ) IS DISTINCT FROM ROW(
           NEW.source_type,
           NEW.source_name,
           NEW.utm_source,
           NEW.utm_medium,
           NEW.utm_campaign,
           NEW.landing_path,
           NEW.locale
       )
    THEN
        RAISE EXCEPTION USING
            ERRCODE='23505',
            MESSAGE='guest_source_daily dimension hash collision detected';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER guest_source_daily_dimension_hash_guard
BEFORE INSERT OR UPDATE ON guest_source_daily
FOR EACH ROW
EXECUTE FUNCTION enforce_guest_source_daily_dimension_hash();

-- Consent-bound first-party analytics is the only supported event shape. An
-- old database containing unconstrained rows must be repaired deliberately;
-- silently deleting or guessing acquisition/consent identity is forbidden.
DO $$
DECLARE
    invalid_count bigint;
BEGIN
    SELECT count(*) INTO invalid_count
    FROM analytics_events
    WHERE guest_visit_id IS NULL
       OR consent_record_id IS NULL
       OR anonymous_session_id IS NULL;
    IF invalid_count > 0 THEN
        RAISE EXCEPTION USING
            ERRCODE='23514',
            MESSAGE=format(
                'Migration 0008 cannot enforce consent-bound analytics: %s existing event(s) lack guest_visit_id, consent_record_id, or anonymous_session_id; no rows were deleted.',
                invalid_count
            );
    END IF;
END
$$;

ALTER TABLE analytics_events
    DROP CONSTRAINT analytics_events_guest_consent_pair_check,
    ALTER COLUMN guest_visit_id SET NOT NULL,
    ALTER COLUMN consent_record_id SET NOT NULL,
    ALTER COLUMN anonymous_session_id SET NOT NULL;

-- An identical source file is idempotent only within one deployment
-- environment. Existing rows cannot be attributed safely during a migration,
-- so they retain an explicit legacy label instead of being guessed.
ALTER TABLE product_import_runs
    ADD COLUMN environment text;

UPDATE product_import_runs
SET environment='legacy';

ALTER TABLE product_import_runs
    ALTER COLUMN environment SET NOT NULL,
    ADD CONSTRAINT product_import_runs_environment_check
        CHECK (environment IN ('production', 'development', 'test', 'legacy'));

DO $$
DECLARE
    old_constraint text;
BEGIN
    SELECT constraint_record.conname INTO old_constraint
    FROM pg_constraint constraint_record
    WHERE constraint_record.conrelid='product_import_runs'::regclass
      AND constraint_record.contype='u'
      AND (
          SELECT array_agg(attribute.attname ORDER BY key_column.ordinality)
          FROM unnest(constraint_record.conkey)
               WITH ORDINALITY AS key_column(attnum, ordinality)
          JOIN pg_attribute attribute
            ON attribute.attrelid=constraint_record.conrelid
           AND attribute.attnum=key_column.attnum
      ) = ARRAY['data_origin', 'source_checksum', 'mapping_version']::name[];
    IF old_constraint IS NULL THEN
        RAISE EXCEPTION
            'Migration 0008 could not find the legacy Product import idempotency constraint';
    END IF;
    EXECUTE format(
        'ALTER TABLE product_import_runs DROP CONSTRAINT %I',
        old_constraint
    );
END
$$;

ALTER TABLE product_import_runs
    ADD CONSTRAINT product_import_runs_environment_source_unique
        UNIQUE (environment, data_origin, source_checksum, mapping_version);

-- Promoted confidential staging rows still contain ciphertext until their
-- expiry and therefore belong in the cleanup index. AES-GCM nonces must never
-- repeat under an active encryption key; expired cryptographic erasure rows
-- are deliberately excluded because their nonce bytes are zeroized.
DROP INDEX product_import_private_staging_expiry_idx;

CREATE INDEX product_import_private_staging_expiry_idx
    ON product_import_private_staging (expires_at)
    WHERE status IN ('received', 'validated', 'promoted', 'rejected');

CREATE UNIQUE INDEX product_import_private_staging_active_nonce_unique
    ON product_import_private_staging (encryption_key_id, nonce)
    WHERE status IN ('received', 'validated', 'promoted', 'rejected');

-- Older installations may already have rows labelled expired. That state is a
-- cryptographic-erasure assertion, so complete the erasure before enforcing it
-- as a durable database invariant. Byte lengths are retained for auditability.
UPDATE product_import_private_staging
SET ciphertext=decode(repeat('00',octet_length(ciphertext)),'hex'),
    nonce=decode(repeat('00',octet_length(nonce)),'hex'),
    authentication_tag=decode(repeat('00',octet_length(authentication_tag)),'hex'),
    processed_at=COALESCE(processed_at,now())
WHERE status='expired';

ALTER TABLE product_import_private_staging
    ADD CONSTRAINT product_import_private_staging_expired_zeroized_check
        CHECK (
            status <> 'expired'
            OR (
                ciphertext=decode(repeat('00',octet_length(ciphertext)),'hex')
                AND nonce=decode(repeat('00',octet_length(nonce)),'hex')
                AND authentication_tag=decode(
                    repeat('00',octet_length(authentication_tag)),
                    'hex'
                )
            )
        );

-- Every mutable General Information pointer now resolves to an immutable
-- revision with the same locale. Backfill old drafts first; conflicts are
-- retained and surfaced by the foreign-key validation rather than rewritten.
INSERT INTO general_information_revisions (
    general_information_id,
    revision,
    payload,
    locale,
    is_placeholder,
    data_origin,
    created_by,
    created_at
)
SELECT id,
       current_revision,
       payload,
       locale,
       is_placeholder,
       data_origin,
       updated_by,
       updated_at
FROM general_information
ON CONFLICT (general_information_id, revision) DO NOTHING;

ALTER TABLE general_information_revisions
    ADD CONSTRAINT general_information_revisions_identity_locale_unique
        UNIQUE (general_information_id, revision, locale);

ALTER TABLE general_information
    ADD CONSTRAINT general_information_current_revision_locale_fk
        FOREIGN KEY (id, current_revision, locale)
        REFERENCES general_information_revisions(
            general_information_id,
            revision,
            locale
        )
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT general_information_published_revision_locale_fk
        FOREIGN KEY (id, published_revision, locale)
        REFERENCES general_information_revisions(
            general_information_id,
            revision,
            locale
        )
        DEFERRABLE INITIALLY DEFERRED;
