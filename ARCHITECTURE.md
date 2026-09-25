# Platform architecture

## Process boundary

```text
Public browser ─┐   HTTP gateway :8088 (Host routing) ─> Public SSR :3000 ─┐
Admin browser  ─┼─────────────────────────────────────> Admin SPA  :3100   ├─> API :8080 ─> PostgreSQL
API clients    ─┘                                      API Host             ┘       └──────> S3-compatible object storage
Public media ───────────────────── immutable external URL ─────────────────────────> public object origin / CDN

Worker ────────────────────────────────────────────────────────────────────> PostgreSQL

Public SSR ───────────────── internal service network ────────────────────> API :8080

Flyway 13.4.0 migration job ─────────── schema DDL ───────────────────────> PostgreSQL
```

The public SSR process and both browsers have no database or object-storage
credentials. The Rust platform loads S3-compatible connection settings from
PostgreSQL only. The API uses that identity to write and compensate failed
uploads; browsers read the public origin stored with each media row. Storage is
disabled until a settings administrator saves a successfully tested complete
configuration, and there is no environment or local-disk fallback.
Public and admin builds do not share a router, cache namespace, service worker,
manifest, or crawl-control files.

PostgreSQL remains the sole runtime database. SQLx is confined to runtime data
access inside the Rust API and Worker; it does not own schema history. Flyway
`13.4.0` is the exclusive schema-version authority.

The checked-in gateway is an executable HTTP integration boundary. It rejects
unknown Hosts, routes the three configured Hosts to separate upstreams, and has
no production DevTools/WebSocket upstream. Production TLS terminates at an
outer ingress or hosting provider rather than inside this Compose stack.

Public SSR, Admin Nginx, Platform, Migrations, and Gateway production images
run as non-root users. The Flyway migration image is an independent release
artifact rather than part of the Rust Platform image. The local Compose
diagnostic ports bind to loopback by default.
PostgreSQL is never host-published by the base stack. MinIO is an optional
`minio` Compose profile; when enabled, its API and console use fixed loopback
ports and its development bucket permits anonymous object reads.
The image-only `compose.production.yaml` publishes only the Gateway listener;
Public `3000`, Admin `3100`, and API `8080` remain internal. Its five immutable
image references are independent release and rollback units.

The Web/Admin Dockerfiles accept an optional `NPM_CONFIG_REGISTRY`; the Platform
Dockerfile accepts an optional `CARGO_REGISTRY_MIRROR` and writes Cargo's
`replace-with` configuration only when it is non-empty. All three accept
`HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY`. Local Compose maps these from
`AIRTEK_NPM_REGISTRY`, `AIRTEK_CARGO_MIRROR`, `AIRTEK_BUILD_PROXY` and the host
`NO_PROXY`; empty mirror/proxy settings preserve the official-source CI build
path and do not alter runtime networking.

## Rendering

- Public content, product, solution, technology, resource, company, contact, and RFQ-entry routes render meaningful HTML on the server.
- Filters, compare tray, PQ exploration, FAQ controls, and multi-step RFQ forms hydrate in the browser.
- Selector content is server-rendered while its calculation workflow is
  client-led. Comparison is a client-rendered route with server configuration
  for title/description and `noindex,follow`; it is not an SSR content page.
- CMS pages consume `PublicContentProjection` schema version 2 and render all
  ten registered composition blocks through the shared `PublicBlockRenderer`.
  FAQ and Contact behavior comes from their typed blocks rather than a legacy
  page payload.
- The management portal is an independent SPA.

## Publication boundary

CMS content has exactly two persisted document states: an owner's current row in
`cms_drafts` and the company's current row in `cms_published_content`. Editor
undo, redo, dirty state, and local preview exist only in browser memory. Saving
is explicit. Editing published content starts by copying the current publication
to a new private draft; successful review republishes by overwriting the same
`content_id` and then deletes that draft, its shares, and its queue record.
Neither draft nor publication documents have recoverable database history.

