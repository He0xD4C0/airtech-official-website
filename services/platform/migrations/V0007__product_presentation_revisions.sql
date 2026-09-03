-- Flyway versioned migration. Never edit after release; add a new migration.
-- Product Master facts and portal-owned website presentation have independent
-- revision clocks. `product_localizations` is retained as the public
-- projection for an exact published Product Master revision; editors write
-- only to the working/revision tables introduced here.

-- A Product may move from the initial verified CSV authority to Feishu. Each
-- immutable fact revision already stores its own origin and has a direct
-- product_id FK; binding every historical origin to the Product's *current*
-- origin would make that supported transition impossible.
ALTER TABLE product_revisions
    DROP CONSTRAINT IF EXISTS product_revisions_product_origin_fk;

CREATE TABLE product_presentation_working (
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    locale text NOT NULL CHECK (length(trim(locale)) BETWEEN 2 AND 35),
    current_revision bigint NOT NULL CHECK (current_revision > 0),
    published_revision bigint,
    slug text NOT NULL CHECK (length(trim(slug)) BETWEEN 1 AND 200),
    title text NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 300),
    summary text,
    content jsonb NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(content) = 'object'),
    seo_metadata jsonb NOT NULL DEFAULT
        '{"title":null,"description":null,"canonicalPath":null,"indexable":false}'::jsonb
        CHECK (
            jsonb_typeof(seo_metadata) = 'object'
            AND seo_metadata ? 'title'
            AND jsonb_typeof(seo_metadata->'title') IN ('string', 'null')
            AND seo_metadata ? 'description'
            AND jsonb_typeof(seo_metadata->'description') IN ('string', 'null')
            AND seo_metadata ? 'canonicalPath'
            AND jsonb_typeof(seo_metadata->'canonicalPath') IN ('string', 'null')
            AND seo_metadata ? 'indexable'
            AND jsonb_typeof(seo_metadata->'indexable') = 'boolean'
        ),
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
    PRIMARY KEY (product_id, locale),
    CHECK (published_revision IS NULL OR published_revision <= current_revision),
    CHECK (
        data_origin <> 'developmentFixture'
        OR (is_placeholder AND NOT indexable)
    )
);

CREATE INDEX product_presentation_working_route_idx
    ON product_presentation_working (locale, slug, product_id);
CREATE INDEX product_presentation_working_search_idx
    ON product_presentation_working USING gin (
        to_tsvector('english', coalesce(title, '') || ' ' || coalesce(summary, ''))
    );

CREATE TABLE product_presentation_revisions (
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    locale text NOT NULL CHECK (length(trim(locale)) BETWEEN 2 AND 35),
    revision bigint NOT NULL CHECK (revision > 0),
    source_product_revision bigint NOT NULL,
    slug text NOT NULL CHECK (length(trim(slug)) BETWEEN 1 AND 200),
    title text NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 300),
    summary text,
    content jsonb NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(content) = 'object'),
    seo_metadata jsonb NOT NULL DEFAULT
        '{"title":null,"description":null,"canonicalPath":null,"indexable":false}'::jsonb
        CHECK (
            jsonb_typeof(seo_metadata) = 'object'
            AND seo_metadata ? 'title'
            AND jsonb_typeof(seo_metadata->'title') IN ('string', 'null')
            AND seo_metadata ? 'description'
            AND jsonb_typeof(seo_metadata->'description') IN ('string', 'null')
            AND seo_metadata ? 'canonicalPath'
            AND jsonb_typeof(seo_metadata->'canonicalPath') IN ('string', 'null')
            AND seo_metadata ? 'indexable'
            AND jsonb_typeof(seo_metadata->'indexable') = 'boolean'
        ),
    translation_state text NOT NULL DEFAULT 'draft'
        CHECK (translation_state IN ('missing', 'draft', 'verified')),
    is_placeholder boolean NOT NULL DEFAULT false,
    indexable boolean NOT NULL DEFAULT false,
    data_origin text NOT NULL DEFAULT 'editorial'
        CHECK (
            data_origin IN ('editorial', 'feishu', 'verifiedCsv', 'developmentFixture')
        ),
    created_by text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (product_id, locale, revision),
    FOREIGN KEY (product_id, source_product_revision)
        REFERENCES product_revisions(product_id, revision)
        ON DELETE CASCADE,
    CHECK (
        data_origin <> 'developmentFixture'
        OR (is_placeholder AND NOT indexable)
    )
);

