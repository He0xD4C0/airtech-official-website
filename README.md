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

PostgreSQL remains the database. SQLx is used only by the Rust API and Worker
for runtime data access; Flyway `13.4.0` exclusively owns schema versions and
migration history.

## Repository layout

| Path | Responsibility |
| --- | --- |
| `apps/` | Deployable Public SSR and Admin SPA applications |
| `packages/` | Shared contracts, design assets, and draft-rendering primitives |
| `services/` | Rust API, Worker, Maintenance, and Flyway-owned schema history |
| `infra/` | Container, gateway, deployment, and object-storage configuration |
| `tests/` | Cross-application browser contracts and their support code |
| `docs/` | Source evidence and planning inputs; not generated runtime output |
| `deliverables/` | Versioned working deliverables, kept separate from source evidence |
| `reports/` | Dated QA and audit evidence intended for repository retention |
| `scripts/` | Generation, validation, boundary checks, and local maintenance tools |

Dependencies, build output, caches, browser reports, and temporary conversion
files are local generated artifacts and remain Git-ignored. `pnpm clean:local`
previews the cleanup whitelist; `pnpm clean:local:apply` explicitly removes it.
Neither command deletes dependencies, environment files, deliverables, or
arbitrary untracked files.

## Local development

Prerequisites:

- Node.js 22+
- pnpm 11+
- Rust 1.98 toolchain (pinned by `rust-toolchain.toml`)
- Docker with Compose for PostgreSQL, optional local MinIO object storage, and the
  pinned Flyway `13.4.0` migration image

The repository development toolchain remains Rust 1.98. The Platform crate's
declared MSRV is Rust 1.88 and CI checks it explicitly.

The repository includes a ready-to-run, Git-ignored `.env` and a tracked
`.env.example`. Keep machine-specific values in `.env`; never place production
credentials there. The main configuration groups are:

| Group | Variables |
| --- | --- |
| Host mappings | `AIRTEK_PUBLIC_HOST_PORT`, `AIRTEK_ADMIN_HOST_PORT`, `AIRTEK_API_HOST_PORT`, `AIRTEK_GATEWAY_HOST_PORT`, debug PostgreSQL port |
| Build network | `AIRTEK_NPM_REGISTRY`, `AIRTEK_CARGO_MIRROR`, `AIRTEK_BUILD_PROXY`; host `NO_PROXY` is forwarded to application image builds |
| Browser and security origins | `PUBLIC_HOST`, `ADMIN_HOST`, `API_HOST`, `AIRTEK_*_ORIGIN`, `VITE_*_BASE_URL`, `PUBLIC_API_BROWSER_ORIGIN` |
| Local persistence | `POSTGRES_*`, `AIRTEK_DATABASE_URL_INTERNAL`, host-side `DATABASE_URL`; `COMPOSE_PROFILES=minio` enables bundled MinIO |
| Schema migration | `AIRTEK_FLYWAY_BASE_IMAGE`, JDBC `FLYWAY_URL`, `FLYWAY_USER`/`FLYWAY_PASSWORD`, and `FLYWAY_PLACEHOLDERS_RUNTIME_ROLE` |
| Authentication, private staging and network trust | setup-only `AIRTEK_ADMIN_BOOTSTRAP_TOKEN`, independent TOTP/Product Staging/Analytics HMAC/invitation replay keys, gateway subnet/address, exact trusted-proxy CIDRs |
| Data lifecycle | `AIRTEK_GUEST_RAW_RETENTION_DAYS`, `AIRTEK_GUEST_AGGREGATE_RETENTION_MONTHS`, `AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION` |
| Analytics vocabulary | `AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES`, `AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS`, `AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS` register the only UTM identifiers the API may store; unknown free text is rejected |
| Direct media | Endpoint, bucket, credentials, key prefix and public base URL are database settings managed through Admin; uploads retain the fixed 25 MiB limit |
| Development behavior | `VITE_ENABLE_DEVTOOLS`, cookie-banner test switch, `RUST_LOG` |
| Reserved external adapters | Feishu and GA4 placeholders; blank values do not enable an adapter |

