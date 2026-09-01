-- One revision protects the complete allow-listed business-settings document.
-- Keeping it separate from individual app_settings rows makes multi-field
-- updates an atomic compare-and-swap rather than a sequence of unrelated edits.
CREATE TABLE platform_settings_state (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    revision bigint NOT NULL CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    updated_by text NOT NULL
);

INSERT INTO platform_settings_state (singleton, revision, updated_by)
VALUES (true, 1, 'migration')
ON CONFLICT (singleton) DO NOTHING;