-- Convert every legacy fact-bound localization into an independent immutable
-- presentation history. The ordinal is deterministic per product and locale,
-- so upgrades produce the same ETags on every database.
WITH ranked AS (
    SELECT localization.*,
           localization.seo_metadata || jsonb_build_object(
               'title', CASE
                   WHEN jsonb_typeof(localization.seo_metadata->'title') IN ('string', 'null')
                   THEN localization.seo_metadata->'title'
                   ELSE 'null'::jsonb
               END,
               'description', CASE
                   WHEN jsonb_typeof(localization.seo_metadata->'description') IN ('string', 'null')
                   THEN localization.seo_metadata->'description'
                   ELSE 'null'::jsonb
               END,
               'canonicalPath', CASE
                   WHEN jsonb_typeof(localization.seo_metadata->'canonicalPath') IN ('string', 'null')
                   THEN localization.seo_metadata->'canonicalPath'
                   ELSE 'null'::jsonb
               END,
               'indexable', CASE
                   WHEN jsonb_typeof(localization.seo_metadata->'indexable') = 'boolean'
                   THEN (localization.seo_metadata->>'indexable')::boolean
                   ELSE localization.indexable
               END
           ) AS normalized_seo_metadata,
           row_number() OVER (
               PARTITION BY localization.product_id, localization.locale
               ORDER BY localization.product_revision, localization.updated_at
           ) AS presentation_revision
    FROM product_localizations localization
)
INSERT INTO product_presentation_revisions (
    product_id,locale,revision,source_product_revision,slug,title,summary,
    content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
    created_by,created_at
)
SELECT product_id,locale,presentation_revision,product_revision,slug,title,summary,
       content,normalized_seo_metadata,translation_state,is_placeholder,indexable,data_origin,
       updated_by,updated_at
FROM ranked;

WITH ranked AS (
    SELECT localization.*,
           product.published_revision AS published_product_revision,
           localization.seo_metadata || jsonb_build_object(
               'title', CASE
                   WHEN jsonb_typeof(localization.seo_metadata->'title') IN ('string', 'null')
                   THEN localization.seo_metadata->'title'
                   ELSE 'null'::jsonb
               END,
               'description', CASE
                   WHEN jsonb_typeof(localization.seo_metadata->'description') IN ('string', 'null')
                   THEN localization.seo_metadata->'description'
                   ELSE 'null'::jsonb
               END,
               'canonicalPath', CASE
                   WHEN jsonb_typeof(localization.seo_metadata->'canonicalPath') IN ('string', 'null')
                   THEN localization.seo_metadata->'canonicalPath'
                   ELSE 'null'::jsonb
               END,
               'indexable', CASE
                   WHEN jsonb_typeof(localization.seo_metadata->'indexable') = 'boolean'
                   THEN (localization.seo_metadata->>'indexable')::boolean
                   ELSE localization.indexable
               END
           ) AS normalized_seo_metadata,
           row_number() OVER (
               PARTITION BY localization.product_id, localization.locale
               ORDER BY localization.product_revision, localization.updated_at
           ) AS presentation_revision
    FROM product_localizations localization
    JOIN products product ON product.id=localization.product_id
), latest AS (
    SELECT DISTINCT ON (product_id, locale)
           product_id,locale,presentation_revision,slug,title,summary,content,
           normalized_seo_metadata,translation_state,is_placeholder,indexable,data_origin,
           updated_by,updated_at
    FROM ranked
    ORDER BY product_id,locale,presentation_revision DESC
), published AS (
    SELECT product_id,locale,max(presentation_revision) AS published_revision
    FROM ranked
    WHERE product_revision=published_product_revision
      AND translation_state='verified'
    GROUP BY product_id,locale
)
INSERT INTO product_presentation_working (
    product_id,locale,current_revision,published_revision,slug,title,summary,
    content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
    updated_by,updated_at
)
SELECT latest.product_id,latest.locale,latest.presentation_revision,
       published.published_revision,latest.slug,latest.title,latest.summary,
       latest.content,latest.normalized_seo_metadata,latest.translation_state,
       latest.is_placeholder,latest.indexable,latest.data_origin,
       latest.updated_by,latest.updated_at