The Rust template registry exposes `routePattern` through
`GET /api/admin/v1/content-drafts/templates` and is the single canonical-path
source used by Admin and publication. A route owned by another entity returns
409. Approval runs in one PostgreSQL transaction: it locks the draft and current
publication, verifies `base_publication_version`, validates relations, media,
and canonical identity, overwrites the current publication and dependency rows,
updates route/outbox metadata, and deletes the private workflow rows. A stale
base returns 409 and restores the draft to `editing` without overwriting or
deleting it. Relation targets must already be published. Referenced media must
exist and not be soft-deleted; otherwise publication returns 422.

The published public CMS API is hard-cut to `PublicContentProjection` schema
version 2. Generic content, route resolution, site bootstrap, News, and discovery
read the current published V2 `document`; canonical lookup and discovery also
use `public_routes`. Public response `revision` values are supplied by
`publication_version`. They never read a private draft or fall back to a legacy
payload. Relations and content-link IDs are resolved to public cards/URLs in the
Rust service before the projection reaches Web. `resolvedMedia` binds each
document reference to the direct public asset and download URLs.

Product publishing retains its own validated Product Master revision boundary.
Publishing atomically changes its public projection and emits a durable outbox
event. Rollback republishes a prior revision rather than mutating history. The
public SSR sitemap handler reads the projection through the API discovery feed,
so it reflects publication without a frontend rebuild. The worker consumes the
internal hand-off, but provider-specific CDN invalidation and external search
indexing are not connected; an internal hook completion does not mean an
external provider was refreshed.

PostgreSQL is the only supported runtime repository for the API, Worker, and
stateful contract tests; there is no in-memory business repository.
Administrator password sessions are limited to TOTP setup
until enrollment is confirmed. Invitation source records contain only a token
hash; the 24-hour idempotency response is encrypted with its own deployment
key so a network retry can safely recover the original one-time token.

## CMS and media API boundary

The authenticated Admin surface under `/api/admin/v1` separates private drafts,
review queue, and current publications into `/content-drafts`,
`/content-reviews`, and `/published-content`. It has no revision, diff, restore,
snapshot, token-preview, or mixed content lifecycle API. Its media surface is
intentionally small: list, synchronous upload, detail, and content references.
Mutations retain authentication, CSRF, permission, optimistic concurrency, and
metadata-only audit boundaries.

The public surface under `/api/public/v1` provides published content,
route resolution, site bootstrap, News, discovery, and the unauthenticated
`/media/{assetId}` and `/media/{assetId}/download` routes. These compatibility
routes redirect to immutable public object URLs; clients never receive object
store credentials.

## Direct media boundary

Upload requires an actor-scoped `Idempotency-Key`, exactly one multipart
`file`, and no more than 25 MiB. The API accepts PNG, JPEG, and WebP by file
header, removes path/control characters from the original name, and computes
SHA-256 while staging. It writes one immutable object, then atomically commits
the catalogue row, audit event, and idempotency replay. If the database phase
fails, it deletes that exact object; if storage fails, it creates no row.

Every successful upload is immediately public at the absolute URL stored in the
catalogue row. Known URLs remain unchanged when object-storage settings change
and remain readable while their original origin and object exist. Publication
validation requires each referenced asset to exist and not be soft-deleted.
Legacy same-origin media routes return permanent redirects to the stored URL.
The object store controls the redirected response's filename and whether the
browser renders it inline or downloads it as an attachment.

The API identity has the least privileges needed for probe/upload and
DeleteObject compensation under the media prefix; anonymous reads use the
configured public origin. The ordinary Worker has no media credentials or
media jobs. `/healthz` and `/readyz` cover the
platform and database boundary; there is no media-specific service or heartbeat.
Historical media review columns remain inert for schema compatibility and are
not read by runtime code.

