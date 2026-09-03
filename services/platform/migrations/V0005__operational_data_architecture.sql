-- Flyway versioned migration. Never edit after release; add a new migration.
-- Extend the foundation without replacing the immutable content/product revision
-- model. Exact product master facts remain Feishu-owned; development fixtures are
-- explicitly marked and constrained so that they cannot masquerade as indexable
-- production data.

ALTER TABLE content_entries
    ADD COLUMN data_origin text NOT NULL DEFAULT 'editorial',
    ADD CONSTRAINT content_entries_data_origin_check
        CHECK (data_origin IN ('editorial', 'developmentFixture')),
    ADD CONSTRAINT content_development_fixture_is_placeholder
        CHECK (data_origin <> 'developmentFixture' OR is_placeholder);

ALTER TABLE content_entries
    DROP CONSTRAINT content_entries_kind_check,
    ADD CONSTRAINT content_entries_kind_check CHECK (
        kind IN (
            'home', 'solution', 'technology', 'article', 'news', 'faq',
            'caseStudy', 'download', 'company', 'legal', 'navigation', 'footer'
        )
    );

ALTER TABLE products
    ADD COLUMN data_origin text NOT NULL DEFAULT 'feishu',
    ALTER COLUMN source_snapshot_id DROP NOT NULL,
    ADD CONSTRAINT products_data_origin_check
        CHECK (data_origin IN ('feishu', 'verifiedCsv', 'developmentFixture')),
    ADD CONSTRAINT products_origin_provenance_check
        CHECK (
            (
                data_origin = 'feishu'
                AND source_snapshot_id IS NOT NULL
                AND length(trim(source_revision)) > 0
            )
            OR
            (
                data_origin = 'verifiedCsv'
                AND source_snapshot_id IS NULL
                AND stable_id NOT LIKE 'DEV-FIXTURE-%'
                AND length(trim(source_revision)) > 0
            )
            OR
            (
                data_origin = 'developmentFixture'
                AND source_snapshot_id IS NULL
                AND stable_id LIKE 'DEV-FIXTURE-%'
                AND NOT indexable
                AND length(trim(source_revision)) > 0
            )
        );

CREATE UNIQUE INDEX products_id_data_origin_unique
    ON products (id, data_origin);

ALTER TABLE product_revisions
    ADD COLUMN data_origin text NOT NULL DEFAULT 'feishu',
    ALTER COLUMN source_snapshot_id DROP NOT NULL,
    ADD CONSTRAINT product_revisions_data_origin_check
        CHECK (data_origin IN ('feishu', 'verifiedCsv', 'developmentFixture')),
    ADD CONSTRAINT product_revisions_origin_provenance_check
        CHECK (
            (data_origin = 'feishu' AND source_snapshot_id IS NOT NULL)
            OR
            (data_origin = 'verifiedCsv' AND source_snapshot_id IS NULL)
            OR
            (data_origin = 'developmentFixture' AND source_snapshot_id IS NULL)
        ),
    ADD CONSTRAINT product_revisions_product_origin_fk
        FOREIGN KEY (product_id, data_origin)
        REFERENCES products (id, data_origin)
        ON DELETE CASCADE;

-- News is a revision extension of the dedicated news content kind. The fixed
-- content_kind column and composite FK prevent attaching news metadata to a
-- non-news content entry.
CREATE UNIQUE INDEX content_entries_id_kind_unique
    ON content_entries (id, kind);

CREATE TABLE news (
    content_id uuid NOT NULL,
    revision bigint NOT NULL,
    content_kind text NOT NULL DEFAULT 'news' CHECK (content_kind = 'news'),
    category text NOT NULL CHECK (length(trim(category)) BETWEEN 1 AND 80),
    author_display_name text CHECK (
        author_display_name IS NULL
        OR length(trim(author_display_name)) BETWEEN 1 AND 160
    ),
    cover_media_asset_id uuid REFERENCES media_assets(id) ON DELETE RESTRICT,
    featured boolean NOT NULL DEFAULT false,
    publication_at timestamptz,
    reading_minutes integer CHECK (reading_minutes IS NULL OR reading_minutes > 0),
    PRIMARY KEY (content_id, revision),
    FOREIGN KEY (content_id, revision)
        REFERENCES content_revisions(content_id, revision)
        ON DELETE CASCADE,
    FOREIGN KEY (content_id, content_kind)
        REFERENCES content_entries(id, kind)
        ON DELETE CASCADE
);

