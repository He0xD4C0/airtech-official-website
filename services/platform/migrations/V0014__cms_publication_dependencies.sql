-- Immutable dependency snapshots for CMS publication. Media dependencies pin
-- only a stable asset ID; publication requires that the asset exists and has
-- not been soft-deleted, with no processing or access-state gate.

CREATE TABLE cms_publication_dependency_sets (
    content_id uuid NOT NULL,
    content_revision bigint NOT NULL,
    extractor_version text NOT NULL CHECK (length(trim(extractor_version)) > 0),
    extraction_status text NOT NULL CHECK (extraction_status IN ('complete','blocked')),
    dependency_count integer NOT NULL CHECK (dependency_count >= 0),
    blocking_issues jsonb NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_typeof(blocking_issues)='array'),
    created_by text NOT NULL CHECK (length(trim(created_by)) > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (content_id,content_revision),
    FOREIGN KEY (content_id,content_revision)
        REFERENCES content_revisions(content_id,revision) ON DELETE RESTRICT,
    CHECK (
        (extraction_status='complete' AND jsonb_array_length(blocking_issues)=0)
        OR (extraction_status='blocked' AND jsonb_array_length(blocking_issues)>0)
    )
);

CREATE TABLE cms_publication_dependencies (
    id uuid PRIMARY KEY,
    source_content_id uuid NOT NULL,
    source_revision bigint NOT NULL,
    reference_path text NOT NULL CHECK (reference_path ~ '^/'),
    dependency_kind text NOT NULL CHECK (
        dependency_kind IN (
            'contentLink','relationContent','relationProduct',
            'mediaInline','mediaDownload'
        )
    ),
    target_content_id uuid,
    target_content_revision bigint,
    target_product_id uuid,
    target_product_revision bigint,
    target_media_asset_id uuid REFERENCES media_assets(id) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (source_content_id,source_revision)
        REFERENCES cms_publication_dependency_sets(content_id,content_revision)
        ON DELETE CASCADE,
    FOREIGN KEY (target_content_id,target_content_revision)
        REFERENCES content_revisions(content_id,revision) MATCH FULL ON DELETE RESTRICT,
    FOREIGN KEY (target_product_id,target_product_revision)
        REFERENCES product_revisions(product_id,revision) MATCH FULL ON DELETE RESTRICT,
    UNIQUE (source_content_id,source_revision,reference_path,dependency_kind),
    CHECK (
        (dependency_kind IN ('contentLink','relationContent')
         AND num_nonnulls(target_content_id,target_content_revision)=2
         AND num_nonnulls(target_product_id,target_product_revision,target_media_asset_id)=0)
        OR
        (dependency_kind='relationProduct'
         AND num_nonnulls(target_product_id,target_product_revision)=2
         AND num_nonnulls(target_content_id,target_content_revision,target_media_asset_id)=0)
        OR
        (dependency_kind IN ('mediaInline','mediaDownload')
         AND target_media_asset_id IS NOT NULL
         AND num_nonnulls(target_content_id,target_content_revision,
                          target_product_id,target_product_revision)=0)
    )
);

CREATE FUNCTION cms_publication_dependency_snapshot_complete(
    checked_content_id uuid,
    checked_content_revision bigint
) RETURNS boolean LANGUAGE sql STABLE PARALLEL SAFE AS $$
    SELECT COALESCE((
        SELECT dependency_set.extraction_status='complete'
           AND dependency_set.dependency_count::bigint=(
               SELECT count(*) FROM cms_publication_dependencies dependency
               WHERE dependency.source_content_id=dependency_set.content_id
                 AND dependency.source_revision=dependency_set.content_revision
           )
        FROM cms_publication_dependency_sets dependency_set
        WHERE dependency_set.content_id=checked_content_id
          AND dependency_set.content_revision=checked_content_revision
    ),false)
$$;

CREATE FUNCTION protect_complete_cms_publication_dependency_snapshot()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE existing_status text;
BEGIN
    IF TG_TABLE_NAME='cms_publication_dependency_sets' THEN
        existing_status := OLD.extraction_status;
    ELSE
        SELECT extraction_status INTO existing_status
        FROM cms_publication_dependency_sets
        WHERE content_id=OLD.source_content_id AND content_revision=OLD.source_revision;
    END IF;
    IF existing_status='complete' THEN
        RAISE EXCEPTION USING ERRCODE='55000',
            MESSAGE='complete CMS publication dependency snapshots are immutable',
            CONSTRAINT='cms_publication_dependency_snapshot_immutable';
    END IF;
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cms_publication_dependency_set_immutable_guard
BEFORE UPDATE OR DELETE ON cms_publication_dependency_sets
FOR EACH ROW EXECUTE FUNCTION protect_complete_cms_publication_dependency_snapshot();

CREATE TRIGGER cms_publication_dependency_row_immutable_guard
BEFORE UPDATE OR DELETE ON cms_publication_dependencies
FOR EACH ROW EXECUTE FUNCTION protect_complete_cms_publication_dependency_snapshot();

CREATE FUNCTION enforce_cms_publication_dependency_snapshot()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE affected_content_id uuid; affected_revision bigint;
BEGIN
    IF TG_TABLE_NAME='cms_publication_dependency_sets' THEN
        affected_content_id := NEW.content_id;
        affected_revision := NEW.content_revision;
    ELSIF TG_OP='DELETE' THEN
        affected_content_id := OLD.source_content_id;
        affected_revision := OLD.source_revision;
    ELSE
        affected_content_id := NEW.source_content_id;
        affected_revision := NEW.source_revision;
    END IF;
    IF EXISTS (
        SELECT 1 FROM cms_publication_dependency_sets
        WHERE content_id=affected_content_id AND content_revision=affected_revision
          AND extraction_status='complete'
    ) AND NOT cms_publication_dependency_snapshot_complete(
        affected_content_id,affected_revision
    ) THEN
        RAISE EXCEPTION USING ERRCODE='23514',
            MESSAGE='complete CMS publication dependency snapshot count does not match its rows',
            CONSTRAINT='cms_publication_dependency_count_matches';
    END IF;
    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER cms_publication_dependency_set_count_guard
AFTER INSERT OR UPDATE ON cms_publication_dependency_sets
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION enforce_cms_publication_dependency_snapshot();

CREATE CONSTRAINT TRIGGER cms_publication_dependency_row_count_guard
AFTER INSERT OR UPDATE OR DELETE ON cms_publication_dependencies
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION enforce_cms_publication_dependency_snapshot();

CREATE INDEX cms_dependencies_target_content_idx
    ON cms_publication_dependencies (target_content_id,target_content_revision)
    WHERE target_content_id IS NOT NULL;
CREATE INDEX cms_dependencies_target_product_idx
    ON cms_publication_dependencies (target_product_id,target_product_revision)
    WHERE target_product_id IS NOT NULL;
CREATE INDEX cms_dependencies_target_media_idx
    ON cms_publication_dependencies (target_media_asset_id,dependency_kind)
    WHERE target_media_asset_id IS NOT NULL;