Application image builds use the official npm and crates.io sources by default.
`AIRTEK_NPM_REGISTRY` optionally selects the Web/Admin npm registry;
`AIRTEK_CARGO_MIRROR` optionally installs a Cargo `replace-with` registry index
for the Platform image; and `AIRTEK_BUILD_PROXY` is forwarded as both
`HTTP_PROXY` and `HTTPS_PROXY`. Leave the mirror and proxy empty to preserve the
normal CI build path. A Cargo mirror value must be a registry index URL, for
example `sparse+https://rsproxy.cn/index/`, rather than a crate download URL.

Internal application ports remain the fixed architecture contract. Host-side
port variables only change loopback diagnostics and avoid collisions; if the
gateway host port changes, update the browser-visible origin values together.
Local PostgreSQL credentials, the two application connection URLs, and the
Flyway JDBC settings must stay in sync. Production must instead give Flyway a
dedicated DDL role, pre-provision the named API/Worker runtime role, and keep
their credentials separate. The runtime role must have `LOGIN`; Flyway grants
it `CONNECT`, the required DML, and Worker `TEMPORARY` access after each
successful migration while keeping schema history read-only.
The production runtime role must not own the database, schema, tables, or
functions and must not retain `CREATE` on `public`.
Generate four independent random 32-byte Base64 keys for TOTP, Product Master
private staging, analytics-token HMAC, and encrypted
invitation idempotency replay before retaining real local data. Invitation
tokens remain hashed in their source table; the short-lived replay response is
stored only as an AES-256-GCM envelope so a retry can return the original token.

The complete local stack builds all three applications plus the independent,
non-root migration image, applies migrations through the one-shot
`flyway-migrate` container, then starts the API and Worker only after that
container exits successfully. On an empty local database, the development-only
maintenance command also creates this fixed Super Admin before the API starts:

- Email: `local-admin@airtek.invalid`
- Password: `Airtek-Local-Admin-20260917!`

If any user already exists, startup preserves every account and skips the
development seed. To explicitly create or restore only the fixed local account,
including clearing its TOTP and revoking its sessions, run
`pnpm dev:admin:reset`. The credentials live only in local Compose; production
artifacts and deployment configuration reject them.

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
be reached from a host PostgreSQL instance. Bundled MinIO starts only when
`COMPOSE_PROFILES=minio`; its fixed development ports bind to loopback.
For native source development, opt in to the loopback-only database mapping,
install dependencies, and start PostgreSQL first. The
Rust binaries load the root `.env` through `dotenvy` when they are launched
from the repository root. Run the migration, API, worker, and frontend commands
in separate terminals:

```sh
pnpm install
docker compose -f compose.yaml -f compose.debug.yaml up -d postgres
pnpm db:migrate
cargo run --manifest-path services/platform/Cargo.toml --bin airtek-api
cargo run --manifest-path services/platform/Cargo.toml --bin airtek-worker
pnpm dev
```

The debug mapping defaults to host port `54320`, so the root `.env.example`
uses `postgres://airtek:airtek@localhost:54320/airtek`. Override
`AIRTEK_POSTGRES_DEBUG_PORT` if that port is occupied. Profiled MinIO uses fixed
loopback ports `19000` and `19001`; those values are intentionally not `.env` settings.

Use the individual frontend commands when working on one application:

```sh
pnpm dev:web
pnpm dev:admin
```

To expose PostgreSQL on loopback for native development, add the debug Compose
overlay. MinIO exposure is controlled independently by its profile:

```sh
docker compose -f compose.yaml -f compose.debug.yaml up --build
```

Object storage starts unconfigured in PostgreSQL, so the platform remains
available while uploads fail closed. A `settings.manage` administrator enters
and tests every S3-compatible value under System Settings → Object Storage;
the Secret is stored in the database by explicit owner decision but never
returned by the API or copied into audit JSON. Each successful PNG, JPEG, or
WebP upload stores its complete external public URL, and later setting changes
affect only new uploads. The compatibility API media route redirects to that
immutable URL.

The Admin SPA always uses the Rust API. Runtime mock records and local mock
authentication are not supported; deterministic UI fixtures live only inside
tests.

Developer mode is also explicit opt-in. Set `VITE_ENABLE_DEVTOOLS=true`, start
the API with Cargo's `devtools` feature, and sign in as a database-backed user
with `devtools.shell`; leaving the variable absent or `false` keeps the Admin
route and terminal package out of the module graph. Production builds reject a
true value instead of silently packaging the terminal.

