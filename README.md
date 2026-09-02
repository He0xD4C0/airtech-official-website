# AIRTEKPOWER Website Platform

This repository contains the public AIRTEKPOWER website, the management portal, and the Rust core platform described in the approved website-growth implementation plan.

## Applications

| Application | Runtime | Local port | Indexing policy |
| --- | --- | ---: | --- |
| Public website | Vue 3 + Vite + Vike SSR | 3000 | Published public pages only |
| Management portal | Vue 3 + Vite SPA | 3100 | Always `noindex`; no sitemap |
| Platform API | Rust + Axum | 8080 | `Disallow: /`; no sitemap |
| Worker | Rust | none | Internal only |
| HTTP gateway | Nginx host router | 8088 | Local integration entry point; TLS terminates outside this stack |

Vike is the SSR layer used by the public Vite application; it does not replace Vite. The admin application is a separate Vite SPA and is independently built and deployed.

## Local development

Prerequisites:

- Node.js 22+
- pnpm 11+
- Rust 1.98 toolchain (pinned by `rust-toolchain.toml`)
- Docker with Compose for PostgreSQL; optional object-storage diagnostics

The repository includes a ready-to-run, Git-ignored `.env` and a tracked
`.env.example`. Keep machine-specific values in `.env`; never place production
credentials there. The main configuration groups are:

| Group | Variables |
| --- | --- |
| Host mappings | `AIRTEK_PUBLIC_HOST_PORT`, `AIRTEK_ADMIN_HOST_PORT`, `AIRTEK_API_HOST_PORT`, `AIRTEK_GATEWAY_HOST_PORT`, debug PostgreSQL/MinIO ports |
| Browser and security origins | `PUBLIC_HOST`, `ADMIN_HOST`, `API_HOST`, `AIRTEK_*_ORIGIN`, `VITE_*_BASE_URL`, `PUBLIC_API_BROWSER_ORIGIN` |
| Local persistence | `POSTGRES_*`, `AIRTEK_DATABASE_URL_INTERNAL`, host-side `DATABASE_URL`, `MINIO_ROOT_*` |
| Authentication, private staging and network trust | setup-only `AIRTEK_ADMIN_BOOTSTRAP_TOKEN`, independent TOTP/preview/Product Staging/Analytics HMAC/invitation replay keys, gateway subnet/address, exact trusted-proxy CIDRs |
| Data lifecycle | `AIRTEK_GUEST_RAW_RETENTION_DAYS`, `AIRTEK_GUEST_AGGREGATE_RETENTION_MONTHS`, `AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION` |
| Analytics vocabulary | `AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES`, `AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS`, `AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS` register the only UTM identifiers the API may store; unknown free text is rejected |
| Development behavior | `VITE_ENABLE_DEVTOOLS`, cookie-banner test switch, `RUST_LOG` |
| Reserved adapters | Feishu, S3-compatible storage and GA4 placeholders; blank values do not enable an adapter |

Internal application ports remain the fixed architecture contract. Host-side
port variables only change loopback diagnostics and avoid collisions; if the
gateway host port changes, update the browser-visible origin values together.
Database credentials and both internal/host connection URLs must also stay in
sync. Generate five independent random 32-byte Base64 keys for TOTP, preview
signing, Product Master private staging, analytics-token HMAC, and encrypted
invitation idempotency replay before retaining real local data. Invitation
tokens remain hashed in their source table; the short-lived replay response is
stored only as an AES-256-GCM envelope so a retry can return the original token.

Content preview links are short-lived bearer capabilities, limited to ten
minutes and one immutable content revision. The authenticated Admin API binds
each token to the issuing Admin user and session. Every preview read revalidates
that the user is active, the session is unrevoked and unexpired, TOTP remains
confirmed, and the user still has `content.read`; disabling the user, revoking
the session, or removing the permission invalidates the URL immediately. Public
SSR exchanges the query token for that exact revision through the internal API
using an `Authorization: Bearer` header. This is intentional: the Admin/API
host-only cookie is not sent to the Public origin.
Preview responses are private, never cached or indexed, and never fall back to
published content. The checked-in gateway disables access logging for the
preview route so its bearer query token is not copied into container logs. Do
not otherwise log, persist, or forward a preview URL; rotating
`AIRTEK_PREVIEW_SIGNING_KEY` immediately invalidates all outstanding links.

The complete local
stack builds all three applications, applies migrations through the one-shot
`platform-migrate` container, then starts the API and worker only after that
container exits successfully:

```sh
docker compose up --build
```

The host-isolated HTTP entry points are:

- `http://www.localhost:8088` for the public site
- `http://admin.localhost:8088` for the management portal
- `http://api.localhost:8088` for the API

