#!/bin/bash
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install 1.88.0 --profile minimal
rustup toolchain install 1.98.0 --profile minimal --component rustfmt,clippy
rustup run 1.88.0 cargo check \
  --manifest-path services/platform/Cargo.toml \
  --locked --workspace --all-targets --features production
rustup run 1.98.0 cargo fmt \
  --manifest-path services/platform/Cargo.toml --all -- --check
node scripts/checks/assert-postgres-runtime-boundary.mjs
rustup run 1.98.0 cargo clippy \
  --manifest-path services/platform/Cargo.toml \
  --workspace --all-targets --features production -- -D warnings
rustup run 1.98.0 cargo test \
  --manifest-path services/platform/Cargo.toml --workspace
node scripts/testing/run-postgres-contracts.mjs
node scripts/checks/assert-platform-production-features.mjs
node scripts/checks/assert-platform-devtools-feature.mjs
node scripts/checks/assert-platform-feature-mutual-exclusion.mjs
cargo tree --manifest-path services/platform/Cargo.toml -p airtek-http --features production -e features \
  | grep -Eq 'aws-sdk-s3 feature "rt-tokio"|aws-sdk-s3 feature "default"'
