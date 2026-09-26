#!/bin/sh
set -eu

origin=${ADMIN_API_ORIGIN:?set ADMIN_API_ORIGIN to the browser-visible API origin}
if ! printf '%s\n' "$origin" | grep -Eq '^https?://[A-Za-z0-9.-]+(:[0-9]{1,5})?$'; then
  echo "ADMIN_API_ORIGIN must be a bare HTTP(S) origin." >&2
  exit 1
fi

target=/usr/share/nginx/html/runtime-config.json
temporary="${target}.tmp"
jq -n --arg apiBaseUrl "${origin%/}/api/admin/v1" '{apiBaseUrl: $apiBaseUrl}' >"$temporary"
chmod 0444 "$temporary"
mv "$temporary" "$target"
