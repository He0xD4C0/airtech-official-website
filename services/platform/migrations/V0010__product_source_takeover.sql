-- Flyway versioned migration. Never edit after release; add a new migration.
-- Product ownership is mutable at the working aggregate while immutable
-- revisions retain the authority that created each revision. The composite
-- revision -> current-product-origin FK introduced in 0005 incorrectly made a
-- verifiedCsv -> Feishu takeover impossible and would erase provenance if the
-- old revision were relabelled. The ordinary product_id FK from 0001 remains.
ALTER TABLE product_revisions
    DROP CONSTRAINT IF EXISTS product_revisions_product_origin_fk;

COMMENT ON COLUMN product_revisions.data_origin IS
    'Immutable authority for this revision; it intentionally may differ from the current products.data_origin after an approved stable_id takeover.';
