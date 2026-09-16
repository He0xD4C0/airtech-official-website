-- Remove server-side CMS body history after V0017 copied only current state.

DROP VIEW IF EXISTS published_news;
DROP VIEW IF EXISTS published_content;

CREATE TABLE cms_current_publication_dependencies (
    id uuid PRIMARY KEY,
    source_content_id uuid NOT NULL REFERENCES cms_published_content(content_id)
        ON DELETE CASCADE,
    reference_path text NOT NULL CHECK (reference_path ~ '^/'),
    dependency_kind text NOT NULL CHECK (
        dependency_kind IN (
            'contentLink','relationContent','relationProduct',
            'mediaInline','mediaDownload'
        )
    ),
    target_content_id uuid REFERENCES cms_published_content(content_id) ON DELETE RESTRICT,
    target_product_id uuid,
    target_product_revision bigint,
    target_media_asset_id uuid REFERENCES media_assets(id) ON DELETE RESTRICT,
    FOREIGN KEY (target_product_id,target_product_revision)
        REFERENCES product_revisions(product_id,revision) MATCH FULL ON DELETE RESTRICT,
    UNIQUE (source_content_id,reference_path,dependency_kind),
    CHECK (
        (dependency_kind IN ('contentLink','relationContent')
         AND target_content_id IS NOT NULL
         AND num_nonnulls(target_product_id,target_product_revision,target_media_asset_id)=0)
        OR
        (dependency_kind='relationProduct'
         AND num_nonnulls(target_product_id,target_product_revision)=2
         AND num_nonnulls(target_content_id,target_media_asset_id)=0)
        OR
        (dependency_kind IN ('mediaInline','mediaDownload')
         AND target_media_asset_id IS NOT NULL
         AND num_nonnulls(target_content_id,target_product_id,target_product_revision)=0)
    )
);

INSERT INTO cms_current_publication_dependencies(
    id,source_content_id,reference_path,dependency_kind,target_content_id,
    target_product_id,target_product_revision,target_media_asset_id
)
SELECT dependency.id,dependency.source_content_id,dependency.reference_path,
       dependency.dependency_kind,dependency.target_content_id,
       dependency.target_product_id,dependency.target_product_revision,
       dependency.target_media_asset_id
FROM cms_publication_dependencies dependency
JOIN cms_published_content published
  ON published.content_id=dependency.source_content_id
 AND published.publication_version=dependency.source_revision;

DROP TRIGGER IF EXISTS cms_publication_dependency_row_count_guard
    ON cms_publication_dependencies;
DROP TRIGGER IF EXISTS cms_publication_dependency_set_count_guard
    ON cms_publication_dependency_sets;
DROP TRIGGER IF EXISTS cms_publication_dependency_row_immutable_guard
    ON cms_publication_dependencies;
DROP TRIGGER IF EXISTS cms_publication_dependency_set_immutable_guard
    ON cms_publication_dependency_sets;
DROP FUNCTION IF EXISTS enforce_cms_publication_dependency_snapshot();
DROP FUNCTION IF EXISTS protect_complete_cms_publication_dependency_snapshot();
DROP FUNCTION IF EXISTS cms_publication_dependency_snapshot_complete(uuid,bigint);
DROP TABLE IF EXISTS cms_publication_dependencies;
DROP TABLE IF EXISTS cms_publication_dependency_sets;

DROP TABLE IF EXISTS news CASCADE;
DROP TABLE IF EXISTS news_working CASCADE;

DO $$
DECLARE constraint_name text;
BEGIN
    FOR constraint_name IN
        SELECT candidate.conname
        FROM pg_constraint candidate
        JOIN pg_class relation ON relation.oid=candidate.conrelid
        JOIN pg_namespace namespace ON namespace.oid=relation.relnamespace
        WHERE relation.relname='asset_references'
          AND namespace.nspname=current_schema()
          AND pg_get_constraintdef(candidate.oid) LIKE '%content_%'
    LOOP
        EXECUTE format('ALTER TABLE asset_references DROP CONSTRAINT %I',constraint_name);
    END LOOP;
END
$$;

DELETE FROM asset_references WHERE content_id IS NOT NULL;
DROP INDEX IF EXISTS asset_references_content_unique;
ALTER TABLE asset_references
    DROP COLUMN content_revision,
    DROP COLUMN content_id,
    ADD CONSTRAINT asset_references_owner_check CHECK (
        num_nonnulls(product_id,general_information_id)=1
    );

DROP TRIGGER IF EXISTS content_revisions_immutable_guard ON content_revisions;
DROP FUNCTION IF EXISTS protect_canonical_content_revision();
ALTER TABLE content_entries
    DROP CONSTRAINT IF EXISTS content_entries_cms_published_revision_fk,
    DROP CONSTRAINT IF EXISTS content_entries_cms_published_revision_check;
DROP TABLE content_preview_snapshots;
DROP TABLE content_drafts;
DROP TABLE content_revisions;

ALTER TABLE content_entries
    DROP COLUMN latest_revision,
    DROP COLUMN cms_published_revision,
    DROP COLUMN cms_updated_by,
    DROP COLUMN current_revision,
    DROP COLUMN published_revision,
    DROP COLUMN scheduled_for,
    DROP COLUMN payload,
    DROP COLUMN status,
    DROP COLUMN title,
    DROP COLUMN updated_at;
ALTER TABLE content_entries RENAME COLUMN cms_created_at TO created_at;

ALTER TABLE audit_log
    ADD COLUMN current_version bigint CHECK (current_version IS NULL OR current_version >= 0);

UPDATE audit_log
SET before_value=NULL,after_value=NULL,reason=NULL
WHERE entity_type='content' OR action LIKE 'content.%';

DELETE FROM idempotency_keys
WHERE scope LIKE 'admin.content.%'
   OR response_body ? 'draft'
   OR response_body ? 'document';

UPDATE outbox_events
SET payload=payload - 'document' - 'draft' - 'before' - 'after'
WHERE aggregate_type='content';

DROP TABLE IF EXISTS cms_data_migrations;

CREATE VIEW published_content AS
SELECT entry.id,entry.kind,
       published.document->>'slug' AS slug,
       entry.locale,published.document->>'title' AS title,
       published.publication_version AS published_revision,
       published.document AS payload,published.updated_at
FROM content_entries entry
JOIN cms_published_content published ON published.content_id=entry.id;

CREATE VIEW published_news AS
SELECT entry.id,published.document->>'slug' AS slug,entry.locale,
       published.document->>'title' AS title,entry.is_placeholder,entry.data_origin,
       published.publication_version AS published_revision,
       published.document AS payload,
       published.document->'typeFields'->>'category' AS category,
       published.document->'typeFields'->>'authorDisplayName' AS author_display_name,
       NULL::uuid AS cover_media_asset_id,
       COALESCE((published.document->'typeFields'->>'featured')::boolean,false) AS featured,
       (published.document->'typeFields'->>'publicationAt')::timestamptz AS publication_at,
       (published.document->'typeFields'->>'readingMinutes')::integer AS reading_minutes,
       published.updated_at AS revision_created_at
FROM content_entries entry
JOIN cms_published_content published ON published.content_id=entry.id
WHERE entry.kind='news';
