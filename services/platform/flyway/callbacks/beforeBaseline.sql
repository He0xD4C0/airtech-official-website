-- A baseline is only valid for the exact SQLx-managed version 1-10 history
-- shipped before Flyway adoption. New/empty databases must use `migrate`.
DO $flyway_adoption$
DECLARE
    core_table_count bigint;
    history_row_count bigint;
    history_version_count bigint;
    mismatch_count bigint;
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_class AS relation
        JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
        WHERE namespace.nspname = 'public'
          AND relation.relname = '_sqlx_migrations'
          AND relation.relkind IN ('r', 'p')
    ) THEN
        RAISE EXCEPTION
            'Flyway baseline refused: public._sqlx_migrations is not a table; use migrate for an empty database';
    END IF;

    SELECT count(*)
    INTO core_table_count
    FROM pg_class AS relation
    JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
    WHERE namespace.nspname = 'public'
      AND relation.relname IN ('content_entries', 'jobs')
      AND relation.relkind IN ('r', 'p');

    IF core_table_count <> 2 THEN
        RAISE EXCEPTION
            'Flyway baseline refused: required legacy tables public.content_entries and public.jobs are missing';
    END IF;

    SELECT count(*), count(DISTINCT version)
    INTO history_row_count, history_version_count
    FROM public._sqlx_migrations;

    IF history_row_count <> 10 OR history_version_count <> 10 THEN
        RAISE EXCEPTION
            'Flyway baseline refused: SQLx history must contain exactly one row for each version 1 through 10';
    END IF;

    WITH expected(version, checksum_hex) AS (
        VALUES
            (1::bigint, 'ccd98441f2636df4e875f67bff11a6d5f984a058380810e1be1884c00057717076f04da1f959599dd67f5757a2930564'),
            (2::bigint, '84a0652742c1e94b5c50ab5295d5e7f93bac2686f43ad78d72c6c48ff4b2c2eda40efd602a5963a4d6c1ce10789574f7'),
            (3::bigint, '1a0867c93e79f58192065c4fdee997988626fe8879f953731e53fa0ee9c498605770d7c09807b153de89c6c31d14f56a'),
            (4::bigint, 'b87cab34cfe88e98663e8d1e01dd7fee2424c8efb2b45e392258b0d3fa80cb2f1a55ff9704607ad87bd91c68da8c7ee2'),
            (5::bigint, '8c4b886f20032b2d1c0f4b5df1c9e373b408e71ed7a5f595c6353e24424c2f4914a3b2373852e52b4e01c8cce8d0a250'),
            (6::bigint, '4a57cee86761d1e06ccdbf824d7347a32dc47c5c8433a636ba24f0e1dd8d38f9c6ccf0985e2c038a8074de47455655cc'),
            (7::bigint, '862d506fbfcccd71e1472ec7f367638ee262772059a3939fc04d786e3e6c4d76961230276d17c74e8446b88a22f1d3cc'),
            (8::bigint, 'f5e7452eadf8b80a01b8abe6715f0470a66045ff1d61038001dce7f9013d1a6dbfe812511fb808f4d4c220670b09c837'),
            (9::bigint, 'a8f96920cdab8147aaf24ac3db504fb2d49eb4f5dc7a6173f984776fabd8a70f1c72dcd8dc60e8d5c373b64b040917a4'),
            (10::bigint, 'd674ac912444d60d354a96c81114dbe2d877f26eb8d8d5622c902b49a460c333d8b8b113886b96fcd9d48f43e4712ed9')
    )
    SELECT count(*)
    INTO mismatch_count
    FROM expected
    FULL OUTER JOIN public._sqlx_migrations AS actual USING (version)
    WHERE expected.version IS NULL
       OR actual.version IS NULL
       OR actual.success IS DISTINCT FROM TRUE
       OR actual.checksum IS DISTINCT FROM decode(expected.checksum_hex, 'hex');

    IF mismatch_count <> 0 THEN
        RAISE EXCEPTION
            'Flyway baseline refused: SQLx history is incomplete, changed, failed, or newer than version 10';
    END IF;
END
$flyway_adoption$;
