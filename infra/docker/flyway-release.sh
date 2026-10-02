#!/bin/sh
# Release wrapper for the migration artifact: pre-migration dump, then Flyway,
# then an automatic restore of that dump when the migration fails.
#
# Exit codes are part of the contract with the on-host CD agent:
#   0  no pending migration, or the migration succeeded (dump removed)
#   1  pre-flight failure before any schema change (configuration, dump, connect)
#   2  migration failed, the pre-migration dump was restored
#   3  migration failed and the restore also failed; database state is unknown
#
# Every failure leaves a record under the migration state directory, and a
# restored failure additionally writes an `operation_runs` row so the database
# itself remembers that a release attempt failed.
set -u

log() { printf '%s\n' "$*" >&2; }

state_dir=${AIRTEK_MIGRATION_STATE_DIR:-/var/lib/airtek/migration-state}
migrations_dir=${AIRTEK_MIGRATIONS_DIR:-/flyway/project/migrations}
entrypoint_bin=${AIRTEK_FLYWAY_ENTRYPOINT:-/usr/local/bin/airtek-flyway}
psql_bin=${AIRTEK_PSQL_BIN:-psql}
pg_dump_bin=${AIRTEK_PG_DUMP_BIN:-pg_dump}
pg_restore_bin=${AIRTEK_PG_RESTORE_BIN:-pg_restore}
release_tag=${AIRTEK_RELEASE_TAG:-${AIRTEK_IMAGE_TAG:-unknown}}
image_revision=${AIRTEK_IMAGE_REVISION:-unknown}
keep_failed_dump=${AIRTEK_KEEP_FAILED_DUMP:-false}

: "${FLYWAY_URL:?FLYWAY_URL is required for the release command}"
: "${FLYWAY_USER:?FLYWAY_USER is required for the release command}"
: "${FLYWAY_PASSWORD:?FLYWAY_PASSWORD is required for the release command}"

if [ "${FLYWAY_URL#jdbc:postgresql://}" = "$FLYWAY_URL" ]; then
  log 'FLYWAY_URL must be a jdbc:postgresql:// URL'
  exit 1
fi
url=${FLYWAY_URL#jdbc:postgresql://}
host_port=${url%%/*}
if [ "$host_port" = "$url" ] || [ -z "$host_port" ]; then
  log 'FLYWAY_URL must include a host and database name'
  exit 1
fi
remainder=${url#*/}
PGDATABASE=${remainder%%\?*}
if [ -z "$PGDATABASE" ]; then
  log 'FLYWAY_URL must include a database name'
  exit 1
fi
case "$host_port" in
  *:*) PGHOST=${host_port%%:*}; PGPORT=${host_port##*:} ;;
  *) PGHOST=$host_port; PGPORT=5432 ;;
esac
PGUSER=$FLYWAY_USER
PGPASSWORD=$FLYWAY_PASSWORD
export PGHOST PGPORT PGDATABASE PGUSER PGPASSWORD

