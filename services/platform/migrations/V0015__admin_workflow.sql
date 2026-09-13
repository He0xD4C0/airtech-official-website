-- Controlled RFQ/contact workflow and query indexes. Unknown existing states
-- abort the migration instead of being silently rewritten.

DO $$
DECLARE unknown_states text;
BEGIN
    SELECT string_agg(DISTINCT status,', ' ORDER BY status)
    INTO unknown_states
    FROM (
        SELECT status FROM rfq_submissions
        UNION ALL
        SELECT status FROM contact_requests
    ) states
    WHERE status NOT IN ('new','triaged','assigned','qualified','closed','spam');
    IF unknown_states IS NOT NULL THEN
        RAISE EXCEPTION 'Unknown RFQ/contact status values: %',unknown_states;
    END IF;
END
$$;

ALTER TABLE rfq_submissions
    ADD COLUMN revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    ADD COLUMN assigned_to uuid REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN updated_at timestamptz NOT NULL DEFAULT now(),
    ADD CONSTRAINT rfq_status_controlled_check
        CHECK (status IN ('new','triaged','assigned','qualified','closed','spam'));

ALTER TABLE contact_requests
    ADD COLUMN revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    ADD COLUMN assigned_to uuid REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN updated_at timestamptz NOT NULL DEFAULT now(),
    ADD CONSTRAINT contact_status_controlled_check
        CHECK (status IN ('new','triaged','assigned','qualified','closed','spam'));

CREATE TABLE business_internal_notes (
    id uuid PRIMARY KEY,
    entity_type text NOT NULL CHECK (entity_type IN ('rfq','contact')),
    entity_id uuid NOT NULL,
    body text NOT NULL CHECK (length(trim(body)) BETWEEN 1 AND 4000),
    created_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE jobs
    ADD COLUMN lease_owner text,
    ADD COLUMN lease_expires_at timestamptz;

DROP INDEX jobs_claim_idx;
CREATE INDEX jobs_claim_idx
    ON jobs (status,available_at,created_at)
    WHERE status IN ('queued','running');

CREATE FUNCTION protect_business_internal_note()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION USING ERRCODE='55000',
        MESSAGE='business internal notes are immutable';
END
$$;

CREATE TRIGGER business_internal_notes_immutable_guard
BEFORE UPDATE OR DELETE ON business_internal_notes
FOR EACH ROW EXECUTE FUNCTION protect_business_internal_note();

CREATE INDEX rfq_assignee_status_updated_idx
    ON rfq_submissions (assigned_to,status,updated_at DESC,id DESC);
CREATE INDEX contact_assignee_status_updated_idx
    ON contact_requests (assigned_to,status,updated_at DESC,id DESC);
CREATE INDEX business_notes_entity_created_idx
    ON business_internal_notes (entity_type,entity_id,created_at DESC,id DESC);
CREATE INDEX business_status_entity_changed_idx
    ON business_status_history (entity_type,entity_id,changed_at DESC,id DESC);
CREATE INDEX audit_action_time_idx ON audit_log (action,occurred_at DESC,id DESC);
CREATE INDEX audit_resource_time_idx
    ON audit_log (entity_type,entity_id,occurred_at DESC,id DESC);
CREATE INDEX audit_full_text_idx ON audit_log USING gin (
    to_tsvector('simple',coalesce(actor,'') || ' ' || coalesce(action,'') || ' '
        || coalesce(entity_type,'') || ' ' || coalesce(reason,''))
);
