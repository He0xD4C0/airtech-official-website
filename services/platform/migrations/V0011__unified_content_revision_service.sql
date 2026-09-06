-- Flyway versioned migration. Never edit after release; add a new migration.
-- Establish the canonical CMS V2 draft/revision service without rewriting the
-- legacy public projection. Legacy columns remain only as a temporary public
-- read compatibility surface and are not used by the Admin CMS service.

ALTER TABLE content_entries
    DROP CONSTRAINT content_entries_kind_check,
    ADD CONSTRAINT content_entries_kind_check CHECK (
        kind IN (
            'home', 'page', 'solution', 'technology', 'article', 'news', 'faq',
            'caseStudy', 'download', 'company', 'legal', 'generalInformation',
            'navigation', 'footer'
        )
    ),
    ADD COLUMN template_key text,
    ADD COLUMN latest_revision bigint NOT NULL DEFAULT 0
        CHECK (latest_revision >= 0),
    ADD COLUMN cms_published_revision bigint,
    ADD COLUMN cms_created_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN cms_updated_by text NOT NULL DEFAULT 'cms-v2-schema-migration',
    ADD CONSTRAINT content_entries_cms_published_revision_check
        CHECK (
            cms_published_revision IS NULL
            OR (
                cms_published_revision > 0
                AND cms_published_revision <= latest_revision
            )
        );

UPDATE content_entries entry
SET template_key=CASE
        WHEN entry.kind='home' THEN 'home'
        WHEN entry.kind='solution' THEN 'solutionDetail'
        WHEN entry.kind='technology' THEN 'technologyDetail'
        WHEN entry.kind='article' THEN 'articleDetail'
        WHEN entry.kind='news' THEN 'newsDetail'
        WHEN entry.kind='faq' THEN 'faqDetail'
        WHEN entry.kind='caseStudy' THEN 'caseStudyDetail'
        WHEN entry.kind='download' THEN 'downloadDetail'
        WHEN entry.kind='company' THEN 'about'
        WHEN entry.kind='legal' THEN 'legal'
        WHEN entry.kind='navigation' THEN 'navigation'
        WHEN entry.kind='footer' THEN 'footer'
        ELSE 'page'
    END,
    latest_revision=COALESCE((
        SELECT max(revision.revision)
        FROM content_revisions revision
        WHERE revision.content_id=entry.id
    ),0),
    cms_published_revision=entry.published_revision,
    cms_created_at=entry.updated_at;

ALTER TABLE content_entries
    ALTER COLUMN template_key SET NOT NULL,
    ADD CONSTRAINT content_entries_template_key_check CHECK (
        template_key IN (
            'home', 'productIndex', 'productFamily', 'selector', 'compare',
            'solutionIndex', 'solutionDetail', 'technologyIndex',
            'technologyDetail', 'articleIndex', 'articleDetail', 'newsIndex',
            'newsDetail', 'faqIndex', 'faqDetail', 'caseStudyIndex',
            'caseStudyDetail', 'downloadIndex', 'downloadDetail', 'about',
            'contact', 'rfqRouter', 'rfqForm', 'search', 'legal', 'navigation',
            'footer', 'generalInformation'
        )
    );

ALTER TABLE content_revisions
    ALTER COLUMN payload DROP NOT NULL,
    ADD COLUMN document jsonb,
    ADD COLUMN source_draft_version bigint,
    ADD COLUMN revision_kind text,
    ADD COLUMN reason text,
    ADD CONSTRAINT content_revisions_v2_shape_check CHECK (
        document IS NULL
        OR (
            jsonb_typeof(document)='object'
            AND (document->>'schemaVersion')::integer=2
            AND source_draft_version > 0
            AND revision_kind IN ('manual','publish','restore')
            AND length(trim(reason)) BETWEEN 10 AND 2000
        )
    );

ALTER TABLE content_entries
    ADD CONSTRAINT content_entries_cms_published_revision_fk
        FOREIGN KEY (id, cms_published_revision)
        REFERENCES content_revisions(content_id, revision)
        DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE content_drafts (
    content_id uuid PRIMARY KEY REFERENCES content_entries(id) ON DELETE CASCADE,
    draft_version bigint NOT NULL CHECK (draft_version > 0),
    document jsonb NOT NULL CHECK (
        jsonb_typeof(document)='object'
        AND (document->>'schemaVersion')::integer=2
        AND (document->>'draftVersion')::bigint=draft_version
    ),
    updated_by text NOT NULL CHECK (length(trim(updated_by)) > 0),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX content_drafts_updated_idx
    ON content_drafts (updated_at DESC, content_id);

CREATE TABLE content_preview_snapshots (
    content_id uuid NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    draft_version bigint NOT NULL CHECK (draft_version > 0),
    document jsonb NOT NULL CHECK (
        jsonb_typeof(document)='object'
        AND (document->>'schemaVersion')::integer=2
        AND (document->>'draftVersion')::bigint=draft_version
    ),
    created_by text NOT NULL CHECK (length(trim(created_by)) > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    PRIMARY KEY (content_id, draft_version),
    CHECK (
        expires_at > created_at
        AND expires_at <= created_at + interval '15 minutes'
    )
);

CREATE INDEX content_preview_snapshots_expiry_idx
    ON content_preview_snapshots (expires_at);

CREATE TABLE cms_data_migrations (
    key text PRIMARY KEY,
    report jsonb NOT NULL CHECK (jsonb_typeof(report)='object'),
    completed_at timestamptz NOT NULL DEFAULT now()
);

CREATE FUNCTION protect_canonical_content_revision()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP='DELETE' AND OLD.document IS NOT NULL THEN
        RAISE EXCEPTION USING
            ERRCODE='55000',
            MESSAGE='canonical content revisions are immutable';
    END IF;
    IF TG_OP='UPDATE' AND OLD.document IS NOT NULL AND ROW(
        OLD.document,
        OLD.source_draft_version,
        OLD.revision_kind,
        OLD.reason,
        OLD.created_by,
        OLD.created_at
    ) IS DISTINCT FROM ROW(
        NEW.document,
        NEW.source_draft_version,
        NEW.revision_kind,
        NEW.reason,
        NEW.created_by,
        NEW.created_at
    ) THEN
        RAISE EXCEPTION USING
            ERRCODE='55000',
            MESSAGE='canonical content revisions are immutable';
    END IF;
    IF TG_OP='DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER content_revisions_immutable_guard
BEFORE UPDATE OR DELETE ON content_revisions
FOR EACH ROW EXECUTE FUNCTION protect_canonical_content_revision();