latest_local=0
for file in "$migrations_dir"/V*__*.sql; do
  [ -f "$file" ] || continue
  name=${file##*/}
  version=${name#V}
  version=${version%%__*}
  case "$version" in
    ''|*[!0-9]*) continue ;;
  esac
  if [ "$version" -gt "$latest_local" ]; then latest_local=$version; fi
done
if [ "$latest_local" -le 0 ]; then
  log "no versioned migrations found in $migrations_dir"
  exit 1
fi

current_version() {
  if [ "$("$psql_bin" -X -A -t -v ON_ERROR_STOP=1 -c \
      "select to_regclass('public.flyway_schema_history') is not null")" = "t" ]; then
    "$psql_bin" -X -A -t -v ON_ERROR_STOP=1 -c \
      "select coalesce(max(case when version ~ '^[0-9]+$' then version::int end),0)
         from public.flyway_schema_history where success is true"
  else
    printf '0\n'
  fi
}

if ! before=$(current_version); then
  log 'cannot read the Flyway history with the migration credentials'
  exit 1
fi
before=$(printf '%s' "$before" | tr -d '[:space:]')

pending=''
for file in "$migrations_dir"/V*__*.sql; do
  [ -f "$file" ] || continue
  name=${file##*/}
  version=${name#V}
  version=${version%%__*}
  case "$version" in
    ''|*[!0-9]*) continue ;;
  esac
  if [ "$version" -gt "$before" ] && [ "$version" -le "$latest_local" ]; then
    pending="${pending}${pending:+,}$version"
  fi
done

if [ -z "$pending" ]; then
  log "schema already current at V$before (target V$latest_local); nothing to migrate"
  exit 0
fi

mkdir -p "$state_dir/failures" 2>/dev/null || true
dump="$state_dir/pre-migrate-$release_tag.dump"
migration_log="$state_dir/flyway-$release_tag.log"
rm -f "$dump"
if ! "$pg_dump_bin" --format=custom --no-owner --no-acl --file "$dump"; then
  log 'pre-migration dump failed; refusing to migrate'
  rm -f "$dump"
  exit 1
fi
dump_sha256=$(sha256sum "$dump" | awk '{print $1}')
dump_bytes=$(wc -c < "$dump" | tr -d ' ')
log "pre-migration dump written for pending V$pending"

if "$entrypoint_bin" migrate >"$migration_log" 2>&1; then
  rm -f "$dump" "$migration_log"
  log "migration succeeded through V$latest_local; pre-migration dump removed"
  exit 0
else
  migration_status=$?
fi

restore_started=$(date +%s 2>/dev/null || printf '0')
"$psql_bin" -X -A -t -v ON_ERROR_STOP=1 -c \
  "select pg_terminate_backend(pid) from pg_stat_activity
    where datname = current_database() and pid <> pg_backend_pid()
      and backend_type = 'client backend'" >/dev/null 2>&1 || true

restore_succeeded=false
restored_version=''
if "$pg_restore_bin" --clean --if-exists --no-owner --no-acl \
    --dbname "$PGDATABASE" "$dump" >>"$migration_log" 2>&1; then
  if restored=$(current_version); then
    restored_version=$(printf '%s' "$restored" | tr -d '[:space:]')
    if [ "$restored_version" = "$before" ]; then restore_succeeded=true; fi
  fi
fi
restore_seconds=$(( $(date +%s 2>/dev/null || printf '0') - restore_started ))

recorded_at=$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || printf 'unknown')
record="$state_dir/failures/$recorded_at-$release_tag.json"
output_tail=$(tail -c 32768 "$migration_log" 2>/dev/null || true)
if [ "$restore_succeeded" = "true" ]; then exit_code=2; else exit_code=3; fi
if [ "$restore_succeeded" = "true" ]; then
  recommendation='Database restored to the pre-migration state; fix the migration and release a new tag.'
else
  recommendation='Restore failed; keep the dump, inspect the database manually, and do not deploy other releases.'
fi

if command -v jq >/dev/null 2>&1; then
  jq -n \
    --arg kind migrationApply \
    --arg status failed \
    --arg recordedAt "$recorded_at" \
    --arg host "$(hostname 2>/dev/null || printf 'unknown')" \
    --arg container "${HOSTNAME:-unknown}" \
    --arg releaseTag "$release_tag" \
    --arg imageRevision "$image_revision" \
    --argjson previousVersion "$before" \
    --argjson targetVersion "$latest_local" \
    --arg pendingVersions "$pending" \
    --argjson flywayExitStatus "$migration_status" \
    --arg flywayOutputTail "$output_tail" \
    --arg dumpPath "$dump" \
    --argjson dumpBytes "${dump_bytes:-0}" \
    --arg dumpSha256 "${dump_sha256:-}" \
    --argjson restoreAttempted true \
    --argjson restoreSucceeded "$restore_succeeded" \
    --argjson restoreSeconds "$restore_seconds" \
    --arg restoredVersion "${restored_version:-unknown}" \
    --argjson exitCode "$exit_code" \
    --arg recommendedAction "$recommendation" \
    '{kind:$kind,status:$status,recordedAt:$recordedAt,host:$host,container:$container,
      releaseTag:$releaseTag,imageRevision:$imageRevision,previousVersion:$previousVersion,
      targetVersion:$targetVersion,pendingVersions:$pendingVersions,
      flywayExitStatus:$flywayExitStatus,flywayOutputTail:$flywayOutputTail,
      dump:{path:$dumpPath,bytes:$dumpBytes,sha256:$dumpSha256},
      restore:{attempted:$restoreAttempted,succeeded:$restoreSucceeded,seconds:$restoreSeconds,
        versionAfterRestore:$restoredVersion},
      databaseStateUnknown:($restoreSucceeded|not),exitCode:$exitCode,
      recommendedAction:$recommendedAction}' >"$record" 2>>"$migration_log" || true
else
  printf '{"kind":"migrationApply","status":"failed","recordedAt":"%s","releaseTag":"%s","exitCode":%s}\n' \
    "$recorded_at" "$release_tag" "$exit_code" >"$record" 2>>"$migration_log" || true
fi

if [ "$restore_succeeded" = "true" ]; then
  "$psql_bin" -X -v ON_ERROR_STOP=1 -q -c "
    insert into operation_runs (id,kind,status,reason,result,created_at,updated_at)
    select gen_random_uuid(),'migrationApply','failed',
      \$airtek_reason\$Release $release_tag migration failed; the pre-migration dump was restored.\$airtek_reason\$,
      \$airtek_result\$$(cat "$record" 2>/dev/null)\$airtek_result\$, now(), now()
    where not exists (
      select 1 from operation_runs
      where kind='migrationApply' and status='failed'
        and result->>'recordedAt' = '$recorded_at'
    )" >/dev/null 2>>"$migration_log" \
    || log 'failed to persist the migration failure record in operation_runs'
fi

ls -1t "$state_dir/failures" 2>/dev/null | tail -n +51 | while read -r stale; do
  rm -f "$state_dir/failures/$stale"
done

keep_dump=false
if [ "$keep_failed_dump" = "true" ] || [ "$restore_succeeded" != "true" ]; then
  keep_dump=true
fi
if [ "$keep_dump" = "true" ]; then
  log "pre-migration dump retained at $dump"
else
  rm -f "$dump"
fi

log "migration failed; failure record written to $record"
log "$recommendation"
exit "$exit_code"