The running API and Worker require the PostgreSQL `DATABASE_URL`; SQLx uses it
only for runtime queries and transactions. Flyway uses the separate JDBC
`FLYWAY_URL`, `FLYWAY_USER`, and `FLYWAY_PASSWORD` settings.
`FLYWAY_PLACEHOLDERS_RUNTIME_ROLE` names the existing PostgreSQL login used by
`DATABASE_URL`; the Flyway entrypoint accepts only a bounded lowercase
identifier before its `afterMigrate` callback grants connection and DML access.
The supported local schema commands are:

```sh
pnpm db:migrate
pnpm db:info
pnpm db:validate
```

Compose runs `airtek-maintenance prepare-runtime` once after Flyway and before
the API/Worker. For a non-Compose deployment, run the same command from the
Platform image after `flyway migrate`; API and Worker startup fails closed when
its versioned preparation marker is absent.

The checked-in local Compose stack explicitly permits its historical single
`airtek` owner role for development-volume compatibility. That shared-role
override is not present in `compose.production.yaml`; production rejects using
the Flyway DDL identity as the application runtime identity.

Application-owned migration and operations CLIs have been removed. In-memory repositories exist only behind isolated test
construction and are not a supported server mode. Runtime media storage is
selected only by the database-owned object-storage setting; there is no
environment or implicit local fallback when it is unconfigured.

### One-time adoption of an existing SQLx v1-10 database

Do not run a normal migration first against a database already managed through
SQLx versions 1 through 10. Confirm the exact environment, database identity,
credentials, and maintenance window, then run this controlled adoption sequence:

```sh
docker compose run --rm flyway-migrate baseline
pnpm db:migrate
pnpm db:validate
```

The `beforeBaseline` callback refuses the baseline unless the old
`_sqlx_migrations` table contains exactly successful versions 1 through 10 with
the reviewed checksums. Flyway then establishes its
`baselineVersion=10` baseline, so the normal migrate does not replay V1-V10 and
applies only later versions. The migration role must own the legacy objects or
hold their grant option so
`afterMigrate` can grant the declared runtime role. If any check fails, stop and
investigate; never bypass the callback.

This is a one-time takeover procedure only. `baselineOnMigrate` must remain
disabled so adoption always requires the explicit, guarded baseline command. A
new empty database must not be baselined; initialize it only with
`pnpm db:migrate`.

## Production image boundary

`compose.production.yaml` is a provider-neutral, image-only deployment
boundary. It requires five separate immutable references for Public Web, Admin
Web, Platform, Migrations, and Gateway. The non-root Flyway migration artifact
is independent from the Rust Platform artifact. Only the Gateway is exposed to
the outer TLS ingress; ports `3000`, `3100`, and `8080` stay internal. This
permits Public and Admin to be promoted or rolled back independently. Start from
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
pnpm check:workspace-hygiene
pnpm test:source-lines
pnpm check:source-lines
pnpm lint
pnpm typecheck
pnpm test
pnpm test:frontend
pnpm test:rust
pnpm test:all
pnpm db:validate
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

The historical `pnpm test` command remains the frontend workspace test entry
point. Use `pnpm test:all` when a local check must also run the repository-tool
tests and Rust Platform tests. The workspace hygiene check rejects copy-conflict
filenames such as `example 2.ts` when `example.ts` exists beside them; it does
not reject legitimate numbered names without a canonical counterpart.

`check:source-lines` audits application code, tests, executable scripts, SQL,
and generated source across the repository. Ordinary source files may contain
at most 500 logical lines. Documentation, lock files, data files, binary assets,
and generated build/test output are excluded. The already-applied V1 and V5
Flyway migrations are the only historical exceptions: their expected line
counts and SHA-256 checksums are pinned, so editing, extending, or removing
either migration fails the check. `test:source-lines` exercises the auditor,
and CI runs both commands before the build. `check:production` also invokes the
source-line audit before building production artifacts.

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
public manifests, webmaster verification files, or sitemaps. The independent
Admin UI boundary checks the editor entry and lazy JavaScript chunks against
their gzip budgets; repository source length is enforced by
`check:source-lines`, not by the bundle checker. Deployment checks also assert
the Flyway migration dependency, independent non-root migration image, gateway
Host boundary, and production DevTools route exclusion. Production Rust builds
use the `production` feature without `devtools`; the mutually enabled
combination must fail to compile.

