#!/bin/sh
set -eu

runtime_role=${FLYWAY_PLACEHOLDERS_RUNTIME_ROLE:-}
allow_shared_role=${AIRTEK_FLYWAY_ALLOW_SHARED_ROLE:-false}
schema_target=${AIRTEK_FLYWAY_TARGET:-}
case "$schema_target" in
  16 ) ;;
  * )
    echo "AIRTEK_FLYWAY_TARGET must be 16." >&2
    exit 64
    ;;
esac
case "$runtime_role" in
  [a-z_]* ) ;;
  * )
    echo "FLYWAY_PLACEHOLDERS_RUNTIME_ROLE must start with a lowercase letter or underscore." >&2
    exit 64
    ;;
esac
case "$runtime_role" in
  *[!a-z0-9_]* )
    echo "FLYWAY_PLACEHOLDERS_RUNTIME_ROLE may contain only lowercase letters, digits, and underscores." >&2
    exit 64
    ;;
esac
if [ "${#runtime_role}" -gt 63 ]; then
  echo "FLYWAY_PLACEHOLDERS_RUNTIME_ROLE must be at most 63 characters." >&2
  exit 64
fi
case "$allow_shared_role" in
  true|false ) ;;
  * )
    echo "AIRTEK_FLYWAY_ALLOW_SHARED_ROLE must be true or false." >&2
    exit 64
    ;;
esac
if [ "$runtime_role" = "${FLYWAY_USER:-}" ] && [ "$allow_shared_role" != true ]; then
  echo "The Flyway DDL role and application runtime role must be distinct." >&2
  exit 64
fi

if [ "$#" -ne 1 ]; then
  echo "Exactly one Flyway command is required." >&2
  exit 64
fi
case "$1" in
  baseline|migrate|info|validate ) flyway_command=$1 ;;
  * )
    echo "Unsupported Flyway command: $1" >&2
    exit 64
    ;;
esac

exec flyway \
  -configFiles=/flyway/project/flyway.toml \
  -locations=filesystem:/flyway/project/migrations \
  -callbackLocations=filesystem:/flyway/project/flyway/callbacks \
  -defaultSchema=public \
  -schemas=public \
  -table=flyway_schema_history \
  -encoding=UTF-8 \
  -connectRetries=10 \
  -executeInTransaction=true \
  -failOnMissingLocations=true \
  -baselineVersion=10 \
  '-baselineDescription=Validated SQLx v1-10 adoption' \
  -baselineOnMigrate=false \
  -cleanDisabled=true \
  '-ignoreMigrationPatterns=*:future' \
  -outOfOrder=false \
  -skipDefaultCallbacks=false \
  -skipDefaultResolvers=false \
  -skipExecutingMigrations=false \
  "-target=$schema_target" \
  -validateMigrationNaming=true \
  -validateOnMigrate=true \
  -placeholderReplacement=true \
  -sqlMigrationPrefix=V \
  -sqlMigrationSeparator=__ \
  -sqlMigrationSuffixes=.sql \
  "-placeholders.runtime_role=$runtime_role" \
  "-placeholders.allow_shared_role=$allow_shared_role" \
  "$flyway_command"
