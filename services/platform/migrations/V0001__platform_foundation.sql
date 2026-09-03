-- Flyway versioned migration. Never edit after release; add a new migration.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE app_settings (
    key text PRIMARY KEY,
    value jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    updated_by text NOT NULL
);

CREATE TABLE roles (
    id uuid PRIMARY KEY,
    key text NOT NULL UNIQUE,
    display_name text NOT NULL,
    system_role boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE permissions (
    key text PRIMARY KEY,
    description text NOT NULL
);

CREATE TABLE role_permissions (
    role_id uuid NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_key text NOT NULL REFERENCES permissions(key) ON DELETE CASCADE,
    PRIMARY KEY (role_id, permission_key)
);

CREATE TABLE users (
    id uuid PRIMARY KEY,
    email text NOT NULL,
    password_hash text NOT NULL,
    display_name text NOT NULL,
    locale text NOT NULL DEFAULT 'zh-CN',
    status text NOT NULL DEFAULT 'invited' CHECK (status IN ('invited','active','disabled')),
    totp_secret_ciphertext bytea,
    totp_confirmed_at timestamptz,
    invited_by uuid REFERENCES users(id),
    invited_at timestamptz,
    last_login_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email_unique ON users (lower(email));

CREATE TABLE user_roles (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id uuid NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE sessions (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    csrf_hash bytea NOT NULL,
    ip_hash bytea,
    user_agent_hash bytea,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    last_seen_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sessions_user_active_idx ON sessions (user_id, expires_at) WHERE revoked_at IS NULL;

CREATE TABLE auth_rate_limits (
    key_hash text PRIMARY KEY,
    attempts integer NOT NULL CHECK (attempts >= 0),
    window_started_at timestamptz NOT NULL,
    blocked_until timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX auth_rate_limits_expiry_idx ON auth_rate_limits (blocked_until, updated_at);

CREATE TABLE recovery_codes (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash bytea NOT NULL,
    used_at timestamptz,
    PRIMARY KEY (user_id, code_hash)
);

CREATE TABLE content_entries (
    id uuid PRIMARY KEY,
    kind text NOT NULL CHECK (kind IN ('home','solution','technology','article','faq','caseStudy','download','company','legal','navigation','footer')),
    slug text NOT NULL,
    locale text NOT NULL,
    title text NOT NULL,
    status text NOT NULL CHECK (status IN ('draft','scheduled','published','archived')),
    is_placeholder boolean NOT NULL DEFAULT false,
    current_revision bigint NOT NULL CHECK (current_revision > 0),
    published_revision bigint,
    scheduled_for timestamptz,
    payload jsonb NOT NULL,
    updated_at timestamptz NOT NULL,
    UNIQUE (kind, slug, locale),
    CHECK (published_revision IS NULL OR published_revision <= current_revision)
);
CREATE INDEX content_entries_status_idx ON content_entries (status, locale, kind);
CREATE INDEX content_entries_search_idx ON content_entries USING gin (
    to_tsvector('english', coalesce(title, '') || ' ' || coalesce(payload->>'summary', ''))
);
CREATE INDEX content_entries_slug_trgm_idx ON content_entries USING gin (slug gin_trgm_ops);

CREATE TABLE content_revisions (
    content_id uuid NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    revision bigint NOT NULL,
    payload jsonb NOT NULL,
    created_by text NOT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (content_id, revision)
);

CREATE TABLE content_relations (
    from_type text NOT NULL,
    from_id uuid NOT NULL,
    relation_type text NOT NULL,
    to_type text NOT NULL,
    to_id uuid NOT NULL,
    sort_order integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (from_type, from_id, relation_type, to_type, to_id)
);

CREATE TABLE public_routes (
    id uuid PRIMARY KEY,
    entity_type text NOT NULL,
    entity_id uuid NOT NULL,
    locale text NOT NULL,
    canonical_path text NOT NULL UNIQUE,
    indexable boolean NOT NULL DEFAULT false,
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (entity_type, entity_id, locale)
);

CREATE TABLE redirects (
    source_path text PRIMARY KEY,
    destination_path text NOT NULL,
    status_code integer NOT NULL CHECK (status_code IN (301, 302, 307, 308)),
    enabled boolean NOT NULL DEFAULT true,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (source_path <> destination_path)
);

CREATE VIEW published_content AS
SELECT entry.id,
       revision.payload->>'kind' AS kind,
       revision.payload->>'slug' AS slug,
       revision.payload->>'locale' AS locale,
       revision.payload->>'title' AS title,
       entry.published_revision,
       revision.payload,
       (revision.payload->>'updatedAt')::timestamptz AS updated_at
FROM content_entries AS entry
JOIN content_revisions AS revision
  ON revision.content_id = entry.id
 AND revision.revision = entry.published_revision
WHERE entry.published_revision IS NOT NULL;

CREATE TABLE source_connectors (
    id uuid PRIMARY KEY,
    connector_type text NOT NULL CHECK (connector_type IN ('feishu')),
    display_name text NOT NULL,
    encrypted_configuration bytea,
    enabled boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sync_mappings (
    id uuid PRIMARY KEY,
    connector_id uuid NOT NULL REFERENCES source_connectors(id) ON DELETE CASCADE,
    version text NOT NULL,
    mapping jsonb NOT NULL,
    schema_version integer NOT NULL DEFAULT 1,
    active boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (connector_id, version)
);

CREATE TABLE sync_runs (
    id uuid PRIMARY KEY,
    source text NOT NULL CHECK (source = 'feishu'),
    dry_run boolean NOT NULL,
    mapping_version text NOT NULL,
    status text NOT NULL CHECK (status IN ('queued','fetching','validating','awaitingResolution','readyToPublish','completed','failed')),
    resume_cursor text,
    records_seen bigint NOT NULL DEFAULT 0 CHECK (records_seen >= 0),
    records_valid bigint NOT NULL DEFAULT 0 CHECK (records_valid >= 0),
    conflict_count bigint NOT NULL DEFAULT 0 CHECK (conflict_count >= 0),
    started_at timestamptz NOT NULL,
    completed_at timestamptz,
    payload jsonb NOT NULL
);
CREATE INDEX sync_runs_status_idx ON sync_runs (status, started_at DESC);

CREATE TABLE source_snapshots (
    id uuid PRIMARY KEY,
    connector_id uuid REFERENCES source_connectors(id),
    sync_run_id uuid REFERENCES sync_runs(id),
    source_record_id text NOT NULL,
    source_revision text NOT NULL,
    checksum text NOT NULL,
    source_payload jsonb NOT NULL,
    received_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (connector_id, source_record_id, source_revision)
);

CREATE TABLE staging_records (
    id uuid PRIMARY KEY,
    sync_run_id uuid NOT NULL REFERENCES sync_runs(id) ON DELETE CASCADE,
    source_snapshot_id uuid NOT NULL REFERENCES source_snapshots(id),
    source_record_id text NOT NULL,
    validation_status text NOT NULL CHECK (validation_status IN ('pending','valid','invalid','conflicted')),
    normalized_payload jsonb,
    validation_errors jsonb NOT NULL DEFAULT '[]'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (sync_run_id, source_record_id)
);

CREATE TABLE products (
    id uuid PRIMARY KEY,
    stable_id text NOT NULL UNIQUE,
    model text,
    slug text NOT NULL,
    locale text NOT NULL,
    family text NOT NULL CHECK (family IN ('centrifugal','axial','crossFlow','inlineDuct','motors')),
    source_snapshot_id uuid NOT NULL REFERENCES source_snapshots(id),
    source_revision text NOT NULL,
    status text NOT NULL CHECK (status IN ('draft','scheduled','published','archived')),
    current_revision bigint NOT NULL CHECK (current_revision > 0),
    published_revision bigint,
    indexable boolean NOT NULL DEFAULT false,
    payload jsonb NOT NULL,
    updated_at timestamptz NOT NULL,
    UNIQUE (slug, locale),
    CHECK (published_revision IS NULL OR published_revision <= current_revision)
);
CREATE INDEX products_family_status_idx ON products (family, status, locale);
CREATE INDEX products_model_trgm_idx ON products USING gin (model gin_trgm_ops);
CREATE INDEX products_payload_idx ON products USING gin (payload jsonb_path_ops);

CREATE TABLE product_revisions (
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    revision bigint NOT NULL,
    source_snapshot_id uuid NOT NULL REFERENCES source_snapshots(id),
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (product_id, revision)
);

CREATE TABLE product_specs (
    product_id uuid NOT NULL,
    product_revision bigint NOT NULL,
    key text NOT NULL,
    label text NOT NULL,
    numeric_value numeric,
    text_value text,
    unit text,
    operating_condition text,
    fact_state text NOT NULL CHECK (fact_state IN ('verified','missing','notApplicable','notTested','confidential','pendingVerification')),
    source_reference text,
    PRIMARY KEY (product_id, product_revision, key),
    FOREIGN KEY (product_id, product_revision) REFERENCES product_revisions(product_id, revision) ON DELETE CASCADE,
    CHECK (numeric_value IS NULL OR text_value IS NULL)
);

CREATE TABLE performance_curves (
    id uuid PRIMARY KEY,
    product_id uuid NOT NULL,
    product_revision bigint NOT NULL,
    airflow_unit text NOT NULL,
    pressure_unit text NOT NULL,
    speed_rpm integer,
    density_kg_m3 numeric,
    voltage text,
    test_method text,
    source_reference text NOT NULL,
    fact_state text NOT NULL CHECK (fact_state IN ('verified','missing','notApplicable','notTested','confidential','pendingVerification')),
    points jsonb NOT NULL,
    FOREIGN KEY (product_id, product_revision) REFERENCES product_revisions(product_id, revision) ON DELETE CASCADE
);

CREATE TABLE product_assets (
    id uuid PRIMARY KEY,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    asset_type text NOT NULL,
    locale text,
    revision text NOT NULL,
    storage_key text NOT NULL UNIQUE,
    checksum text NOT NULL,
    scan_status text NOT NULL CHECK (scan_status IN ('pending','clean','quarantined','failed')),
    access_level text NOT NULL CHECK (access_level IN ('public','authenticated','internal')),
    source_reference text,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE product_temporary_overrides (
    id uuid PRIMARY KEY,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    field_path text NOT NULL,
    value jsonb NOT NULL,
    reason text NOT NULL CHECK (length(trim(reason)) >= 10),
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    resolved_at timestamptz,
    resolved_by text,
    CHECK (expires_at > created_at)
);
CREATE INDEX product_override_active_idx ON product_temporary_overrides (product_id, expires_at) WHERE resolved_at IS NULL;

CREATE TABLE sync_conflicts (
    id uuid PRIMARY KEY,
    sync_run_id uuid NOT NULL REFERENCES sync_runs(id) ON DELETE CASCADE,
    product_id uuid REFERENCES products(id),
    source_record_id text NOT NULL,
    base_value jsonb,
    local_value jsonb,
    incoming_value jsonb,
    field_diffs jsonb NOT NULL,
    resolved_at timestamptz,
    resolution text,
    resolved_by text
);
CREATE INDEX sync_conflicts_open_idx ON sync_conflicts (sync_run_id) WHERE resolved_at IS NULL;

CREATE VIEW published_products AS
SELECT product.id,
       revision.payload->>'stableId' AS stable_id,
       revision.payload->>'model' AS model,
       revision.payload->>'slug' AS slug,
       revision.payload->>'locale' AS locale,
       revision.payload->>'family' AS family,
       product.published_revision,
       revision.payload,
       (revision.payload->>'updatedAt')::timestamptz AS updated_at
FROM products AS product
JOIN product_revisions AS revision
  ON revision.product_id = product.id
 AND revision.revision = product.published_revision
WHERE product.published_revision IS NOT NULL;

CREATE TABLE media_assets (
    id uuid PRIMARY KEY,
    storage_key text NOT NULL UNIQUE,
    original_name text NOT NULL,
    media_type text NOT NULL,
    byte_size bigint NOT NULL CHECK (byte_size >= 0),
    checksum text NOT NULL,
    scan_status text NOT NULL CHECK (scan_status IN ('pending','clean','quarantined','failed')),
    access_level text NOT NULL CHECK (access_level IN ('public','authenticated','internal')),
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE TABLE rfq_submissions (
    id uuid PRIMARY KEY,
    reference text NOT NULL UNIQUE,
    journey text NOT NULL CHECK (journey IN ('product','selection','project','replacement')),
    status text NOT NULL,
    source_path text NOT NULL,
    locale text NOT NULL,
    submitted_at timestamptz NOT NULL,
    retention_until timestamptz NOT NULL,
    payload jsonb NOT NULL
);
CREATE INDEX rfq_status_idx ON rfq_submissions (status, submitted_at DESC);
CREATE INDEX rfq_retention_idx ON rfq_submissions (retention_until);

CREATE TABLE contact_requests (
    id uuid PRIMARY KEY,
    reference text NOT NULL UNIQUE,
    topic text NOT NULL,
    status text NOT NULL,
    source_path text NOT NULL,
    locale text NOT NULL,
    submitted_at timestamptz NOT NULL,
    retention_until timestamptz NOT NULL,
    payload jsonb NOT NULL
);
CREATE INDEX contact_status_idx ON contact_requests (status, submitted_at DESC);
CREATE INDEX contact_retention_idx ON contact_requests (retention_until);

CREATE TABLE business_status_history (
    id uuid PRIMARY KEY,
    entity_type text NOT NULL CHECK (entity_type IN ('rfq','contact')),
    entity_id uuid NOT NULL,
    from_status text,
    to_status text NOT NULL,
    note text,
    changed_by text NOT NULL,
    changed_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE analytics_events (
    id uuid PRIMARY KEY,
    event_name text NOT NULL,
    source_path text NOT NULL,
    locale text NOT NULL,
    anonymous_session_id uuid,
    properties jsonb NOT NULL DEFAULT '{}'::jsonb,
    occurred_at timestamptz NOT NULL
);
CREATE INDEX analytics_event_time_idx ON analytics_events (event_name, occurred_at DESC);

CREATE TABLE analytics_aggregates (
    bucket_start timestamptz NOT NULL,
    bucket_interval text NOT NULL,
    metric text NOT NULL,
    dimensions jsonb NOT NULL DEFAULT '{}'::jsonb,
    value bigint NOT NULL DEFAULT 0,
    PRIMARY KEY (bucket_start, bucket_interval, metric, dimensions)
);

CREATE TABLE consent_records (
    id uuid PRIMARY KEY,
    anonymous_session_id uuid NOT NULL,
    policy_version text NOT NULL,
    analytics_allowed boolean NOT NULL,
    granted_at timestamptz NOT NULL,
    expires_at timestamptz
);

CREATE TABLE idempotency_keys (
    scope text NOT NULL,
    key_hash text NOT NULL,
    request_hash text NOT NULL,
    response_status integer NOT NULL,
    response_body jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL DEFAULT now() + interval '24 hours',
    PRIMARY KEY (scope, key_hash)
);

CREATE TABLE operation_runs (
    id uuid PRIMARY KEY,
    kind text NOT NULL CHECK (kind IN ('migrationPreflight','migrationApply','backup','restoreValidate','retentionApply','searchReindex','cacheInvalidate','feishuSync')),
    status text NOT NULL CHECK (status IN ('queued','running','completed','failed')),
    reason text NOT NULL,
    result jsonb,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

CREATE TABLE jobs (
    id uuid PRIMARY KEY,
    job_type text NOT NULL,
    status text NOT NULL CHECK (status IN ('queued','running','completed','failed')),
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    result jsonb,
    attempts integer NOT NULL DEFAULT 0,
    max_attempts integer NOT NULL DEFAULT 5,
    available_at timestamptz NOT NULL,
    last_error text,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);
CREATE INDEX jobs_claim_idx ON jobs (available_at, created_at) WHERE status = 'queued';

CREATE TABLE outbox_events (
    id uuid PRIMARY KEY,
    topic text NOT NULL,
    aggregate_type text NOT NULL,
    aggregate_id uuid NOT NULL,
    payload jsonb NOT NULL,
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','processing','completed','failed')),
    attempts integer NOT NULL DEFAULT 0,
    max_attempts integer NOT NULL DEFAULT 5,
    available_at timestamptz NOT NULL DEFAULT now(),
    locked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    processed_at timestamptz
);
CREATE INDEX outbox_claim_idx ON outbox_events (available_at, created_at)
WHERE status IN ('pending', 'processing');
CREATE UNIQUE INDEX outbox_publish_revision_unique
ON outbox_events (topic, aggregate_id, (payload->>'revision'))
WHERE topic IN ('public.content.published', 'public.product.published');

CREATE TABLE audit_log (
    id uuid PRIMARY KEY,
    actor text NOT NULL,
    action text NOT NULL,
    entity_type text NOT NULL,
    entity_id uuid,
    before_value jsonb,
    after_value jsonb,
    reason text,
    request_id uuid NOT NULL,
    occurred_at timestamptz NOT NULL
);
CREATE INDEX audit_entity_idx ON audit_log (entity_type, entity_id, occurred_at DESC);
CREATE INDEX audit_actor_idx ON audit_log (actor, occurred_at DESC);

INSERT INTO permissions (key, description) VALUES
    ('dashboard.read', 'Read dashboard summaries'),
    ('content.read', 'Read content and revisions'),
    ('content.write', 'Create and edit drafts'),
    ('content.publish', 'Publish and roll back content'),
    ('product.read', 'Read product working records'),
    ('product.write', 'Edit site fields and create temporary source-field overrides'),
    ('product.publish', 'Publish validated products'),
    ('integration.run', 'Configure and run Feishu synchronization'),
    ('media.write', 'Manage media and downloads'),
    ('rfq.read', 'Read RFQ and contact records'),
    ('rfq.read_pii', 'Read protected personal fields in RFQ and contact records'),
    ('rfq.assign', 'Assign and update business records'),
    ('analytics.read', 'Read first-party analytics'),
    ('identity.manage', 'Manage users and roles'),
    ('audit.read', 'Read the immutable audit trail'),
    ('settings.manage', 'Manage platform settings'),
    ('operations.run', 'Run predefined operational tasks'),
    ('devtools.shell', 'Open the development-only host PTY')
ON CONFLICT (key) DO NOTHING;

INSERT INTO app_settings (key, value, updated_by) VALUES
    ('rfqRetentionDays', '365'::jsonb, 'migration'),
    ('retentionDeletionGraceDays', '30'::jsonb, 'migration'),
    ('temporaryOverrideDefaultDays', '30'::jsonb, 'migration'),
    ('publicLocale', '"en"'::jsonb, 'migration')
ON CONFLICT (key) DO NOTHING;
