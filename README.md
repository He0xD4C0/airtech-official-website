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

## Local development

Prerequisites:

- Node.js 22+
- pnpm 11+
- Rust 1.98 toolchain (pinned by `rust-toolchain.toml`)
- Docker with Compose for PostgreSQL and the pinned Flyway `13.4.0` migration
  image; optional local MinIO profile for S3-compatible media work

The repository includes a ready-to-run, Git-ignored `.env` and a tracked
`.env.example`. Keep machine-specific values in `.env`; never place production
credentials there. The main configuration groups are:

| Group | Variables |
| --- | --- |
| Host mappings | `AIRTEK_PUBLIC_HOST_PORT`, `AIRTEK_ADMIN_HOST_PORT`, `AIRTEK_API_HOST_PORT`, `AIRTEK_GATEWAY_HOST_PORT`, debug PostgreSQL/MinIO ports |
| Build network | `AIRTEK_NPM_REGISTRY`, `AIRTEK_CARGO_MIRROR`, `AIRTEK_BUILD_PROXY`; host `NO_PROXY` is forwarded to application image builds |
| Browser and security origins | `PUBLIC_HOST`, `ADMIN_HOST`, `API_HOST`, `AIRTEK_*_ORIGIN`, `VITE_*_BASE_URL`, `PUBLIC_API_BROWSER_ORIGIN` |
| Local persistence | `POSTGRES_*`, `AIRTEK_DATABASE_URL_INTERNAL`, host-side `DATABASE_URL`, `MINIO_ROOT_*` |
| Schema migration | `AIRTEK_FLYWAY_BASE_IMAGE`, JDBC `FLYWAY_URL`, `FLYWAY_USER`/`FLYWAY_PASSWORD`, and `FLYWAY_PLACEHOLDERS_RUNTIME_ROLE` |
| Authentication, private staging and network trust | setup-only `AIRTEK_ADMIN_BOOTSTRAP_TOKEN`, independent TOTP/preview/Product Staging/Analytics HMAC/invitation replay keys, gateway subnet/address, exact trusted-proxy CIDRs |
| Data lifecycle | `AIRTEK_GUEST_RAW_RETENTION_DAYS`, `AIRTEK_GUEST_AGGREGATE_RETENTION_MONTHS`, `AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION` |
| Analytics vocabulary | `AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES`, `AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS`, `AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS` register the only UTM identifiers the API may store; unknown free text is rejected |
| Media pipeline | `AIRTEK_MEDIA_STORAGE`, `AIRTEK_MEDIA_LOCAL_ROOT`, and the `AIRTEK_MEDIA_S3_*` endpoint/bucket/credential/prefix/path-style settings |
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
Generate five independent random 32-byte Base64 keys for TOTP, preview
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

The complete local stack builds all three applications plus the independent,
non-root migration image, applies migrations through the one-shot
`flyway-migrate` container, then starts the API and Worker only after that
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
be reached from a host PostgreSQL instance. The S3-compatible media adapter is
implemented, while its local MinIO service and storage selection are opt-in.
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
`AIRTEK_POSTGRES_DEBUG_PORT` if that port is occupied. MinIO diagnostics use
`19000` and `19001` by default and are configurable independently.

Use the individual frontend commands when working on one application:

```sh
pnpm dev:web
pnpm dev:admin
```

To exercise the S3-compatible adapter against the private local bucket, opt in
to both media storage and the MinIO profile. Including `compose.debug.yaml`
publishes the MinIO API and console on loopback for diagnostics:

```sh
AIRTEK_MEDIA_STORAGE=s3 docker compose -f compose.yaml -f compose.debug.yaml \
  --profile object-storage up --build
```

Leaving `AIRTEK_MEDIA_STORAGE` empty keeps uploads and delivery disabled while
the Admin catalogue remains readable. Selecting `s3` requires a complete
endpoint, bucket and credential set; when any required value is absent or
blank, media storage remains disabled and the application continues without a
fallback backend. Explicit malformed endpoints, prefixes and boolean values
still fail configuration. The MinIO profile creates a non-anonymous bucket and
is only a local development dependency.

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

The checked-in local Compose stack explicitly permits its historical single
`airtek` owner role for development-volume compatibility. That shared-role
override is not present in `compose.production.yaml`; production rejects using
the Flyway DDL identity as the application runtime identity.

The former Rust `airtek-migrate` binary has been removed, and `airtekctl` has no
`migrate` subcommand. In-memory repositories exist only behind isolated test
construction and are not a supported server mode. The local/S3-compatible media
storage implementations are selected explicitly; there is no implicit local
fallback when media storage is unconfigured.

### One-time adoption of an existing SQLx v1-10 database

Do not run a normal migration first against a database already managed through
SQLx versions 1 through 10. Back up that database, verify the restore path, and
confirm the exact environment, database identity, credentials, and maintenance
window. Then run this controlled adoption sequence:

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
pnpm lint
pnpm typecheck
pnpm test
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
also assert the Flyway migration dependency, independent non-root migration
image, gateway Host boundary, and production DevTools route exclusion.
Production Rust builds use the `production` feature without `devtools`; the
mutually enabled combination must fail to compile.

## Database-driven content and Product Master

CMS V2 drafts, immutable content revisions, public routes, products, guest
attribution, users and roles are durable PostgreSQL records. News, General
Information, navigation and footer are CMS V2 content kinds rather than
parallel write services. Business/editorial copy is never read from a
production frontend fallback. Missing optional data is omitted; a missing
public record returns 404 and an unavailable required projection fails closed
rather than fabricating content.