## Brand asset evidence workflow

The legacy public website may be searched for missing company-brand asset
candidates, but it is a mutable discovery source and does not approve an asset
for production. The capture script scans its fixed company/brand page scope,
downloads eligible AIRTEKPOWER/LDY CDN files into an isolated Git-ignored
staging directory, records source-page context, and deduplicates files by
SHA-256:

```sh
node scripts/scrape_airtek_brand_assets.mjs [staging-directory]
```

After every unique candidate has been visually reviewed, record a decision for
each checksum ID in `scripts/airtek-brand-asset-decisions.json`, then finalize
the same staging directory:

```sh
node scripts/finalize_airtek_brand_assets.mjs [staging-directory]
```

Finalization fails when decision coverage is incomplete or a downloaded file no
longer matches its recorded hash. It writes `brand-manifest.json`, a review CSV,
and a staging README, and copies only retained candidates into
`review-required/` or `hold/`; excluded source candidates are not promoted into
those retained sets. Image dimensions use ImageMagick `identify` when it is
available and otherwise remain unknown. No generated review package, legacy
logo crop, favicon, facility image, video, or certificate is approved for CMS
upload merely because the workflow completed. Follow the owner, rights,
currency, identity, integrity, and media-safety gates in
[brand asset governance](./.agents/skills/airtek-brand/references/brand-assets.md).

## Database-driven content and Product Master

CMS V2 drafts, immutable content revisions, public routes, products, guest
attribution, users and roles are durable PostgreSQL records. News, General
Information, navigation and footer are CMS V2 content kinds rather than
parallel write services. Business/editorial copy is never read from a
production frontend fallback. Missing optional data is omitted; a missing
public record returns 404 and an unavailable required projection fails closed
rather than fabricating content.

An owner-approved Product Master, when supplied, is imported through the Admin
Product Master screen or its API. Each CSV import is keyed by deployment
environment, SHA-256 and mapping version, rejects malformed rows without
creating products, encrypts commercial price columns in private staging, and
records unresolved filename-only assets. Product facts and portal-owned
localized presentation use independent immutable revision histories;
publishing binds one presentation revision to the exact accepted Product Master
revision. This media/CMS delivery does not itself establish that a current
owner-approved Product Master exists. Noise remains excluded from the public
projection until its measurement setup is supplied.

### CMS V2 public contract

The published public CMS boundary is a hard V2 cutover. Public content is
decoded from the single current row in `cms_published_content`; `public_routes`
owns routable lookup and discovery. Responses carry `PublicContentProjection.schemaVersion = 2`,
including typed `typeFields`, `body`, `composition`, SEO, the publication version,
and server-resolved relations, content links, and current `resolvedMedia`.
Route resolution, site
bootstrap, News, generic content and discovery do not fall back to legacy
`payload` columns or the `published_news` / `published_general_information`
views.

`GET /api/admin/v1/content-drafts/templates` exposes each template's
`routePattern`. Editor undo, redo, dirty state, and preview live only in browser
memory. An explicit save writes one current private draft. Approval locks that
draft and the current publication, validates canonical routes and dependencies,
overwrites the publication row, replaces current dependencies, emits route and
outbox metadata, and deletes the private draft in one PostgreSQL transaction.
Stale base publication versions return 409 and put the draft back into editing.

The main CMS and media endpoints are:

| Boundary | Routes |
| --- | --- |
| Admin CMS (`/api/admin/v1`) | `/content-drafts`; `/content-reviews`; `/published-content`; explicit draft save, submit, withdraw, approve, reject, sharing, and copy-to-private-draft operations |
| Public CMS (`/api/public/v1`) | `GET /content/{kind}/{slug}`; `/routes/resolve`; `/site-bootstrap`; `/news`; `/news/{slug}`; `/discovery` |
| Admin media (`/api/admin/v1`) | `GET/POST /media/assets`; `GET /media/assets/{id}`; `GET /media/assets/{id}/references` |
| Public media | `GET /api/public/v1/media/{assetId}` and `/download`, mirrored below the public Host `/media/*` |

The former dedicated Admin News and General Information mutation APIs are
removed; all supported kinds use the private-draft lifecycle. The Web
application consumes the same V2 contract and renders the ten registered block
kinds through `PublicBlockRenderer`, including FAQ, Contact, relations, media
and downloads.