FROM latest
LEFT JOIN published USING (product_id, locale);

-- Keep the retained SSR/public projection decodable as well. Legacy schema
-- accepted any JSON object, so preserve valid strings and unknown future keys
-- while replacing only values that cannot deserialize into `SeoMetadata`.
UPDATE product_localizations localization
SET seo_metadata = localization.seo_metadata || jsonb_build_object(
    'title', CASE
        WHEN jsonb_typeof(localization.seo_metadata->'title') IN ('string', 'null')
        THEN localization.seo_metadata->'title'
        ELSE 'null'::jsonb
    END,
    'description', CASE
        WHEN jsonb_typeof(localization.seo_metadata->'description') IN ('string', 'null')
        THEN localization.seo_metadata->'description'
        ELSE 'null'::jsonb
    END,
    'canonicalPath', CASE
        WHEN jsonb_typeof(localization.seo_metadata->'canonicalPath') IN ('string', 'null')
        THEN localization.seo_metadata->'canonicalPath'
        ELSE 'null'::jsonb
    END,
    'indexable', CASE
        WHEN jsonb_typeof(localization.seo_metadata->'indexable') = 'boolean'
        THEN (localization.seo_metadata->>'indexable')::boolean
        ELSE localization.indexable
    END
);

ALTER TABLE product_presentation_working
    ADD CONSTRAINT product_presentation_working_current_revision_fk
        FOREIGN KEY (product_id,locale,current_revision)
        REFERENCES product_presentation_revisions(product_id,locale,revision)
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT product_presentation_working_published_revision_fk
        FOREIGN KEY (product_id,locale,published_revision)
        REFERENCES product_presentation_revisions(product_id,locale,revision)
        DEFERRABLE INITIALLY DEFERRED;

-- Draft rows have been preserved above. From this migration onward the legacy
-- table is exclusively an SSR/public projection and is written only by the
-- controlled Product publish transaction.
DELETE FROM product_localizations localization
USING products product
WHERE product.id=localization.product_id
  AND (
      product.published_revision IS NULL
      OR localization.product_revision <> product.published_revision
      OR localization.translation_state <> 'verified'
  );

ALTER TABLE product_localizations
    ALTER COLUMN translation_state SET DEFAULT 'verified',
    ADD CONSTRAINT product_localizations_published_projection_check
        CHECK (translation_state = 'verified'),
    ADD CONSTRAINT product_localizations_seo_metadata_shape_check
        CHECK (
            jsonb_typeof(seo_metadata) = 'object'
            AND seo_metadata ? 'title'
            AND jsonb_typeof(seo_metadata->'title') IN ('string', 'null')
            AND seo_metadata ? 'description'
            AND jsonb_typeof(seo_metadata->'description') IN ('string', 'null')
            AND seo_metadata ? 'canonicalPath'
            AND jsonb_typeof(seo_metadata->'canonicalPath') IN ('string', 'null')
            AND seo_metadata ? 'indexable'
            AND jsonb_typeof(seo_metadata->'indexable') = 'boolean'
        );
