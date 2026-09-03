-- Keep the long-lived application role on DML plus required temporary-table
-- access while Flyway uses a deployment-only DDL identity. The container
-- entrypoint validates the placeholder before Flyway substitutes it.
DO $flyway_runtime_grants$
DECLARE
    runtime_role text := '${runtime_role}';
    allow_shared_role boolean := '${allow_shared_role}'::boolean;
    missing_privileges integer;
BEGIN
    IF runtime_role !~ '^[a-z_][a-z0-9_]{0,62}$' THEN
        RAISE EXCEPTION 'Invalid Flyway runtime role identifier';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = runtime_role) THEN
        RAISE EXCEPTION
            'Flyway runtime role % does not exist; provision it before migrating',
            runtime_role;
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_roles
        WHERE rolname = runtime_role
          AND rolcanlogin
    ) THEN
        RAISE EXCEPTION
            'Flyway runtime role % must have LOGIN',
            runtime_role;
    END IF;
    IF runtime_role = current_user AND NOT allow_shared_role THEN
        RAISE EXCEPTION
            'Flyway DDL role and application runtime role must be distinct';
    END IF;
    IF runtime_role <> current_user THEN
        IF EXISTS (
            SELECT 1
            FROM pg_roles
            WHERE rolname = runtime_role
              AND (
                  rolsuper
                  OR rolcreatedb
                  OR rolcreaterole
                  OR rolreplication
                  OR rolbypassrls
              )
        ) THEN
            RAISE EXCEPTION
                'Flyway runtime role % has a forbidden elevated role attribute',
                runtime_role;
        END IF;
        IF EXISTS (
            SELECT 1
            FROM pg_roles AS target_role
            WHERE target_role.rolname <> runtime_role
              AND (
                  pg_has_role(runtime_role, target_role.oid, 'SET')
                  OR pg_has_role(runtime_role, target_role.oid, 'USAGE')
              )
        ) THEN
            RAISE EXCEPTION
                'Flyway runtime role % must not inherit or SET ROLE to another role',
                runtime_role;
        END IF;
    END IF;

    -- A project database must not let the application create objects through
    -- PostgreSQL's historical PUBLIC schema grant.
    REVOKE CREATE ON SCHEMA public FROM PUBLIC;
    EXECUTE format('REVOKE CREATE ON SCHEMA public FROM %I', runtime_role);
    EXECUTE format(
        'REVOKE CREATE ON DATABASE %I FROM PUBLIC',
        current_database()
    );
    EXECUTE format(
        'REVOKE CREATE ON DATABASE %I FROM %I',
        current_database(),
        runtime_role
    );
    EXECUTE format('GRANT USAGE ON SCHEMA public TO %I', runtime_role);
    EXECUTE format(
        'GRANT CONNECT ON DATABASE %I TO %I',
        current_database(),
        runtime_role
    );
    EXECUTE format(
        'GRANT TEMPORARY ON DATABASE %I TO %I',
        current_database(),
        runtime_role
    );
    EXECUTE format(
        'GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO %I',
        runtime_role
    );
    EXECUTE format(
        'GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO %I',
        runtime_role
    );
    EXECUTE format(
        'GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO %I',
        runtime_role
    );
    EXECUTE format(
        'ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO %I',
        current_user,
        runtime_role
    );
    EXECUTE format(
        'ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA public GRANT USAGE, SELECT, UPDATE ON SEQUENCES TO %I',
        current_user,
        runtime_role
    );
    EXECUTE format(
        'ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA public GRANT EXECUTE ON FUNCTIONS TO %I',
        current_user,
        runtime_role
    );

    EXECUTE format(
        'REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON TABLE public.flyway_schema_history FROM %I',
        runtime_role
    );
    EXECUTE format(
        'GRANT SELECT ON TABLE public.flyway_schema_history TO %I',
        runtime_role
    );
    IF NOT has_schema_privilege(runtime_role, 'public', 'USAGE') THEN
        RAISE EXCEPTION
            'Flyway runtime role % lacks USAGE on schema public',
            runtime_role;
    END IF;
    IF NOT has_database_privilege(runtime_role, current_database(), 'CONNECT') THEN
        RAISE EXCEPTION
            'Flyway runtime role % lacks CONNECT on the application database',
            runtime_role;
    END IF;
    IF NOT has_database_privilege(runtime_role, current_database(), 'TEMPORARY') THEN
        RAISE EXCEPTION
            'Flyway runtime role % lacks TEMPORARY on the application database',
            runtime_role;
    END IF;
    IF NOT has_table_privilege(runtime_role, 'public.flyway_schema_history', 'SELECT') THEN
        RAISE EXCEPTION
            'Flyway runtime role % cannot read Flyway history',
            runtime_role;
    END IF;
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        EXECUTE format(
            'REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON TABLE public._sqlx_migrations FROM %I',
            runtime_role
        );
        EXECUTE format(
            'GRANT SELECT ON TABLE public._sqlx_migrations TO %I',
            runtime_role
        );
        IF NOT has_table_privilege(runtime_role, 'public._sqlx_migrations', 'SELECT') THEN
            RAISE EXCEPTION
                'Flyway runtime role % cannot read SQLx history',
                runtime_role;
        END IF;
    END IF;

    SELECT count(*)
    INTO missing_privileges
    FROM pg_class AS relation
    JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
    WHERE namespace.nspname = 'public'
      AND relation.relkind IN ('r', 'p', 'v', 'm', 'f')
      AND relation.relname NOT IN ('flyway_schema_history', '_sqlx_migrations')
      AND NOT (
          has_table_privilege(runtime_role, relation.oid, 'SELECT')
          AND has_table_privilege(runtime_role, relation.oid, 'INSERT')
          AND has_table_privilege(runtime_role, relation.oid, 'UPDATE')
          AND has_table_privilege(runtime_role, relation.oid, 'DELETE')
      );
    IF missing_privileges <> 0 THEN
        RAISE EXCEPTION
            'Flyway runtime grant verification failed for % tables or views',
            missing_privileges;
    END IF;

    SELECT count(*)
    INTO missing_privileges
    FROM pg_class AS relation
    JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
    WHERE namespace.nspname = 'public'
      AND relation.relkind = 'S'
      AND NOT (
          has_sequence_privilege(runtime_role, relation.oid, 'USAGE')
          AND has_sequence_privilege(runtime_role, relation.oid, 'SELECT')
          AND has_sequence_privilege(runtime_role, relation.oid, 'UPDATE')
      );
    IF missing_privileges <> 0 THEN
        RAISE EXCEPTION
            'Flyway runtime grant verification failed for % sequences',
            missing_privileges;
    END IF;

    SELECT count(*)
    INTO missing_privileges
    FROM pg_proc AS routine
    JOIN pg_namespace AS namespace ON namespace.oid = routine.pronamespace
    WHERE namespace.nspname = 'public'
      AND NOT has_function_privilege(runtime_role, routine.oid, 'EXECUTE');
    IF missing_privileges <> 0 THEN
        RAISE EXCEPTION
            'Flyway runtime grant verification failed for % functions',
            missing_privileges;
    END IF;

    IF runtime_role <> current_user THEN
        IF EXISTS (
            SELECT 1
            FROM pg_database
            JOIN pg_roles ON pg_roles.oid = pg_database.datdba
            WHERE pg_database.datname = current_database()
              AND pg_roles.rolname = runtime_role
        ) THEN
            RAISE EXCEPTION
                'Flyway runtime role % must not own the database',
                runtime_role;
        END IF;
        IF has_schema_privilege(runtime_role, 'public', 'CREATE') THEN
            RAISE EXCEPTION
                'Flyway runtime role % retains CREATE on schema public',
                runtime_role;
        END IF;
        IF has_database_privilege(runtime_role, current_database(), 'CREATE') THEN
            RAISE EXCEPTION
                'Flyway runtime role % retains CREATE on the application database',
                runtime_role;
        END IF;
        IF EXISTS (
            SELECT 1
            FROM pg_class AS relation
            JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
            JOIN pg_roles ON pg_roles.oid = relation.relowner
            WHERE namespace.nspname = 'public'
              AND pg_roles.rolname = runtime_role
        ) OR EXISTS (
            SELECT 1
            FROM pg_proc AS routine
            JOIN pg_namespace AS namespace ON namespace.oid = routine.pronamespace
            JOIN pg_roles ON pg_roles.oid = routine.proowner
            WHERE namespace.nspname = 'public'
              AND pg_roles.rolname = runtime_role
        ) THEN
            RAISE EXCEPTION
                'Flyway runtime role % must not own schema objects',
                runtime_role;
        END IF;
        IF EXISTS (
            SELECT 1
            FROM pg_extension
            JOIN pg_roles ON pg_roles.oid = pg_extension.extowner
            WHERE pg_roles.rolname = runtime_role
        ) THEN
            RAISE EXCEPTION
                'Flyway runtime role % must not own database extensions',
                runtime_role;
        END IF;
        IF has_table_privilege(runtime_role, 'public.flyway_schema_history', 'INSERT')
           OR has_table_privilege(runtime_role, 'public.flyway_schema_history', 'UPDATE')
           OR has_table_privilege(runtime_role, 'public.flyway_schema_history', 'DELETE')
           OR has_table_privilege(runtime_role, 'public.flyway_schema_history', 'TRUNCATE')
           OR has_table_privilege(runtime_role, 'public.flyway_schema_history', 'REFERENCES')
           OR has_table_privilege(runtime_role, 'public.flyway_schema_history', 'TRIGGER') THEN
            RAISE EXCEPTION
                'Flyway runtime role % retains write access to Flyway history',
                runtime_role;
        END IF;
        IF to_regclass('public._sqlx_migrations') IS NOT NULL
           AND (
               has_table_privilege(runtime_role, 'public._sqlx_migrations', 'INSERT')
               OR has_table_privilege(runtime_role, 'public._sqlx_migrations', 'UPDATE')
               OR has_table_privilege(runtime_role, 'public._sqlx_migrations', 'DELETE')
               OR has_table_privilege(runtime_role, 'public._sqlx_migrations', 'TRUNCATE')
               OR has_table_privilege(runtime_role, 'public._sqlx_migrations', 'REFERENCES')
               OR has_table_privilege(runtime_role, 'public._sqlx_migrations', 'TRIGGER')
           ) THEN
            RAISE EXCEPTION
                'Flyway runtime role % retains write access to SQLx history',
                runtime_role;
        END IF;
    END IF;
END
$flyway_runtime_grants$;