The selected single-host production topology operates PostgreSQL in the
independent `airtek-infra` project. Object storage is an external application
dependency configured through Admin and persisted in PostgreSQL, not production
Compose or environment configuration. Production readiness still requires a
reviewed public delivery origin, monitoring, off-host backup, restore validation,
and an endpoint smoke test.

The required product-master paths are:

```text
Owner-approved CSV -> checksum/idempotent staging -> schema/unit/reference validation
                   -> immutable source snapshot -> revision -> explicit publish

Selected Feishu tables -> complete per-table scan -> staging and validation
                       -> source attachments -> immutable fact revision -> automatic publish
                       -> successful missing-set reconciliation -> unpublish -> durable purge
```

The initial CSV snapshot is authoritative only for its registered environment,
checksum and mapping version. Product facts and portal-owned localized
presentation advance on independent immutable revision clocks; a publish binds
the selected presentation to an exact fact revision. Private commercial columns
remain encrypted outside public product revisions. The Feishu connector reads
only enabled GUI-selected sources and never writes to Feishu. Its App ID and
write-only App Secret are stored in PostgreSQL; the tenant token remains
memory-only. A successful connection test binds field IDs to a versioned
mapping, and later ID/type drift fails only that table instead of guessing.

Every Feishu run freezes the enabled source list, mapping version, and settings
revision, then performs a full scan. Valid facts automatically publish while the
last published CMS presentation revision is copied forward; an unpublished CMS
working draft is never promoted. Record failures retain the prior public fact
revision. Only a completely fetched and schema-valid table may reconcile
missing records. There is no Feishu approval-conflict or rollback workflow.

Source images receive WebP previews. Valid original images, documents, sheets,
and supported CAD files remain downloadable; non-images have no preview route
and download with `attachment` plus `nosniff`. SHA-256 deduplication and durable
object compensation prevent one source purge from deleting a shared asset.

## Search-engine isolation

- Public `robots.txt` references only the public sitemap index.
- Public sitemap documents contain only canonical, indexable, published public URLs.
- Admin `robots.txt` disallows all crawlers and admin sitemap paths return 404 before SPA fallback.
- Admin responses include `X-Robots-Tag: noindex, nofollow, noarchive`.
- API `robots.txt` disallows all crawlers and API sitemap paths return 404.
- Public `/admin` and admin `/en` routes return 404 at their own server boundaries.
- The gateway repeats the public/admin/API isolation rules before proxying and
  rejects unmatched Host values instead of falling through to an application.

Admin HTML, assets, expected errors, and fallback responses keep the same
`X-Robots-Tag` and security-header set. Public manifests, Vite manifests,
webmaster verification files, public locale routes, and every sitemap variant
return 404 on the admin origin before SPA fallback.

## Startup and migrations

`flyway-migrate` is an independent, non-root, one-shot Flyway `13.4.0` image.
It receives a JDBC URL and dedicated migration credentials through
`FLYWAY_URL`, `FLYWAY_USER`, and `FLYWAY_PASSWORD`. Production grants that role
the required DDL privileges while the API/Worker `DATABASE_URL` uses a separate
pre-provisioned runtime role named by `FLYWAY_PLACEHOLDERS_RUNTIME_ROLE`. The
validated `afterMigrate` callback grants current and future application objects
to that login role for `CONNECT`, DML, and the Worker's required database
`TEMPORARY` capability, but makes both Flyway and legacy SQLx history read-only.
It also revokes the legacy `PUBLIC` schema-create grant and fails if a distinct
runtime role owns the database/schema objects or lacks the required privileges.
Compose waits for PostgreSQL health, requires `flyway-migrate` to exit
successfully, then runs the idempotent `airtek-maintenance prepare-runtime`
data-preparation job. The API and Worker start only after that job records the
versioned preparation markers; their own startup performs bounded readiness
checks and never runs migrations or data backfills.

The supported operator entry points are `pnpm db:migrate`, `pnpm db:info`, and
`pnpm db:validate`. Application binaries do not expose schema or operational
commands. Operational data preparation is exposed only by the dedicated
maintenance binary. SQLx performs runtime queries and transactions only.