CREATE INDEX news_category_publication_idx
    ON news (category, publication_at DESC, content_id);
CREATE INDEX news_featured_publication_idx
    ON news (publication_at DESC, content_id)
    WHERE featured;

CREATE VIEW published_news AS
SELECT entry.id,
       revision.payload->>'slug' AS slug,
       revision.payload->>'locale' AS locale,
       revision.payload->>'title' AS title,
       entry.is_placeholder,
       entry.data_origin,
       entry.published_revision,
       revision.payload,
       news.category,
       news.author_display_name,
       news.cover_media_asset_id,
       news.featured,
       news.publication_at,
       news.reading_minutes,
       revision.created_at AS revision_created_at
FROM content_entries AS entry
JOIN content_revisions AS revision
  ON revision.content_id = entry.id
 AND revision.revision = entry.published_revision
JOIN news
  ON news.content_id = entry.id
 AND news.revision = entry.published_revision
WHERE entry.kind = 'news'
  AND entry.published_revision IS NOT NULL;

-- General Information is a locale-aware, revisioned aggregate for company,
-- contact, social, brand asset and default SEO data. Operational settings stay
-- in app_settings and are intentionally not mixed into this document.
CREATE TABLE general_information (
    id uuid PRIMARY KEY,
    scope text NOT NULL DEFAULT 'site' CHECK (length(trim(scope)) BETWEEN 1 AND 80),
    locale text NOT NULL CHECK (length(trim(locale)) BETWEEN 2 AND 35),
    status text NOT NULL CHECK (status IN ('draft', 'scheduled', 'published', 'archived')),
    is_placeholder boolean NOT NULL DEFAULT false,
    data_origin text NOT NULL DEFAULT 'editorial'
        CHECK (data_origin IN ('editorial', 'developmentFixture')),
    current_revision bigint NOT NULL CHECK (current_revision > 0),
    published_revision bigint,
    scheduled_for timestamptz,
    payload jsonb NOT NULL CHECK (jsonb_typeof(payload) = 'object'),
    updated_by text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (scope, locale),
    CHECK (published_revision IS NULL OR published_revision <= current_revision),
    CHECK (data_origin <> 'developmentFixture' OR is_placeholder)
);

CREATE INDEX general_information_status_idx
    ON general_information (status, locale, scope);

CREATE TABLE general_information_revisions (
    general_information_id uuid NOT NULL
        REFERENCES general_information(id) ON DELETE CASCADE,
    revision bigint NOT NULL CHECK (revision > 0),
    payload jsonb NOT NULL CHECK (jsonb_typeof(payload) = 'object'),
    created_by text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (general_information_id, revision)
);

CREATE VIEW published_general_information AS
SELECT information.id,
       information.scope,
       information.locale,
       information.is_placeholder,
       information.data_origin,
       information.published_revision,
       revision.payload,
       revision.created_at AS revision_created_at
FROM general_information AS information
JOIN general_information_revisions AS revision
  ON revision.general_information_id = information.id
 AND revision.revision = information.published_revision
WHERE information.published_revision IS NOT NULL;

-- Localized presentation fields are attached to an exact immutable product
-- revision. This removes the one-locale-per-stable-id limitation for new reads
-- without changing the legacy products columns used by the current runtime.
CREATE TABLE product_localizations (
    product_id uuid NOT NULL,
    product_revision bigint NOT NULL,
    locale text NOT NULL CHECK (length(trim(locale)) BETWEEN 2 AND 35),
    slug text NOT NULL CHECK (length(trim(slug)) BETWEEN 1 AND 200),
    title text NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 300),
    summary text,
    content jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(content) = 'object'),
    seo_metadata jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(seo_metadata) = 'object'),
    translation_state text NOT NULL DEFAULT 'draft'
        CHECK (translation_state IN ('missing', 'draft', 'verified')),
    is_placeholder boolean NOT NULL DEFAULT false,
    indexable boolean NOT NULL DEFAULT false,
    data_origin text NOT NULL DEFAULT 'editorial'
        CHECK (
            data_origin IN ('editorial', 'feishu', 'verifiedCsv', 'developmentFixture')
        ),
    updated_by text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (product_id, product_revision, locale),
    FOREIGN KEY (product_id, product_revision)
        REFERENCES product_revisions(product_id, revision)
        ON DELETE CASCADE,
    CHECK (
        data_origin <> 'developmentFixture'
        OR (is_placeholder AND NOT indexable)
    )
);

