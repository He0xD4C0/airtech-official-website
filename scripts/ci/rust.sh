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
rustup run 1.98.0 cargo check --manifest-path services/platform/Cargo.toml --workspace --features production
rustup run 1.98.0 cargo check --manifest-path services/platform/Cargo.toml --workspace --features devtools
if rustup run 1.98.0 cargo check \
  --manifest-path services/platform/Cargo.toml --workspace --features production,devtools; then
  echo 'production+devtools unexpectedly compiled' >&2
  exit 1
fi
if rustup run 1.98.0 cargo tree --manifest-path services/platform/Cargo.toml --features production \
  | grep -E '(^| )((clap|portable-pty|tokio-tungstenite|tungstenite) v)'; then
  echo 'production dependency tree contains a DevTools package' >&2
  exit 1
fi