The initial approved Product Master is imported through the Admin Product
Master screen or its API. Each CSV import is keyed by deployment environment,
SHA-256 and mapping version, rejects malformed rows without creating products,
encrypts commercial price columns in private staging, and records unresolved
filename-only assets. Product facts and portal-owned localized presentation use
independent immutable revision histories; publishing binds one presentation
revision to the exact accepted Product Master revision. Noise remains excluded
from the public projection until its measurement setup is supplied.

### CMS V2 public contract

The published public CMS boundary is a hard V2 cutover. Public content is
decoded from `content_revisions.document` at the exact revision named by
`content_entries.cms_published_revision`; `public_routes` owns routable lookup
and discovery. Responses carry `PublicContentProjection.schemaVersion = 2`,
including typed `typeFields`, `body`, `composition`, SEO, the published revision,
and server-resolved relations and content links. Route resolution, site
bootstrap, News, generic content and discovery do not fall back to legacy
`payload` columns or the `published_news` / `published_general_information`
views. Those columns and views remain in the schema only for migration evidence
and operational rollback.

`GET /api/admin/v1/content/templates` exposes each template's `routePattern`.
The Rust template registry is the shared source for Admin canonical previews and
publication. A publish transaction validates relations and referenced media,
then upserts `public_routes` for routable templates; a path already owned by
another entity returns 409, while unpublished relation targets and invalid media
references return 422. Non-routable navigation, footer and General Information
documents do not receive routes. A route is indexable only when the document's
SEO allows it and it is neither a placeholder nor non-routable.

The main CMS and media endpoints are:

| Boundary | Routes |
| --- | --- |
| Admin CMS (`/api/admin/v1`) | `GET/POST /content`; `GET /content/templates`; `GET/PATCH /content/{id}/draft`; `POST /content/{id}/snapshots`; `GET /content/{id}/revisions`; `GET /content/{id}/diff`; `POST /content/{id}/revisions/{revision}/restore` |
| Public CMS (`/api/public/v1`) | `GET /content/{kind}/{slug}`; `/routes/resolve`; `/site-bootstrap`; `/news`; `/news/{slug}`; `/discovery` |
| Admin media (`/api/admin/v1`) | `GET /media/assets`; `POST /media/uploads`; `POST /media/assets/{id}/scan` |
| Public media | `GET /api/public/v1/media/{assetId}` and `/media/{assetId}/download`; the public Host exposes the corresponding routes at `/media/{assetId}` and `/media/{assetId}/download` |

The former dedicated Admin News and General Information mutation APIs are
removed; all supported kinds use the unified `/content` lifecycle. The Web
application consumes the same V2 contract and renders the ten registered block
kinds through `PublicBlockRenderer`, including FAQ, Contact, relations, media
and downloads.

### Media pipeline

`POST /api/admin/v1/media/uploads` requires `media.write` and one multipart
`file` part. The platform accepts at most 25 MiB and verifies the byte signature
as PNG, JPEG or WebP; SVG is not accepted. Every successful upload creates a
`pending` catalogue row that is not publicly readable. A reviewer with
`media.write` explicitly changes it to `clean` or `quarantined` through the
legacy-named `/scan` compatibility endpoint, and every decision requires a
non-empty reason. This flow has no machine scanner or automatic clean mode.

Public delivery returns only live assets whose database state is both `clean`
and `public`. The gateway proxies `/media/*` through the Rust API on the public
origin, so the storage endpoint and bucket remain private. Responses use a
strong ETag, immutable one-year caching and `nosniff`; the download route forces
`Content-Disposition: attachment`. Missing, quarantined, non-public, deleted,
unconfigured or backend-mismatched objects fail closed. The repository includes
local-file and S3-compatible storage implementations plus the opt-in MinIO
profile, but it does not provide a CDN, public bucket, third-party scanner, or a
configured production object store. Public delivery streams local files
asynchronously and forwards S3 response bytes through a bounded channel; the
gateway disables proxy response buffering for `/media/*`, so neither layer
collects a complete object before sending it to the client. The checked-in
production Compose boundary leaves media storage disabled until an approved
deployment supplies and reviews the complete settings.

## Data and publication rules

- The owner-approved Product Master CSV is the authoritative initial snapshot. Future Feishu changes still pass through staging, three-way diff, validation and an explicit publish action.
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
- A media or download block can publish only when its immutable asset/version
  reference resolves to a live `clean + public` catalogue record.
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
  import/presentation, media upload/review, guest-source analytics and
  identity-management workflows. News and General Information now use the
  unified CMS V2 editor rather than dedicated legacy APIs.
- Durable publication revisions and outbox hand-off exist. Runtime sitemaps read
  the published projection directly, while CDN invalidation and external search
  indexing adapters are not wired; no external provider refresh should be
  inferred from a completed internal hook.
- The S3-compatible adapter and local MinIO profile are implemented and remain
  opt-in. Feishu and GA4 values are still reserved configuration only; no live
  Feishu synchronization or GA4 loading is implied. The standard production
  deployment does not configure an object store; machine scanning is not part
  of the media workflow.
- Consent-gated first-party analytics events are sanitized in the public client,
  accepted by the Rust API, and exposed through the admin aggregate. A GA4
  provider adapter is not connected, so no GA4 loading is implied.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for service and security boundaries.