CREATE INDEX product_localizations_route_idx
    ON product_localizations (locale, slug, product_id, product_revision);
CREATE INDEX product_localizations_search_idx
    ON product_localizations USING gin (
        to_tsvector('english', coalesce(title, '') || ' ' || coalesce(summary, ''))
    );

-- Product import transport is separated from the existing validated staging
-- projection. Private source rows are ciphertext-only at rest; normalized,
-- publishable data continues through staging_records and the publication gate.
CREATE TABLE product_import_runs (
    id uuid PRIMARY KEY,
    sync_run_id uuid UNIQUE REFERENCES sync_runs(id) ON DELETE SET NULL,
    connector_id uuid REFERENCES source_connectors(id) ON DELETE RESTRICT,
    data_origin text NOT NULL
        CHECK (data_origin IN ('feishu', 'verifiedCsv', 'developmentFixture')),
    dry_run boolean NOT NULL DEFAULT false,
    status text NOT NULL CHECK (
        status IN (
            'queued', 'receiving', 'validating', 'awaitingResolution',
            'readyToPublish', 'completed', 'failed', 'cancelled'
        )
    ),
    mapping_version text NOT NULL CHECK (length(trim(mapping_version)) > 0),
    source_checksum text NOT NULL CHECK (length(trim(source_checksum)) > 0),
    resume_cursor_ciphertext bytea,
    records_received bigint NOT NULL DEFAULT 0 CHECK (records_received >= 0),
    records_valid bigint NOT NULL DEFAULT 0 CHECK (records_valid >= 0),
    error_count bigint NOT NULL DEFAULT 0 CHECK (error_count >= 0),
    created_by uuid REFERENCES users(id) ON DELETE SET NULL,
    started_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (data_origin, source_checksum, mapping_version),
    UNIQUE (id, data_origin),
    CHECK (completed_at IS NULL OR completed_at >= started_at),
    CHECK (
        (data_origin = 'feishu' AND connector_id IS NOT NULL)
        OR
        (
            data_origin = 'verifiedCsv'
            AND connector_id IS NULL
            AND sync_run_id IS NULL
        )
        OR
        (
            data_origin = 'developmentFixture'
            AND connector_id IS NULL
            AND sync_run_id IS NULL
        )
    )
);

CREATE INDEX product_import_runs_status_idx
    ON product_import_runs (status, started_at DESC);

ALTER TABLE products
    ADD COLUMN product_import_run_id uuid,
    ADD CONSTRAINT products_import_origin_fk
        FOREIGN KEY (product_import_run_id, data_origin)
        REFERENCES product_import_runs(id, data_origin)
        ON DELETE RESTRICT,
    ADD CONSTRAINT products_verified_csv_import_check
        CHECK (
            (data_origin = 'verifiedCsv' AND product_import_run_id IS NOT NULL)
            OR
            (data_origin <> 'verifiedCsv' AND product_import_run_id IS NULL)
        );

CREATE UNIQUE INDEX products_id_origin_import_unique
    ON products (id, data_origin, product_import_run_id);

ALTER TABLE product_revisions
    ADD COLUMN product_import_run_id uuid,
    ADD CONSTRAINT product_revisions_import_origin_fk
        FOREIGN KEY (product_import_run_id, data_origin)
        REFERENCES product_import_runs(id, data_origin)
        ON DELETE RESTRICT,
    ADD CONSTRAINT product_revisions_verified_csv_import_check
        CHECK (
            (data_origin = 'verifiedCsv' AND product_import_run_id IS NOT NULL)
            OR
            (data_origin <> 'verifiedCsv' AND product_import_run_id IS NULL)
        );

-- Each immutable revision points to the import run that created it. The
-- working product row points to the newest accepted run; do not couple old
-- revisions to that mutable pointer or a later import could not append history.

