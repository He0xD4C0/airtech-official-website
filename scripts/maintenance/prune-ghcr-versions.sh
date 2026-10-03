#!/usr/bin/env bash
set -euo pipefail

: "${GH_TOKEN:?GH_TOKEN is required}"
: "${PACKAGE_OWNER:?PACKAGE_OWNER is required}"
: "${PACKAGE_NAME:?PACKAGE_NAME is required}"
: "${KEEP_RELEASE_VERSIONS:?KEEP_RELEASE_VERSIONS is required}"
RETENTION_TAG_PREFIX=${RETENTION_TAG_PREFIX:-}

if ! [[ "$KEEP_RELEASE_VERSIONS" =~ ^[1-9][0-9]*$ ]]; then
  echo "KEEP_RELEASE_VERSIONS must be a positive integer." >&2
  exit 1
fi

if [[ -n "$RETENTION_TAG_PREFIX" && ! "$RETENTION_TAG_PREFIX" =~ ^[A-Za-z0-9._-]+-$ ]]; then
  echo "RETENTION_TAG_PREFIX must be an image-tag prefix ending with '-'." >&2
  exit 1
fi

owner_type=$(gh api "users/$PACKAGE_OWNER" --jq '.type')
case "$owner_type" in
  Organization) package_scope="orgs" ;;
  User) package_scope="users" ;;
  *)
    echo "Unsupported GitHub package owner type: $owner_type" >&2
    exit 1
    ;;
esac

versions_endpoint="$package_scope/$PACKAGE_OWNER/packages/container/$PACKAGE_NAME/versions"
versions_json=$(gh api --paginate --slurp "$versions_endpoint?per_page=100")

# Streams publish to separate packages, and RETENTION_TAG_PREFIX narrows
# retention further when a caller wants one stream inside a shared package: a
# version is only eligible when every tag it carries belongs to that stream, so
# a busy development stream can never prune a production image. Untagged OCI
# provenance/SBOM artifacts are excluded for the same reason.
release_versions=()
while IFS= read -r version_row; do
  [[ -n "$version_row" ]] && release_versions+=("$version_row")
done < <(
  jq -r --arg prefix "$RETENTION_TAG_PREFIX" '
    [
      .[][]
      | select((.metadata.container.tags // []) | length > 0)
      | select($prefix == "" or ((.metadata.container.tags | map(select(startswith($prefix))) | length) > 0))
      | select($prefix == "" or ((.metadata.container.tags | map(select(startswith($prefix) | not)) | length) == 0))
    ]
    | sort_by(.created_at)
    | reverse
    | .[]
    | [(.id | tostring), (.metadata.container.tags | join(",")), .created_at]
    | @tsv
  ' <<<"$versions_json"
)

release_count=${#release_versions[@]}
if (( release_count <= KEEP_RELEASE_VERSIONS )); then
  echo "$PACKAGE_NAME has $release_count ${RETENTION_TAG_PREFIX:-} image(s); nothing to prune."
  exit 0
fi

echo "$PACKAGE_NAME has $release_count ${RETENTION_TAG_PREFIX:-} image(s); keeping the newest $KEEP_RELEASE_VERSIONS."
for ((index = KEEP_RELEASE_VERSIONS; index < release_count; index += 1)); do
  IFS=$'\t' read -r version_id version_tags created_at <<<"${release_versions[$index]}"
  echo "Deleting package version $version_id ($version_tags, created $created_at)."
  gh api --method DELETE "$versions_endpoint/$version_id"
done
