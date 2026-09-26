#!/bin/bash
set -u

for project in \
  "airtek-e2e-${BUILD_TAG:-local}" \
  "airtek-postgres-${BUILD_TAG:-local}" \
  airtek-e2e airtek-ci; do
  docker compose --project-directory . -p "$project" down --volumes --remove-orphans >/dev/null 2>&1 || true
done
docker ps -aq --filter "label=com.airtek.ci=${BUILD_TAG:-local}" | xargs -r docker rm -f >/dev/null 2>&1 || true
docker network prune -f --filter "label=com.airtek.ci=${BUILD_TAG:-local}" >/dev/null 2>&1 || true