CREATE TABLE product_import_private_staging (
    id uuid PRIMARY KEY,
    import_run_id uuid NOT NULL
        REFERENCES product_import_runs(id) ON DELETE CASCADE,
    source_record_id text NOT NULL CHECK (length(trim(source_record_id)) > 0),
    source_row_number integer NOT NULL CHECK (source_row_number > 1),
    ciphertext bytea NOT NULL CHECK (octet_length(ciphertext) > 0),
    encryption_algorithm text NOT NULL CHECK (length(trim(encryption_algorithm)) > 0),
    encryption_key_id text NOT NULL CHECK (length(trim(encryption_key_id)) > 0),
    nonce bytea NOT NULL CHECK (octet_length(nonce) >= 12),
    authentication_tag bytea NOT NULL CHECK (octet_length(authentication_tag) >= 12),
    checksum text NOT NULL CHECK (length(trim(checksum)) > 0),
    status text NOT NULL DEFAULT 'received'
        CHECK (status IN ('received', 'validated', 'promoted', 'rejected', 'expired')),
    promoted_staging_record_id uuid REFERENCES staging_records(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    processed_at timestamptz,
    UNIQUE (import_run_id, source_record_id),
    CHECK (expires_at > created_at),
    CHECK (processed_at IS NULL OR processed_at >= created_at)
);

CREATE INDEX product_import_private_staging_expiry_idx
    ON product_import_private_staging (expires_at)
    WHERE status IN ('received', 'validated', 'rejected');

CREATE TABLE product_import_normalized_records (
    id uuid PRIMARY KEY,
    import_run_id uuid NOT NULL
        REFERENCES product_import_runs(id) ON DELETE CASCADE,
    source_record_id text NOT NULL CHECK (length(trim(source_record_id)) > 0),
    normalized_payload jsonb NOT NULL CHECK (jsonb_typeof(normalized_payload) = 'object'),
    validation_status text NOT NULL DEFAULT 'pending'
        CHECK (validation_status IN ('pending', 'valid', 'invalid')),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (import_run_id, source_record_id),
    FOREIGN KEY (import_run_id, source_record_id)
        REFERENCES product_import_private_staging(import_run_id, source_record_id)
        ON DELETE CASCADE
);

CREATE INDEX product_import_normalized_status_idx
    ON product_import_normalized_records (import_run_id, validation_status, created_at);

CREATE TABLE product_import_errors (
    id uuid PRIMARY KEY,
    import_run_id uuid NOT NULL
        REFERENCES product_import_runs(id) ON DELETE CASCADE,
    private_staging_id uuid
        REFERENCES product_import_private_staging(id) ON DELETE CASCADE,
    source_record_id text,
    severity text NOT NULL CHECK (severity IN ('warning', 'error')),
    error_code text NOT NULL CHECK (length(trim(error_code)) > 0),
    field_path text,
    message text NOT NULL CHECK (length(trim(message)) > 0),
    details jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(details) = 'object'),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX product_import_errors_run_idx
    ON product_import_errors (import_run_id, severity, created_at);

CREATE TABLE product_import_missing_assets (
    id uuid PRIMARY KEY,
    import_run_id uuid NOT NULL
        REFERENCES product_import_runs(id) ON DELETE CASCADE,
    source_record_id text NOT NULL CHECK (length(trim(source_record_id)) > 0),
    asset_type text NOT NULL CHECK (length(trim(asset_type)) > 0),
    source_reference text NOT NULL CHECK (length(trim(source_reference)) > 0),
    field_path text,
    resolution_status text NOT NULL DEFAULT 'missing'
        CHECK (resolution_status IN ('missing', 'resolved', 'waived')),
    resolved_media_asset_id uuid REFERENCES media_assets(id) ON DELETE RESTRICT,
    resolution_reason text,
    created_at timestamptz NOT NULL DEFAULT now(),
    resolved_at timestamptz,
    UNIQUE (import_run_id, source_record_id, asset_type, source_reference),
    FOREIGN KEY (import_run_id, source_record_id)
        REFERENCES product_import_normalized_records(import_run_id, source_record_id)
        ON DELETE CASCADE,
    CHECK (
        (resolution_status = 'missing' AND resolved_media_asset_id IS NULL AND resolved_at IS NULL)
        OR
        (
            resolution_status = 'resolved'
            AND resolved_media_asset_id IS NOT NULL
            AND resolved_at IS NOT NULL
        )
        OR
        (
            resolution_status = 'waived'
            AND resolved_media_asset_id IS NULL
            AND resolved_at IS NOT NULL
            AND length(trim(resolution_reason)) >= 10
        )
    )
);

CREATE INDEX product_import_missing_assets_open_idx
    ON product_import_missing_assets (import_run_id, source_record_id, created_at)
    WHERE resolution_status = 'missing';

-- One strongly constrained reference table covers revision-specific CMS,
-- product and General Information assets without polymorphic orphan rows.
CREATE TABLE asset_references (
    id uuid PRIMARY KEY,
    media_asset_id uuid NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    content_id uuid,
    content_revision bigint,
    product_id uuid,
    product_revision bigint,
    general_information_id uuid,
    general_information_revision bigint,
    usage text NOT NULL CHECK (
        usage IN (
            'hero', 'cover', 'gallery', 'inline', 'download', 'datasheet',
            'cad', 'certificate', 'logo', 'favicon', 'social', 'other'
        )
    ),
    locale text,
    alt_text text,
    sort_order integer NOT NULL DEFAULT 0 CHECK (sort_order >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (num_nonnulls(content_id, product_id, general_information_id) = 1),
    CHECK (num_nonnulls(content_id, content_revision) IN (0, 2)),
    CHECK (num_nonnulls(product_id, product_revision) IN (0, 2)),
    CHECK (
        num_nonnulls(general_information_id, general_information_revision) IN (0, 2)
    ),
    FOREIGN KEY (content_id, content_revision)
        REFERENCES content_revisions(content_id, revision)
        MATCH FULL ON DELETE CASCADE,
    FOREIGN KEY (product_id, product_revision)
        REFERENCES product_revisions(product_id, revision)
        MATCH FULL ON DELETE CASCADE,
    FOREIGN KEY (general_information_id, general_information_revision)
        REFERENCES general_information_revisions(general_information_id, revision)
        MATCH FULL ON DELETE CASCADE
);

CREATE UNIQUE INDEX asset_references_content_unique
    ON asset_references (content_id, content_revision, usage, media_asset_id)
    WHERE content_id IS NOT NULL;
CREATE UNIQUE INDEX asset_references_product_unique
    ON asset_references (product_id, product_revision, usage, media_asset_id)
    WHERE product_id IS NOT NULL;
CREATE UNIQUE INDEX asset_references_general_information_unique
    ON asset_references (
        general_information_id,
        general_information_revision,
        usage,
        media_asset_id
    )
    WHERE general_information_id IS NOT NULL;

-- A guest visit is created only from an affirmative analytics consent record.
-- Only the external host and cleaned campaign dimensions are stored; raw IP,
-- user agent and complete referrer URLs are intentionally absent.
CREATE UNIQUE INDEX consent_records_id_session_allowed_unique
    ON consent_records (id, anonymous_session_id, analytics_allowed);

CREATE INDEX consent_records_session_time_idx
    ON consent_records (anonymous_session_id, granted_at DESC);
CREATE INDEX consent_records_expiry_idx
    ON consent_records (expires_at)
    WHERE expires_at IS NOT NULL;

CREATE TABLE guest_visits (
    id uuid PRIMARY KEY,
    anonymous_session_id uuid NOT NULL,
    consent_record_id uuid NOT NULL,
    consent_analytics_allowed boolean NOT NULL DEFAULT true
        CHECK (consent_analytics_allowed),
    locale text NOT NULL CHECK (length(trim(locale)) BETWEEN 2 AND 35),
    landing_path text NOT NULL CHECK (
        landing_path LIKE '/%'
        AND position('?' IN landing_path) = 0
        AND position('#' IN landing_path) = 0
        AND length(landing_path) <= 2048
    ),
    source_type text NOT NULL CHECK (
        source_type IN (
            'direct', 'organicSearch', 'paidSearch', 'referral',
            'social', 'email', 'other', 'unknown'
        )
    ),
    referrer_host text CHECK (
        referrer_host IS NULL
        OR (
            length(referrer_host) BETWEEN 1 AND 253
            AND referrer_host !~ '[/\\?#@]'
        )
    ),
    utm_source text CHECK (utm_source IS NULL OR length(utm_source) <= 128),
    utm_medium text CHECK (utm_medium IS NULL OR length(utm_medium) <= 128),
    utm_campaign text CHECK (utm_campaign IS NULL OR length(utm_campaign) <= 200),
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    retention_until timestamptz NOT NULL DEFAULT (now() + interval '180 days'),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, anonymous_session_id, consent_record_id),
    FOREIGN KEY (consent_record_id, anonymous_session_id, consent_analytics_allowed)
        REFERENCES consent_records(id, anonymous_session_id, analytics_allowed)
        ON DELETE RESTRICT,
    CHECK (last_seen_at >= first_seen_at),
    CHECK (retention_until >= first_seen_at)
);

CREATE INDEX guest_visits_session_time_idx
    ON guest_visits (anonymous_session_id, first_seen_at DESC);
CREATE INDEX guest_visits_source_time_idx
    ON guest_visits (source_type, first_seen_at DESC);
CREATE INDEX guest_visits_retention_idx
    ON guest_visits (retention_until);

ALTER TABLE analytics_events
    ADD COLUMN guest_visit_id uuid,
    ADD COLUMN consent_record_id uuid,
    ADD CONSTRAINT analytics_events_guest_consent_pair_check
        CHECK (
            (guest_visit_id IS NULL AND consent_record_id IS NULL)
            OR
            (
                guest_visit_id IS NOT NULL
                AND consent_record_id IS NOT NULL
                AND anonymous_session_id IS NOT NULL
            )
        ),
    ADD CONSTRAINT analytics_events_guest_consent_fk
        FOREIGN KEY (guest_visit_id, anonymous_session_id, consent_record_id)
        REFERENCES guest_visits(id, anonymous_session_id, consent_record_id)
        ON DELETE RESTRICT;

CREATE INDEX analytics_events_guest_time_idx
    ON analytics_events (guest_visit_id, occurred_at DESC)
    WHERE guest_visit_id IS NOT NULL;
CREATE INDEX analytics_events_consent_time_idx
    ON analytics_events (consent_record_id, occurred_at DESC)
    WHERE consent_record_id IS NOT NULL;
CREATE INDEX analytics_events_occurred_brin_idx
    ON analytics_events USING brin (occurred_at);

CREATE TABLE guest_source_daily (
    bucket_date date NOT NULL,
    source_type text NOT NULL CHECK (
        source_type IN (
            'direct', 'organicSearch', 'paidSearch', 'referral',
            'social', 'email', 'other', 'unknown'
        )
    ),
    source_name text NOT NULL DEFAULT '',
    utm_source text NOT NULL DEFAULT '',
    utm_medium text NOT NULL DEFAULT '',
    utm_campaign text NOT NULL DEFAULT '',
    landing_path text NOT NULL DEFAULT '',
    locale text NOT NULL,
    visits bigint NOT NULL DEFAULT 0 CHECK (visits >= 0),
    page_views bigint NOT NULL DEFAULT 0 CHECK (page_views >= 0),
    engaged_visits bigint NOT NULL DEFAULT 0 CHECK (engaged_visits >= 0),
    rfq_starts bigint NOT NULL DEFAULT 0 CHECK (rfq_starts >= 0),
    rfq_submissions bigint NOT NULL DEFAULT 0 CHECK (rfq_submissions >= 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (
        bucket_date,
        source_type,
        source_name,
        utm_source,
        utm_medium,
        utm_campaign,
        landing_path,
        locale
    ),
    CHECK (length(source_name) <= 253),
    CHECK (length(utm_source) <= 128),
    CHECK (length(utm_medium) <= 128),
    CHECK (length(utm_campaign) <= 200),
    CHECK (length(landing_path) <= 2048)
);

CREATE INDEX guest_source_daily_source_idx
    ON guest_source_daily (source_type, source_name, bucket_date DESC);

INSERT INTO app_settings (key, value, updated_by) VALUES
    ('guestVisitRetentionDays', '180'::jsonb, 'migration'),
    ('analyticsEventRetentionDays', '180'::jsonb, 'migration'),
    ('guestSourceAggregateRetentionMonths', '24'::jsonb, 'migration')
ON CONFLICT (key) DO NOTHING;

-- Invitation tokens are hash-only. Role assignment is captured before account
-- activation, and status history provides an auditable state transition record.
ALTER TABLE users
    ADD COLUMN revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0);
ALTER TABLE roles
    ADD COLUMN revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0);

