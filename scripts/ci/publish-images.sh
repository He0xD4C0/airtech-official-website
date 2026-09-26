#!/bin/bash
set -euo pipefail

release_sha=${GIT_COMMIT:-$(git rev-parse HEAD)}
image_prefix=${AIRTEK_IMAGE_PREFIX:-ghcr.io/he0xd4c0/airtekpower}
if ! printf '%s' "$release_sha" | grep -Eq '^[0-9a-f]{40}$'; then
  echo "GIT_COMMIT must be a full lowercase Git SHA." >&2
  exit 2
fi

mkdir -p reports
export AIRTEK_IMAGE_PREFIX="$image_prefix"
export AIRTEK_IMAGE_TAG="$release_sha"
export AIRTEK_IMAGE_REVISION="$release_sha"
docker buildx bake --file infra/docker/docker-bake.hcl --push

for component in public-web admin-web platform migrations gateway; do
  image="$image_prefix-$component:$release_sha"
  trivy image --quiet --format cyclonedx --output "reports/$component.sbom.cdx.json" "$image" || true
  trivy image --quiet --format json --output "reports/$component.vulnerabilities.json" "$image" || true
done