An existing database with the exact legacy SQLx v1-10 history requires a
controlled, one-time takeover. Confirm the target environment and database,
then run:

```sh
docker compose run --rm flyway-migrate baseline
pnpm db:migrate
pnpm db:validate
```

The `beforeBaseline` callback verifies the complete legacy version and checksum
set before Flyway establishes its `baselineVersion=10` baseline. The normal
migrate therefore does not replay V1-V10 and applies only later versions.
`baselineOnMigrate` stays disabled so takeover cannot happen implicitly. Empty
databases use only `pnpm db:migrate`.

## Repository quality boundaries

Repository source structure and production bundle shape are separate enforced
contracts. `scripts/assert-source-line-limits.mjs` audits source-code extensions
across the checkout, including tests, executable scripts, SQL, and generated
source, while excluding documentation, data, lock files, dependencies, and
build/test output. Ordinary source files are limited to 500 logical lines. The
already-applied V1 and V5 Flyway migrations are immutable historical
exceptions whose line counts and SHA-256 checksums are pinned; any edit,
extension, or removal is a violation. CI first tests the auditor and then runs
it, and the production check repeats the audit before building artifacts.

`scripts/assert-admin-ui-boundary.mjs` has the narrower post-build concern: it
inspects the Admin production JavaScript output and enforces gzip budgets for
the editor entry and lazy chunks. It does not scan source length. Keeping these
checks independent prevents a bundle assertion from silently defining source
governance and makes each failure identify the boundary that was violated.

Repository cleanup and source reorganization are separate concerns. Generated
artifacts are removed only through the explicit local-cleanup whitelist, while
`check:workspace-hygiene` rejects accidental numbered copies in source and
document directories. It never deletes files.

Admin source moves are incremental and domain-scoped: content first,
catalog/media second, then identity/analytics/settings. A migration keeps each
domain's views, components, services, stores, and tests together while shared
transport, authentication, pagination, and base UI remain common. Each change
preserves routes, request contracts, state behavior, and the `@` source alias.
The Rust Platform remains one crate with Route → Service → Storage boundaries;
growing modules continue to use the existing `module.rs` plus `module/` layout
rather than introducing a parallel architecture.

## Developer tools

Development builds may include the browser terminal UI and PTY API. The PTY
opens the host `$SHELL` and inherits the non-root OS identity that started the service;
application users and `devtools.shell` authorization remain database identities
and are not mapped to OS accounts. The Admin UI uses an xterm-compatible terminal
only from its development-only route chunk. A hashed, 60-second, single-use token
is bound to the active Admin session before upgrade; the WebSocket protocol
supports ANSI byte output, input frames, resize, explicit start/end events and
exit codes. Four concurrent sessions, bounded pending tokens/input/dimensions,
a 64 MiB per-session output cap,
a 15-minute idle timeout and a two-hour absolute timeout limit the host surface.
Audit events store actor, session, timestamps, fixed reasons and command-frame
metadata while intentionally excluding command text and terminal output.

Production builds exclude DevTools UI chunks, the Rust `devtools` feature, API
route, WebSocket upstream, PTY dependencies, and every shell or operational
entrypoint. `production` and `devtools` are mutually exclusive build features.

## Adapter readiness

PostgreSQL persistence, Flyway-managed schema versions, internal jobs, durable
outbox claiming, and the one-way Feishu connector are platform capabilities.
S3-compatible direct media transport is implemented. MinIO is the local/E2E
reference service; production media and Feishu remain unverified until approved
endpoints, least-privilege application access, monitoring, and real connection
and synchronization smoke tests are complete. GA4 loading, CDN purge, external
search providers, email/CRM/webhooks, backup executors, and isolated restore
executors are not connected. Passing local tests or displaying an Admin screen
must not be treated as proof of an active external integration.