CREATE TABLE user_invitations (
    id uuid PRIMARY KEY,
    email text NOT NULL CHECK (length(trim(email)) BETWEEN 3 AND 254),
    display_name text NOT NULL CHECK (length(trim(display_name)) BETWEEN 1 AND 200),
    locale text NOT NULL DEFAULT 'zh-CN',
    status text NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'revoked', 'expired')),
    token_hash bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) >= 32),
    invited_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    invited_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    accepted_at timestamptz,
    accepted_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    revoked_at timestamptz,
    revoked_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    revoke_reason text,
    CHECK (expires_at > invited_at),
    CHECK (
        (accepted_at IS NULL AND accepted_user_id IS NULL)
        OR
        (accepted_at IS NOT NULL AND accepted_user_id IS NOT NULL)
    ),
    CHECK (accepted_at IS NULL OR accepted_at >= invited_at),
    CHECK (revoked_at IS NULL OR revoked_at >= invited_at),
    CHECK (accepted_at IS NULL OR revoked_at IS NULL),
    CHECK (revoked_at IS NULL OR length(trim(revoke_reason)) >= 10),
    CHECK (
        (status = 'pending' AND accepted_at IS NULL AND revoked_at IS NULL)
        OR
        (status = 'accepted' AND accepted_at IS NOT NULL AND revoked_at IS NULL)
        OR
        (status = 'revoked' AND accepted_at IS NULL AND revoked_at IS NOT NULL)
        OR
        (status = 'expired' AND accepted_at IS NULL AND revoked_at IS NULL)
    )
);