The fixed application ports `3000`, `3100`, and `8080` also remain published
to `127.0.0.1` for direct diagnostics. They are not reachable from another
machine and are not substitutes for the Host boundary. Set
`AIRTEK_BIND_ADDRESS` only for an intentional local-network test. Production
TLS and external port `443` belong to the hosting provider or an outer ingress.

PostgreSQL is network-internal in the base stack, so it cannot collide with or
be reached from a host PostgreSQL instance. MinIO is disabled by default because
the object-storage adapter is not connected. For native source development,
opt in to the loopback-only database mapping, install dependencies, and start
PostgreSQL first. The
Rust binaries load the root `.env` through `dotenvy` when they are launched
from the repository root. Run the migration, API, worker, and frontend commands
in separate terminals:

```sh
pnpm install
docker compose -f compose.yaml -f compose.debug.yaml up -d postgres
cargo run --manifest-path services/platform/Cargo.toml --bin airtek-migrate
cargo run --manifest-path services/platform/Cargo.toml --bin airtek-api
cargo run --manifest-path services/platform/Cargo.toml --bin airtek-worker
pnpm dev
```

The debug mapping defaults to host port `54320`, so the root `.env.example`
uses `postgres://airtek:airtek@localhost:54320/airtek`. Override
`AIRTEK_POSTGRES_DEBUG_PORT` if that port is occupied. MinIO diagnostics use
`19000` and `19001` by default and are configurable independently.

Use the individual frontend commands when working on one application:

```sh
pnpm dev:web
pnpm dev:admin
```

Only when working on the reserved object-storage adapter, start its API and
console with an explicit profile and the same loopback-only debug override:

```sh
docker compose -f compose.yaml -f compose.debug.yaml --profile object-storage up -d minio
```

The Admin SPA always uses the Rust API. Runtime mock records and local mock
authentication are not supported; deterministic UI fixtures live only inside
tests.

Developer mode is also explicit opt-in. Set `VITE_ENABLE_DEVTOOLS=true`, start
the API with Cargo's `devtools` feature, and sign in as a database-backed user
with `devtools.shell`; leaving the variable absent or `false` keeps the Admin
route and terminal package out of the module graph. Production builds reject a
true value instead of silently packaging the terminal.

The running API, worker and migration process all require `DATABASE_URL`.
In-memory repositories exist only behind isolated test construction and are not
a supported server mode. MinIO is present as a reserved local dependency; the
current platform does not yet include an object-storage network adapter.

## Production image boundary

`compose.production.yaml` is a provider-neutral, image-only deployment
boundary. It requires separate immutable references for Public Web, Admin Web,
Platform, and Gateway, exposes only the Gateway to the outer TLS ingress, and
keeps ports `3000`, `3100`, and `8080` internal. This permits Public and Admin
to be promoted or rolled back independently. Start from
`infra/deploy/production.env.example`; replace every example image, host,
origin, database URL, and secret through the deployment platform before use.

The public image must be built with the final `VITE_PUBLIC_ORIGIN` and
`VITE_PUBLIC_API_BASE_URL`; the admin image must be built with the final
`VITE_ADMIN_API_BASE_URL`. Runtime origin values are security and SSR settings,
not a way to rewrite browser code that was already compiled. Remove the
bootstrap token after the initial Super Admin has been created. The production
gateway listens on loopback port `8088` for an outer ingress; only that ingress
publishes HTTPS `443`.

Validate both deployment shapes without starting them:

```sh
docker compose config --quiet
docker compose -f compose.yaml -f compose.debug.yaml --profile object-storage config --quiet
docker compose --env-file infra/deploy/production.env.example -f compose.production.yaml config --quiet
```

## Verification

```sh
pnpm lint
pnpm typecheck
pnpm test
pnpm test:e2e
pnpm test:e2e:stack
pnpm check:contracts
pnpm check:production
pnpm check:deployment
docker compose config --quiet
cargo fmt --manifest-path services/platform/Cargo.toml --all -- --check
cargo clippy --manifest-path services/platform/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path services/platform/Cargo.toml
```

The Playwright suite is a production-shape browser contract. The self-contained
runner creates a disposable PostgreSQL volume, starts the complete Compose
stack, and drives three same-site gateway hosts under `airtek.test`. Chromium
maps those test-only names to loopback so host routing, CORS and host-only
cookies behave like the production subdomains. Global setup creates a TOTP-enabled administrator and
publishes the minimum placeholder Home, navigation, footer, and General
Information records through the real Admin API. It then verifies SSR and origin
isolation plus authenticated News, product import, analytics, invitation, role,
and authorization workflows. The disposable stack is removed after the run:

```sh
pnpm exec playwright install chromium
pnpm test:e2e:stack
```

Playwright loads the root `.env`. The isolated runner defaults to gateway port
`8088` and sets `E2E_PUBLIC_ORIGIN`, `E2E_ADMIN_ORIGIN`, and `E2E_API_ORIGIN`
to the corresponding gateway hosts. `pnpm test:e2e` remains available for an
already-running equivalent environment; authenticated mutation scenarios only
run when the caller explicitly enables the isolated-stack flags. CI uses the
self-contained runner and uploads Playwright traces/reports on failure.

The axe checks use WCAG 2/2.1 A and AA rules and reject all serious or critical
findings on the Public SSR home page and Admin login. No selectors or rules are
excluded from those scans.

### API contract generation

The Rust `openapi::document()` function is the API contract source. The export
command compiles it with the `production` feature, writes the deterministic
snapshot at `packages/contracts/openapi/openapi.production.json`, and regenerates
`packages/contracts/src/generated/openapi.ts` with `openapi-typescript`:

```sh
pnpm generate:contracts
pnpm check:contracts
```

`check:contracts` performs a fresh Rust export and TypeScript generation in a
temporary directory, then fails on either snapshot or generated-code drift. It
also compares every production Rust router path/method with the document and
rejects any DevTools path or terminal schema in the production contract.
The generated package exports path/operation types and named request/response
types; its small fetch helper remains available for application transport.

The production isolation check rejects an admin bundle containing DevTools,
public manifests, webmaster verification files, or sitemaps. Deployment checks
also assert the migration dependency, non-root images, gateway Host boundary,
and production DevTools route exclusion. Production Rust builds use the
`production` feature without `devtools`; the mutually enabled combination must
fail to compile.

## Database-driven content and Product Master

News, General Information, public routes, navigation, footer, products, guest
attribution, users and roles are durable PostgreSQL records. Business/editorial
copy is never read from a production frontend fallback. Missing optional data is
omitted; a missing public record returns 404 and an unavailable required
projection fails closed rather than fabricating content.

The initial approved Product Master is imported through the Admin Product
Master screen or its API. Each CSV import is keyed by deployment environment,
SHA-256 and mapping version, rejects malformed rows without creating products,
encrypts commercial price columns in private staging, and records unresolved
filename-only assets. Product facts and portal-owned localized presentation use
independent immutable revision histories; publishing binds one presentation
revision to the exact accepted Product Master revision. Noise remains excluded
from the public projection until its measurement setup is supplied.

## Data and publication rules

- The owner-approved Product Master CSV is the authoritative initial snapshot. Future Feishu changes still pass through staging, three-way diff, validation and an explicit publish action.
- The HTML demos under `docs/Plan & Solution/` are interaction references only. Their product specifications, curves, downloads, scores, and colors are not production data.
- The public application reads only published projections. Drafts, source snapshots, conflicts, RFQ records, users, and audit data are admin-only.
- Public RFQs never accept file uploads.
- Placeholder pages are `noindex` and omitted from public sitemaps.
- Development fixture ownership is one-way: clearing `isPlaceholder` in Admin
  atomically takes the record into editorial ownership. Later development seed
  runs retain their ledger entry but skip that record without overwriting it.
- News metadata and content revisions are one transaction through the dedicated
  News API. Generic Content list and mutation endpoints exclude or reject News.
- `docs/` is preserved as source evidence and is not edited by the implementation.

## Current integration status

- Exact products may be published only from the audited Product Master snapshot.
  Missing curves, certifications, downloads and case outcomes remain omitted
  until their own controlled evidence exists.
- Public SSR serves `robots.txt` and every sitemap document dynamically. The
  Rust discovery feed contributes only published, canonical, indexable,
  non-placeholder content and products; API failure is handled fail-closed.
  Static files with those route names are intentionally excluded so they cannot
  bypass the runtime publication filter.
- The management portal provides real News, General Information, Product Master
  import/presentation, guest-source analytics and identity-management workflows.
  Reserved external adapters remain separate and are not represented as live.
- Durable publication revisions and outbox hand-off exist. Runtime sitemaps read
  the published projection directly, while CDN invalidation and external search
  indexing adapters are not wired; no external provider refresh should be
  inferred from a completed internal hook.
- Feishu credentials, S3-compatible values, and GA4 values are reserved
  configuration only. This scaffold does not currently connect those network
  adapters.
- Consent-gated first-party analytics events are sanitized in the public client,
  accepted by the Rust API, and exposed through the admin aggregate. A GA4
  provider adapter is not connected, so no GA4 loading is implied.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for service and security boundaries.
