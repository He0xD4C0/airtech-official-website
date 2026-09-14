# Platform architecture

## Process boundary

```text
Public browser ─┐   HTTP gateway :8088 (Host routing) ─> Public SSR :3000 ─┐
Admin browser  ─┼─────────────────────────────────────> Admin SPA  :3100   ├─> API :8080 ─> PostgreSQL
API clients    ─┘                                      API Host             ┘       └──────> private object storage
Public /media/* ───────── same-origin API proxy ───────────────────────────┘

Worker ────────────────────────────────────────────────────────────────────> PostgreSQL

Public SSR ───────────────── internal service network ────────────────────> API :8080

Flyway 13.4.0 migration job ─────────── schema DDL ───────────────────────> PostgreSQL
```

The public SSR process and both browsers have no database or object-storage
credentials. The Rust platform contains local-file and S3-compatible adapters.
Only the API receives a least-privilege media identity, limited to reading,
writing, and compensating failed uploads under its media prefix. Storage is
disabled when no backend is selected or an S3 selection
lacks a required endpoint, bucket, or credential. Malformed explicit values
still fail configuration, and an incomplete selection never falls back to local
disk.
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
PostgreSQL is never host-published by the base stack. MinIO is part of the local
stack for the S3-compatible media adapter; its API and console loopback
mappings live only in `compose.debug.yaml`, and its bucket remains private.
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

CMS V2 content and products use mutable working drafts and immutable revisions.
For CMS content, `content_entries.cms_published_revision` selects exactly one
`content_revisions.document`; publication also upserts `public_routes` for a
routable template in the same transaction. The Rust template registry exposes
`routePattern` through `GET /api/admin/v1/content/templates` and is the single
canonical-path source used by both Admin preview and publication. A route owned
by another entity returns 409. CMS publication runs as a retry-bounded
`SERIALIZABLE` transaction: it locks the source and referenced targets in stable
order, validates relations plus referenced media existence, and only
then writes the revision, complete dependency snapshot, pointer, route, audit,
idempotency result, and outbox event. Relation targets must already be
published. A media reference must exist and not be soft-deleted; otherwise
publication returns 422 without leaving a new revision or route. Exhausted
serialization retries return 409.

The published public CMS API is hard-cut to `PublicContentProjection` schema
version 2. Generic content, route resolution, site bootstrap, News and discovery
read the published V2 `document`; canonical lookup and discovery additionally
use `public_routes`. They never fall back to a legacy `payload`. Legacy columns
and the `published_news` and `published_general_information` views remain in the
schema for migration evidence and rollback only. The former dedicated Admin
News and General Information mutation routes are removed; every CMS V2 kind
uses the unified `/api/admin/v1/content` lifecycle. Relations and content-link
IDs are resolved to public cards/URLs in the Rust service before the projection
reaches Web. `resolvedMedia` binds each document reference to the direct public
asset and download URLs.

Unpublishing is explicit and separate from editing or archiving.
`POST /api/admin/v1/content/{id}/unpublish` requires `content.publish`, the
current draft ETag, an idempotency key, the expected published revision, and a
reason. A currently published dependant blocks the operation with 409 and no
automatic cascade. Success removes the public route and published pointer in
the same transaction, returns the entry to draft, and preserves draft content,
revision history, and dependency evidence. Any future archive operation must
require an already-unpublished entry rather than silently performing this
transition.

Product publishing retains its own validated Product Master revision boundary.
Publishing atomically changes its public projection and emits a durable outbox
event. Rollback republishes a prior revision rather than mutating history. The
public SSR sitemap handler reads the projection through the API discovery feed,
so it reflects publication without a frontend rebuild. The worker consumes the
internal hand-off, but provider-specific CDN invalidation and external search
indexing are not connected; an internal hook completion does not mean an
external provider was refreshed.

