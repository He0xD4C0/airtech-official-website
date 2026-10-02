#!/bin/bash
set -euo pipefail

: "${AIRTEK_DEPLOY_HOST:?set AIRTEK_DEPLOY_HOST}"
: "${AIRTEK_DEPLOY_USER:?set AIRTEK_DEPLOY_USER}"
: "${AIRTEK_KNOWN_HOSTS:?set AIRTEK_KNOWN_HOSTS}"
release_sha=${GIT_COMMIT:-$(git rev-parse HEAD)}
image_prefix=${AIRTEK_IMAGE_PREFIX:?set AIRTEK_IMAGE_PREFIX from .env or the release pipeline}
remote_dir="/tmp/airtek-release-$release_sha"
ssh_options=(-o BatchMode=yes -o StrictHostKeyChecking=yes -o "UserKnownHostsFile=$AIRTEK_KNOWN_HOSTS")

ssh "${ssh_options[@]}" "$AIRTEK_DEPLOY_USER@$AIRTEK_DEPLOY_HOST" "install -d -m 0700 '$remote_dir'"
scp "${ssh_options[@]}" infra/compose/production.app.yaml infra/deploy/deploy-app.sh \
  "$AIRTEK_DEPLOY_USER@$AIRTEK_DEPLOY_HOST:$remote_dir/"
ssh "${ssh_options[@]}" "$AIRTEK_DEPLOY_USER@$AIRTEK_DEPLOY_HOST" \
  "chmod 0500 '$remote_dir/deploy-app.sh' && sudo '$remote_dir/deploy-app.sh' '$release_sha' '$image_prefix' '$remote_dir/production.app.yaml'"