CREATE UNIQUE INDEX user_invitations_active_email_unique
    ON user_invitations (lower(email))
    WHERE status = 'pending';
CREATE INDEX user_invitations_expiry_idx
    ON user_invitations (expires_at)
    WHERE status = 'pending';

CREATE TABLE user_invitation_roles (
    invitation_id uuid NOT NULL
        REFERENCES user_invitations(id) ON DELETE CASCADE,
    role_id uuid NOT NULL REFERENCES roles(id) ON DELETE RESTRICT,
    PRIMARY KEY (invitation_id, role_id)
);

CREATE TABLE user_status_history (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    from_status text CHECK (from_status IN ('invited', 'active', 'disabled')),
    to_status text NOT NULL CHECK (to_status IN ('invited', 'active', 'disabled')),
    reason text NOT NULL CHECK (length(trim(reason)) >= 10),
    changed_by uuid REFERENCES users(id) ON DELETE SET NULL,
    request_id uuid NOT NULL,
    changed_at timestamptz NOT NULL DEFAULT now(),
    CHECK (from_status IS NULL OR from_status <> to_status)
);

CREATE INDEX users_status_idx ON users (status, created_at DESC);
CREATE INDEX user_status_history_user_idx
    ON user_status_history (user_id, changed_at DESC);