PostgreSQL is the only supported runtime repository for the API. The in-memory
adapter is retained solely for isolated unit tests and cannot be selected by
the API executable. Administrator password sessions are limited to TOTP setup
until enrollment is confirmed. Invitation source records contain only a token
hash; the 24-hour idempotency response is encrypted with its own deployment
key so a network retry can safely recover the original one-time token.

## CMS and media API boundary

The authenticated Admin surface under `/api/admin/v1` provides unified
content drafts, explicit publication lifecycle operations, revisions and
diffs. Its media surface is intentionally small: list, synchronous upload,
detail, and content references. Mutations retain authentication, CSRF,
permission, idempotency, optimistic-concurrency where an entity is revised,
and audit boundaries.

The public surface under `/api/public/v1` provides published content,
route resolution, site bootstrap, News, discovery, and the unauthenticated
`/media/{assetId}` and `/media/{assetId}/download` routes. Clients never
receive an object-store URL or credentials.

## Direct media boundary

Upload requires an actor-scoped `Idempotency-Key`, exactly one multipart
`file`, and no more than 25 MiB. The API accepts PNG, JPEG, and WebP by file
header, removes path/control characters from the original name, and computes
SHA-256 while staging. It writes one immutable object, then atomically commits
the catalogue row, audit event, and idempotency replay. If the database phase
fails, it deletes that exact object; if storage fails, it creates no row.

Every successful upload is immediately public. Known URLs remain readable when
referencing content is draft, published, or later unpublished. Publication
snapshots require only that each referenced asset exists and is not
soft-deleted. Public responses use the SHA-256 as a strong ETag, one-year
immutable caching, `nosniff`, and a sanitized inline or attachment disposition.

MinIO/S3 remains private. The API identity has only GetObject, PutObject, and
DeleteObject for compensation under the media prefix. The ordinary Worker has
no media credentials or media jobs. `/healthz` and `/readyz` cover the
platform and database boundary; there is no media-specific service or heartbeat.
Historical media review columns remain inert for schema compatibility and are
not read by runtime code.

The required product-master paths are:

```text
Owner-approved CSV -> checksum/idempotent staging -> schema/unit/reference validation
                   -> immutable source snapshot -> revision -> explicit publish

Future Feishu snapshot -> staging -> schema/unit/reference validation -> three-way diff
                       -> conflict resolution or accepted change -> revision -> publish
```

The initial CSV snapshot is authoritative only for its registered environment,
checksum and mapping version. Product facts and portal-owned localized
presentation advance on independent immutable revision clocks; a publish binds
the selected presentation to an exact fact revision. Private commercial columns
remain encrypted outside public product revisions, and unresolved asset filenames do not become downloads. The
Feishu transport adapter is not currently connected, so reserved credentials do
not cause remote synchronization.

Temporary local overrides retain the Feishu source value, require a reason and expiry, do not write back to Feishu, and cannot be republished after expiry until resolved.

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
successfully, and only then starts the API and Worker.

The supported operator entry points are `pnpm db:migrate`, `pnpm db:info`, and
`pnpm db:validate`. Application binaries do not expose schema or operational
commands. SQLx performs runtime queries and transactions only.

An existing database with the exact legacy SQLx v1-10 history requires a
controlled, one-time takeover. First back it up, verify restoration, and confirm
the target environment and database. Then run:

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

PostgreSQL persistence, Flyway-managed schema versions, internal jobs, and
durable outbox claiming are local platform capabilities. Local-file and
S3-compatible direct media transport are implemented. MinIO is the local/E2E
reference service; production media remains unverified until an approved
private HTTPS endpoint, least-privilege API identity, monitoring, backups, and
a real endpoint smoke test are in place. Feishu network synchronization, GA4
loading, CDN purge, external search
providers, email/CRM/webhooks, backup executors, and isolated restore executors
are not connected. Environment placeholders, passing local tests, or Admin
screens must not be treated as proof of an active external integration.
