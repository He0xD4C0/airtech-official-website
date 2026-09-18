-- General public source assets downloaded from Feishu. Only raster images
-- receive preview derivatives; documents and CAD files retain null previews.

ALTER TABLE asset_references DROP CONSTRAINT asset_references_usage_check;
ALTER TABLE asset_references ADD CONSTRAINT asset_references_usage_check CHECK (
    usage IN (
        'hero','cover','gallery','inline','download','datasheet','cad',
        'certificate','logo','favicon','social','other','curve','drawing',
        'technicalDocument'
    )
);

ALTER TABLE asset_references
    ADD COLUMN source_field_id text,
    ADD COLUMN source_field_name text,
    ADD COLUMN source_record_id text;

ALTER TABLE asset_references ADD CONSTRAINT asset_reference_source_shape_check CHECK (
    (source_field_id IS NULL AND source_field_name IS NULL AND source_record_id IS NULL)
    OR (
        length(trim(source_field_id)) > 0
        AND length(trim(source_field_name)) > 0
        AND length(trim(source_record_id)) > 0
    )
);

CREATE INDEX asset_references_product_revision_idx
    ON asset_references (product_id,product_revision,sort_order,id)
    WHERE product_id IS NOT NULL;

COMMENT ON COLUMN asset_references.source_field_id IS
    'Stable Feishu field id for source-owned Product Master attachments.';
