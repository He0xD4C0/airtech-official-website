#!/bin/sh
set -eu

release_id=${1:-}
image_prefix=${2:-}
source_compose=${3:-./compose.production.yaml}
deploy_root=${AIRTEK_DEPLOY_ROOT:-/opt/airtek/app}
production_env=${AIRTEK_PRODUCTION_ENV:-/etc/airtek/production.env}

if ! printf '%s' "$release_id" | grep -Eq '^[0-9a-f]{40}$'; then
  echo "Release ID must be a full lowercase Git commit SHA." >&2
  exit 2
fi
case "$image_prefix" in
  ''|*[!A-Za-z0-9._:/-]*)
    echo "Image prefix contains unsupported characters." >&2
    exit 2
    ;;
esac

if [ ! -r "$source_compose" ]; then
  echo "Production Compose source is not readable: $source_compose" >&2
  exit 2
fi
if [ ! -r "$production_env" ]; then
  echo "Production environment is not readable: $production_env" >&2
  exit 2
fi

mkdir -p "$deploy_root/releases"
lock_file="$deploy_root/deploy.lock"
if [ "${AIRTEK_DEPLOY_LOCKED:-0}" != 1 ]; then
  export AIRTEK_DEPLOY_LOCKED=1
  exec flock -n -E 75 "$lock_file" "$0" "$@"
fi

release_dir="$deploy_root/releases/$release_id"
mkdir -p "$release_dir"
cp "$source_compose" "$release_dir/compose.production.yaml"

cat >"$release_dir/images.env" <<EOF
AIRTEK_PUBLIC_WEB_IMAGE=$image_prefix/public-web:$release_id
AIRTEK_ADMIN_WEB_IMAGE=$image_prefix/admin-web:$release_id
AIRTEK_PLATFORM_IMAGE=$image_prefix/platform:$release_id
AIRTEK_MIGRATIONS_IMAGE=$image_prefix/migrations:$release_id
AIRTEK_GATEWAY_IMAGE=$image_prefix/gateway:$release_id
EOF

previous_release=
if [ -L "$deploy_root/current" ]; then
  previous_release=$(readlink -f "$deploy_root/current" || true)
fi
deployment_started=0

compose_release() {
  target=$1
  shift
  docker compose \
    --env-file "$production_env" \
    --env-file "$target/images.env" \
    -f "$target/compose.production.yaml" "$@"
}

rollback() {
  if [ "$deployment_started" -ne 1 ] || [ -z "$previous_release" ] || [ ! -d "$previous_release" ]; then
    return
  fi
  echo "Application health check failed; restoring previous application images." >&2
  compose_release "$previous_release" up -d --no-deps --remove-orphans --wait --wait-timeout 180 \
    platform-api platform-worker public-web admin-web gateway
  ln -sfn "$previous_release" "$deploy_root/current"
}

finish() {
  status=$?
  trap - EXIT
  if [ "$status" -ne 0 ]; then
    rollback || echo "Automatic application rollback also failed." >&2
  fi
  exit "$status"
}
trap finish EXIT

set -a
# production.env is an administrator-owned shell-compatible env file.
. "$production_env"
set +a

: "${PUBLIC_HOST:?set PUBLIC_HOST in production.env}"
: "${ADMIN_HOST:?set ADMIN_HOST in production.env}"
: "${API_HOST:?set API_HOST in production.env}"
ingress_address=${AIRTEK_INGRESS_BIND_ADDRESS:-127.0.0.1}
ingress_port=${AIRTEK_INGRESS_HTTP_PORT:-8088}
if [ "$ingress_address" != 127.0.0.1 ]; then
  echo "Application Gateway must remain bound to 127.0.0.1 behind the TLS ingress." >&2
  exit 2
fi

docker network inspect "${AIRTEK_PRODUCTION_NETWORK:-airtek-production}" >/dev/null
compose_release "$release_dir" config --quiet
compose_release "$release_dir" pull
compose_release "$release_dir" run --rm flyway-migrate migrate
compose_release "$release_dir" run --rm flyway-migrate validate

deployment_started=1
compose_release "$release_dir" up -d --no-deps --remove-orphans --wait --wait-timeout 180 \
  platform-api platform-worker public-web admin-web gateway

curl --fail --silent --show-error "http://127.0.0.1:$ingress_port/healthz" >/dev/null
curl --fail --silent --show-error -H "Host: $API_HOST" "http://127.0.0.1:$ingress_port/readyz" >/dev/null
curl --fail --silent --show-error -H "Host: $PUBLIC_HOST" "http://127.0.0.1:$ingress_port/en" >/dev/null
curl --fail --silent --show-error -H "Host: $ADMIN_HOST" "http://127.0.0.1:$ingress_port/robots.txt" >/dev/null

ln -sfn "$release_dir" "$deploy_root/current"
deployment_started=0
echo "Application release $release_id is healthy."
