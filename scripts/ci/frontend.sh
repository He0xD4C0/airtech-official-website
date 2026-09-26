#!/bin/bash
set -euo pipefail

corepack pnpm install --frozen-lockfile
corepack pnpm check:contracts
corepack pnpm test:source-lines
corepack pnpm check:architecture
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm test
corepack pnpm check:production
corepack pnpm check:deployment
corepack pnpm check:media-boundary
corepack pnpm check:compose
git diff --check