-- Stable role keys are reference data. Existing installations retain their role
-- IDs. Super Admin is protected; the other templates remain customizable.
INSERT INTO roles (id, key, display_name, system_role) VALUES
    ('a17e0000-0000-4000-8000-000000000001', 'super-admin', 'Super Admin', true),
    ('a17e0000-0000-4000-8000-000000000002', 'content-editor', 'Content Editor', false),
    ('a17e0000-0000-4000-8000-000000000003', 'publisher', 'Publisher', false),
    ('a17e0000-0000-4000-8000-000000000004', 'product-manager', 'Product Manager', false),
    ('a17e0000-0000-4000-8000-000000000005', 'integration-operator', 'Integration Operator', false),
    ('a17e0000-0000-4000-8000-000000000006', 'rfq-operator', 'RFQ Operator', false),
    ('a17e0000-0000-4000-8000-000000000007', 'analyst', 'Analyst', false),
    ('a17e0000-0000-4000-8000-000000000008', 'auditor', 'Auditor', false),
    ('a17e0000-0000-4000-8000-000000000009', 'developer', 'Developer', false)
ON CONFLICT (key) DO UPDATE SET
    display_name = EXCLUDED.display_name,
    system_role = EXCLUDED.system_role;

INSERT INTO role_permissions (role_id, permission_key)
SELECT role.id, template.permission_key
FROM (
    VALUES
        ('super-admin', 'dashboard.read'),
        ('super-admin', 'content.read'),
        ('super-admin', 'content.write'),
        ('super-admin', 'content.publish'),
        ('super-admin', 'product.read'),
        ('super-admin', 'product.write'),
        ('super-admin', 'product.publish'),
        ('super-admin', 'integration.run'),
        ('super-admin', 'media.write'),
        ('super-admin', 'rfq.read'),
        ('super-admin', 'rfq.read_pii'),
        ('super-admin', 'rfq.assign'),
        ('super-admin', 'analytics.read'),
        ('super-admin', 'identity.manage'),
        ('super-admin', 'audit.read'),
        ('super-admin', 'settings.manage'),
        ('super-admin', 'operations.run'),
        ('super-admin', 'devtools.shell'),
        ('content-editor', 'dashboard.read'),
        ('content-editor', 'content.read'),
        ('content-editor', 'content.write'),
        ('content-editor', 'media.write'),
        ('publisher', 'dashboard.read'),
        ('publisher', 'content.read'),
        ('publisher', 'content.write'),
        ('publisher', 'content.publish'),
        ('publisher', 'media.write'),
        ('product-manager', 'dashboard.read'),
        ('product-manager', 'content.read'),
        ('product-manager', 'product.read'),
        ('product-manager', 'product.write'),
        ('product-manager', 'product.publish'),
        ('product-manager', 'media.write'),
        ('integration-operator', 'dashboard.read'),
        ('integration-operator', 'product.read'),
        ('integration-operator', 'integration.run'),
        ('integration-operator', 'operations.run'),
        ('rfq-operator', 'dashboard.read'),
        ('rfq-operator', 'rfq.read'),
        ('rfq-operator', 'rfq.read_pii'),
        ('rfq-operator', 'rfq.assign'),
        ('analyst', 'dashboard.read'),
        ('analyst', 'analytics.read'),
        ('analyst', 'content.read'),
        ('analyst', 'product.read'),
        ('auditor', 'dashboard.read'),
        ('auditor', 'content.read'),
        ('auditor', 'product.read'),
        ('auditor', 'audit.read'),
        ('developer', 'dashboard.read'),
        ('developer', 'audit.read'),
        ('developer', 'settings.manage'),
        ('developer', 'operations.run'),
        ('developer', 'devtools.shell')
) AS template(role_key, permission_key)
JOIN roles AS role ON role.key = template.role_key
JOIN permissions AS permission ON permission.key = template.permission_key
ON CONFLICT (role_id, permission_key) DO NOTHING;

