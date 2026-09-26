#!/bin/bash
set -euo pipefail

corepack pnpm install --frozen-lockfile
corepack pnpm exec playwright install chromium
export AIRTEK_TOTP_ENCRYPTION_KEY="$(openssl rand -base64 32)"
export AIRTEK_PREVIEW_SIGNING_KEY="$(openssl rand -base64 32)"
export AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY="$(openssl rand -base64 32)"
export AIRTEK_ANALYTICS_TOKEN_HMAC_KEY="$(openssl rand -base64 32)"
export AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY="$(openssl rand -base64 32)"
export E2E_PUBLIC_ORIGIN=http://www.airtek.test:8088
export E2E_ADMIN_ORIGIN=http://admin.airtek.test:8088
export E2E_API_ORIGIN=http://api.airtek.test:8088
corepack pnpm test:e2e:stack
