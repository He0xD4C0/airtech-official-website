#!/bin/sh
set -eu

validate_identifier() {
  name=$1
  value=$2
  case "$value" in
    ''|[!a-z_]*|*[!a-z0-9_]*)
      echo "$name must be a lowercase PostgreSQL identifier." >&2
      exit 1
      ;;
  esac
  if [ "${#value}" -gt 63 ]; then
    echo "$name must be at most 63 characters." >&2
    exit 1
  fi
}

validate_identifier AIRTEK_DATABASE_NAME "$AIRTEK_DATABASE_NAME"
validate_identifier AIRTEK_MIGRATOR_ROLE "$AIRTEK_MIGRATOR_ROLE"
validate_identifier AIRTEK_RUNTIME_ROLE "$AIRTEK_RUNTIME_ROLE"

if [ "$AIRTEK_MIGRATOR_ROLE" = "$AIRTEK_RUNTIME_ROLE" ]; then
  echo "Migration and runtime roles must be distinct." >&2
  exit 1
fi

psql --set ON_ERROR_STOP=1 \
  --set database_name="$AIRTEK_DATABASE_NAME" \
  --set migrator_role="$AIRTEK_MIGRATOR_ROLE" \
  --set migrator_password="$AIRTEK_MIGRATOR_PASSWORD" \
  --set runtime_role="$AIRTEK_RUNTIME_ROLE" \
  --set runtime_password="$AIRTEK_RUNTIME_PASSWORD" <<'SQL'
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L', :'migrator_role', :'migrator_password')
WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = :'migrator_role') \gexec
SELECT format('ALTER ROLE %I LOGIN PASSWORD %L NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION', :'migrator_role', :'migrator_password') \gexec

SELECT format('CREATE ROLE %I LOGIN PASSWORD %L', :'runtime_role', :'runtime_password')
WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = :'runtime_role') \gexec
SELECT format('ALTER ROLE %I LOGIN PASSWORD %L NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION', :'runtime_role', :'runtime_password') \gexec

SELECT format('ALTER DATABASE %I OWNER TO %I', :'database_name', :'migrator_role') \gexec
SELECT format('REVOKE ALL ON DATABASE %I FROM %I', :'database_name', :'runtime_role') \gexec
SELECT format('GRANT CONNECT, TEMPORARY ON DATABASE %I TO %I', :'database_name', :'runtime_role') \gexec
SQL

psql --dbname "$AIRTEK_DATABASE_NAME" --set ON_ERROR_STOP=1 \
  --set runtime_role="$AIRTEK_RUNTIME_ROLE" <<'SQL'
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
SELECT format('GRANT USAGE ON SCHEMA public TO %I', :'runtime_role') \gexec
SQL

echo "PostgreSQL deployment and runtime roles are ready."