-- The ledger makes fixture upserts deterministic and ownership-safe. It does not
-- contain business values and is only intended for the development-only seed
-- binary, which must remain absent from production builds.
CREATE TABLE development_fixture_ledger (
    fixture_key text PRIMARY KEY CHECK (fixture_key LIKE 'development/%'),
    entity_type text NOT NULL CHECK (
        entity_type IN ('content', 'news', 'product', 'generalInformation')
    ),
    entity_id uuid NOT NULL,
    locale text,
    seed_version integer NOT NULL CHECK (seed_version > 0),
    checksum text NOT NULL CHECK (length(trim(checksum)) > 0),
    loaded_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (entity_type, entity_id)
);

COMMENT ON TABLE news IS
    'Revision-specific metadata for content_entries of kind news; public URLs use /resources/news.';
COMMENT ON TABLE product_import_private_staging IS
    'Ciphertext-only transient source rows. Plain source payloads must not be written to this table.';
COMMENT ON TABLE guest_visits IS
    'Consent-bound, data-minimized first-party visit attribution. No raw IP, user agent or referrer URL.';
COMMENT ON TABLE development_fixture_ledger IS
    'Development seed ownership ledger; fixture rows must also carry developmentFixture data_origin.';

-- Product Master CSV validation is exposed through the existing durable
-- operation/SSE mechanism rather than a second, incompatible job protocol.
ALTER TABLE operation_runs DROP CONSTRAINT operation_runs_kind_check;
ALTER TABLE operation_runs ADD CONSTRAINT operation_runs_kind_check CHECK (
    kind IN (
        'migrationPreflight','migrationApply','backup','restoreValidate',
        'retentionApply','searchReindex','cacheInvalidate','feishuSync','productImport'
    )
);

-- Confidential commercial data is never included in the ordinary product
-- read permission. A future private-pricing endpoint must require this key at
-- the service boundary; only Super Admin receives it by default.
INSERT INTO permissions(key, description) VALUES
    ('product.pricing.read', 'Read encrypted or decrypted private Product Master pricing')
ON CONFLICT (key) DO NOTHING;
INSERT INTO role_permissions(role_id, permission_key)
SELECT id, 'product.pricing.read' FROM roles WHERE key='super-admin'
ON CONFLICT (role_id, permission_key) DO NOTHING;

-- Explicit revision-scoped operating conditions preserve the one-to-one
-- relationship used to interpret slash-separated Product Master values.
CREATE TABLE product_operating_conditions (
    product_id uuid NOT NULL,
    product_revision bigint NOT NULL,
    key text NOT NULL CHECK (length(trim(key)) > 0),
    label text NOT NULL CHECK (length(trim(label)) > 0),
    frequency_hz numeric,
    voltage text,
    fact_state text NOT NULL CHECK (
        fact_state IN (
            'verified','missing','notApplicable','notTested',
            'confidential','pendingVerification'
        )
    ),
    source_reference text NOT NULL,
    payload jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(payload)='object'),
    PRIMARY KEY(product_id, product_revision, key),
    FOREIGN KEY(product_id, product_revision)
        REFERENCES product_revisions(product_id, revision) ON DELETE CASCADE
);