### Direct media uploads

`POST /api/admin/v1/media/assets` requires `media.write`, an
`Idempotency-Key`, and exactly one multipart `file` part. The request streams
into a restricted temporary file, accepts at most 25 MiB, identifies PNG, JPEG,
or WebP from file headers, sanitizes the original file name, and computes
SHA-256. It stores one immutable object and commits the catalogue row, audit
record, and replay response in one database transaction. A database failure
triggers exact-object deletion; a storage failure creates no catalogue row.

Success returns `201 MediaAsset`. Replaying the same user, key, and bytes
returns the original response without another object; reusing that key for
different bytes returns stable `409 media_idempotency_conflict`.

Every non-deleted asset has an immutable external `publicUrl` captured at
upload. `/api/public/v1/media/{assetId}` and its `/download` variant remain as
compatibility redirects. Content and product publication only determines
whether a website projection contains the link. Soft-deleted historical assets
return 404, and this release exposes no new delete operation.

The API identity can put and compensate-delete objects under the configured
media prefix; the configured public base URL must provide anonymous reads.
The ordinary Worker continues non-media jobs and has no media credentials,
heartbeat, or media job types.
Historical review-related database columns from early migrations are inert
compatibility columns and are not part of runtime models, OpenAPI, or Admin UI.

Production connects only the API to an independently operated S3-compatible
service using settings saved through Admin. The browser receives no object-store
credentials and reads the immutable public URL recorded for the asset. This
repository does not provide a CDN or third-party malware scanner.

## Data and publication rules

- A supplied, owner-approved Product Master CSV is authoritative only for its
  registered environment, checksum, and mapping version. Future Feishu changes
  still pass through staging, three-way diff, validation, and an explicit
  publish action.
- The HTML demos under `docs/Plan & Solution/` are interaction references only. Their product specifications, curves, downloads, scores, and colors are not production data.
- The public application reads only published projections. Drafts, source snapshots, conflicts, RFQ records, users, and audit data are admin-only.
- Public RFQs never accept file uploads.
- Placeholder pages are `noindex` and omitted from public sitemaps.
- Development fixture ownership is one-way: clearing `isPlaceholder` in Admin
  atomically takes the record into editorial ownership. Later development seed
  runs retain their ledger entry but skip that record without overwriting it.
- Every CMS V2 kind, including News and General Information, uses the unified
  content draft/snapshot/revision transaction. Publishing a routable kind writes
  its canonical `public_routes` row in that same transaction.
- A media or download block can publish only when its referenced asset exists
  and has not been soft-deleted.
- `docs/` is preserved as source evidence and is not edited by the implementation.

## Current integration status

- Exact products may be published only from an accepted, audited Product Master
  snapshot. This repository state does not prove that an owner-approved current
  snapshot has been supplied. Missing curves, certifications, downloads, and
  case outcomes remain omitted until their own controlled evidence exists.
- Public SSR serves `robots.txt` and every sitemap document dynamically. The
  Rust discovery feed contributes only published, canonical, indexable,
  non-placeholder content and products; API failure is handled fail-closed.
  Static files with those route names are intentionally excluded so they cannot
  bypass the runtime publication filter.
- The management portal provides real News, General Information, Product Master
  import/presentation, direct media upload, guest-source analytics and
  identity-management workflows. News and General Information now use the
  unified CMS V2 editor rather than dedicated legacy APIs.
- One current publication row and durable outbox hand-off exist. Runtime sitemaps read
  the published projection directly, while CDN invalidation and external search
  indexing adapters are not wired; no external provider refresh should be
  inferred from a completed internal hook.
- The API owns the direct S3-compatible upload and delete-compensation path,
  while browsers read the immutable external URL. Production still requires a
  reviewed HTTPS object-store endpoint and public origin, least-privilege API
  credentials, monitoring, off-host backup and an end-to-end smoke test; none
  is implied by checked-in configuration. Machine scanning is not part of the
  media workflow.
- Feishu and GA4 values are still reserved configuration only; no live Feishu
  synchronization or GA4 loading is implied.
- Consent-gated first-party analytics events are sanitized in the public client,
  accepted by the Rust API, and exposed through the admin aggregate. A GA4
  provider adapter is not connected, so no GA4 loading is implied.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for service and security boundaries.
