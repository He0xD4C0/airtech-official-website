-- Flyway versioned migration. Never edit after release; add a new migration.
-- Separate mutable News autosaves from immutable publication/preview history.
-- This migration is intentionally incremental because 0005 may already be
-- present in development databases.
CREATE TABLE news_working (
    content_id uuid PRIMARY KEY,
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
    data_origin text NOT NULL DEFAULT 'editorial'
        CHECK (data_origin IN ('editorial','developmentFixture')),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (content_id, content_kind)
        REFERENCES content_entries(id, kind)
        ON DELETE CASCADE
);

ALTER TABLE news
    ADD COLUMN data_origin text NOT NULL DEFAULT 'editorial',
    ADD CONSTRAINT news_data_origin_check
        CHECK (data_origin IN ('editorial','developmentFixture'));

UPDATE news metadata
SET data_origin=entry.data_origin
FROM content_entries entry
WHERE entry.id=metadata.content_id;

-- Backfill the current working metadata from the exact current revision where
-- it exists. Published historical rows remain immutable in `news`.
INSERT INTO news_working (
    content_id,content_kind,category,author_display_name,cover_media_asset_id,
    featured,publication_at,reading_minutes,data_origin,updated_at
)
SELECT entry.id,'news',metadata.category,metadata.author_display_name,
       metadata.cover_media_asset_id,metadata.featured,metadata.publication_at,
       metadata.reading_minutes,metadata.data_origin,entry.updated_at
FROM content_entries entry
JOIN news metadata
  ON metadata.content_id=entry.id
 AND metadata.revision=entry.current_revision
WHERE entry.kind='news'
ON CONFLICT (content_id) DO NOTHING;

CREATE OR REPLACE VIEW published_news AS
SELECT entry.id,
       revision.payload->>'slug' AS slug,
       revision.payload->>'locale' AS locale,
       revision.payload->>'title' AS title,
       COALESCE((revision.payload->>'isPlaceholder')::boolean,false) AS is_placeholder,
       metadata.data_origin,
       entry.published_revision,
       revision.payload,
       metadata.category,
       metadata.author_display_name,
       metadata.cover_media_asset_id,
       metadata.featured,
       metadata.publication_at,
       metadata.reading_minutes,
       revision.created_at AS revision_created_at
FROM content_entries entry
JOIN content_revisions revision
  ON revision.content_id=entry.id
 AND revision.revision=entry.published_revision
JOIN news metadata
  ON metadata.content_id=entry.id
 AND metadata.revision=entry.published_revision
WHERE entry.kind='news' AND entry.published_revision IS NOT NULL;

ALTER TABLE general_information_revisions
    ADD COLUMN locale text,
    ADD COLUMN is_placeholder boolean,
    ADD COLUMN data_origin text;

UPDATE general_information_revisions revision
SET locale=information.locale,
    is_placeholder=information.is_placeholder,
    data_origin=information.data_origin
FROM general_information information
WHERE information.id=revision.general_information_id;

ALTER TABLE general_information_revisions
    ALTER COLUMN locale SET NOT NULL,
    ALTER COLUMN is_placeholder SET NOT NULL,
    ALTER COLUMN data_origin SET NOT NULL,
    ADD CONSTRAINT general_information_revision_locale_check
        CHECK (length(trim(locale)) BETWEEN 2 AND 35),
    ADD CONSTRAINT general_information_revision_origin_check
        CHECK (data_origin IN ('editorial','developmentFixture')),
    ADD CONSTRAINT general_information_revision_fixture_check
        CHECK (data_origin <> 'developmentFixture' OR is_placeholder);

CREATE OR REPLACE VIEW published_general_information AS
SELECT information.id,
       information.scope,
       revision.locale,
       revision.is_placeholder,
       revision.data_origin,
       information.published_revision,
       revision.payload,
       revision.created_at AS revision_created_at
FROM general_information information
JOIN general_information_revisions revision
  ON revision.general_information_id=information.id
 AND revision.revision=information.published_revision
WHERE information.published_revision IS NOT NULL;
