-- Private CMS drafts, review queue, and one current publication per content ID.
-- Editor undo/redo is deliberately absent: it is browser-session state only.

ALTER TABLE users
    ADD COLUMN manager_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    ADD CONSTRAINT users_manager_not_self CHECK (manager_user_id IS NULL OR manager_user_id<>id);

CREATE FUNCTION reject_user_manager_cycle()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('airtek.users.manager-chain',0));
    IF NEW.manager_user_id IS NULL THEN RETURN NEW; END IF;
    IF EXISTS (
        WITH RECURSIVE managers(id) AS (
            SELECT NEW.manager_user_id
            UNION ALL
            SELECT users.manager_user_id
            FROM users JOIN managers ON users.id=managers.id
            WHERE users.manager_user_id IS NOT NULL
        )
        SELECT 1 FROM managers WHERE id=NEW.id
    ) THEN
        RAISE EXCEPTION USING ERRCODE='23514',
            MESSAGE='manager relationship would create a cycle',
            CONSTRAINT='users_manager_acyclic';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER users_manager_acyclic_guard
BEFORE INSERT OR UPDATE OF manager_user_id ON users
FOR EACH ROW EXECUTE FUNCTION reject_user_manager_cycle();

INSERT INTO app_settings(key,value,updated_by)
VALUES ('contentReviewRequired','true'::jsonb,'migration')
ON CONFLICT (key) DO NOTHING;

CREATE TABLE cms_published_content (
    content_id uuid PRIMARY KEY REFERENCES content_entries(id) ON DELETE CASCADE,
    document jsonb NOT NULL CHECK (
        jsonb_typeof(document)='object'
        AND document->>'schemaVersion'='2'
    ),
    publication_version bigint NOT NULL CHECK (publication_version > 0),
    published_by uuid REFERENCES users(id) ON DELETE SET NULL,
    published_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM content_entries entry
        LEFT JOIN content_revisions revision
          ON revision.content_id=entry.id
         AND revision.revision=entry.cms_published_revision
        WHERE entry.cms_published_revision IS NOT NULL
          AND (
            revision.document IS NULL
            OR jsonb_typeof(revision.document)<>'object'
            OR revision.document->>'schemaVersion'<>'2'
            OR revision.document->>'kind' IS DISTINCT FROM entry.kind
            OR revision.document->>'locale' IS DISTINCT FROM entry.locale
            OR revision.document->>'templateKey' IS DISTINCT FROM entry.template_key
          )
    ) THEN
        RAISE EXCEPTION USING ERRCODE='23514',
            MESSAGE='published CMS content cannot be migrated without a valid current document';
    END IF;
END
$$;

INSERT INTO cms_published_content(
    content_id,document,publication_version,published_by,published_at,updated_at
)
SELECT entry.id,revision.document,entry.cms_published_revision,
       actor.id,revision.created_at,revision.created_at
FROM content_entries entry
JOIN content_revisions revision
  ON revision.content_id=entry.id AND revision.revision=entry.cms_published_revision
LEFT JOIN LATERAL (
    SELECT users.id FROM users
    WHERE lower(users.email)=lower(revision.created_by)
    ORDER BY users.id LIMIT 1
) actor ON true
WHERE entry.cms_published_revision IS NOT NULL;

CREATE TABLE cms_drafts (
    draft_id uuid PRIMARY KEY,
    content_id uuid NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    owner_user_id uuid REFERENCES users(id) ON DELETE RESTRICT,
    document jsonb NOT NULL CHECK (
        jsonb_typeof(document)='object'
        AND document->>'schemaVersion'='2'
    ),
    draft_version bigint NOT NULL CHECK (draft_version > 0),
    base_publication_version bigint NOT NULL DEFAULT 0
        CHECK (base_publication_version >= 0),
    state text NOT NULL DEFAULT 'editing' CHECK (state IN ('editing','pendingReview')),
    rejection_reason text CHECK (
        rejection_reason IS NULL OR length(trim(rejection_reason)) BETWEEN 1 AND 2000
    ),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((document->>'draftVersion')::bigint=draft_version)
);

CREATE UNIQUE INDEX cms_drafts_owner_content_unique
    ON cms_drafts(owner_user_id,content_id) WHERE owner_user_id IS NOT NULL;

INSERT INTO cms_drafts(
    draft_id,content_id,owner_user_id,document,draft_version,
    base_publication_version,state,created_at,updated_at
)
SELECT draft.content_id,draft.content_id,owner.id,draft.document,draft.draft_version,
       COALESCE(entry.cms_published_revision,0),'editing',draft.updated_at,draft.updated_at
FROM content_drafts draft
JOIN content_entries entry ON entry.id=draft.content_id
LEFT JOIN LATERAL (
    SELECT users.id FROM users
    WHERE lower(users.email)=lower(draft.updated_by)
    ORDER BY users.id LIMIT 1
) owner ON true;

CREATE TABLE cms_draft_shares (
    draft_id uuid NOT NULL REFERENCES cms_drafts(draft_id) ON DELETE CASCADE,
    shared_with_user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    shared_by_user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (draft_id,shared_with_user_id)
);

CREATE TABLE cms_review_queue (
    draft_id uuid PRIMARY KEY REFERENCES cms_drafts(draft_id) ON DELETE CASCADE,
    submitted_by_user_id uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    submitted_at timestamptz NOT NULL DEFAULT now()
);

CREATE FUNCTION enforce_cms_review_queue_state()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM cms_drafts
        WHERE draft_id=NEW.draft_id AND state='pendingReview'
    ) THEN
        RAISE EXCEPTION USING ERRCODE='23514',
            MESSAGE='review queue requires a pendingReview draft';
    END IF;
    RETURN NEW;
END
$$;

CREATE CONSTRAINT TRIGGER cms_review_queue_state_guard
AFTER INSERT OR UPDATE ON cms_review_queue
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION enforce_cms_review_queue_state();
