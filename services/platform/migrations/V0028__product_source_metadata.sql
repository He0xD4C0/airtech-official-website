-- Administrative supplier and brand source metadata. These rows preserve
-- immutable archive provenance and are never projected to public product APIs.

CREATE TABLE product_source_metadata (
    id uuid PRIMARY KEY,
    kind text NOT NULL CHECK (kind IN ('supplier','brand')),
    label text NOT NULL CHECK (length(btrim(label)) BETWEEN 1 AND 300),
    source_table text NOT NULL CHECK (length(btrim(source_table)) BETWEEN 1 AND 100),
    source_record_id text NOT NULL CHECK (length(btrim(source_record_id)) BETWEEN 1 AND 200),
    archive_sha256 char(64) NOT NULL CHECK (archive_sha256 ~ '^[0-9a-f]{64}$'),
    record_checksum char(64) NOT NULL CHECK (record_checksum ~ '^[0-9a-f]{64}$'),
    attributes jsonb NOT NULL CHECK (jsonb_typeof(attributes)='object'),
    raw_fields jsonb NOT NULL CHECK (jsonb_typeof(raw_fields)='object'),
    captured_at timestamptz NOT NULL,
    imported_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (kind,archive_sha256,source_table,source_record_id)
);

CREATE INDEX product_source_metadata_kind_label_idx
    ON product_source_metadata(kind,label,id);

COMMENT ON TABLE product_source_metadata IS
    'Admin-only supplier and brand source records with raw archive provenance; never expose through public APIs.';
