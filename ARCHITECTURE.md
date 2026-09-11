# Platform architecture

## Process boundary

```text
Public browser ─┐   HTTP gateway :8088 (Host routing) ─> Public SSR :3000 ─┐
Admin browser  ─┼─────────────────────────────────────> Admin SPA  :3100   ├─> API :8080 ─> PostgreSQL
API clients    ─┘                                      API Host             ┘       ├──────> Worker / outbox
Public /media/* ───────── same-origin gateway proxy ───────────────────────┘       └──────> configured media storage

Public SSR ───────────────── internal service network ────────────────────> API :8080

Flyway 13.4.0 migration job ─────────── schema DDL ───────────────────────> PostgreSQL
```

The public SSR process and both browsers have no database or object-storage
credentials. The Rust API contains local-file and S3-compatible media storage
implementations; storage credentials belong only to that process. Storage is
disabled when no backend is selected or an S3 selection lacks any required
endpoint, bucket or credential. Malformed explicit values still fail
configuration, and an incomplete selection never falls back to local disk.
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
PostgreSQL is never host-published by the base stack. MinIO is an opt-in local
profile for the S3-compatible media adapter; its API and console loopback
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
by another entity returns 409. Relation targets must already be published, and
every referenced media/download asset version must be live, `clean` and
`public`, or publication returns 422.

The published public CMS API is hard-cut to `PublicContentProjection` schema
version 2. Generic content, route resolution, site bootstrap, News and discovery
read the published V2 `document`; canonical lookup and discovery additionally
use `public_routes`. They never fall back to a legacy `payload`. Legacy columns
and the `published_news` and `published_general_information` views remain in the
schema for migration evidence and rollback only. The former dedicated Admin
News and General Information mutation routes are removed; every CMS V2 kind
uses the unified `/api/admin/v1/content` lifecycle. Relations and content-link
IDs are resolved to public cards/URLs in the Rust service before the projection
reaches Web.

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
`/content`, `/content/templates`, draft, snapshot, revision, diff and restore
routes. Its media surface provides `GET /media/assets`,
`POST /media/uploads`, and `POST /media/assets/{id}/scan`. These routes retain
the existing Admin authentication, CSRF, permission and audit boundaries;
uploads and review require `media.write`.

The public surface under `/api/public/v1` provides V2 `/content/{kind}/{slug}`,
`/routes/resolve`, `/site-bootstrap`, `/news`, `/news/{slug}`, and `/discovery`,
plus `/media/{assetId}` and `/media/{assetId}/download`. The public Host maps
same-origin `/media/*` requests to those media endpoints through the gateway;
clients never receive an object-store URL or credential.

## Media boundary

The upload path accepts one multipart `file` of at most 25 MiB and recognizes
PNG, JPEG and WebP by byte signature. SVG is rejected. Every upload remains
`pending` until an administrator records a human `clean` or `quarantined`
decision with a non-empty reason. The existing `/scan` route name is retained
only as an API compatibility surface; there is no machine-scanning or
automatic-clean stage.

Public media lookup reveals only non-deleted catalogue rows whose scan status is
`clean` and access level is `public`; all other catalogue states return 404.
Delivery also verifies that the configured backend owns the stored object, and
uses strong ETags, immutable caching, `nosniff`, inline disposition by default,
and forced attachment for the download route. The gateway keeps the bucket
private and same-origin but is not a CDN. Local objects are read asynchronously
in bounded chunks, while S3 response bytes cross a bounded channel into the
Axum response body. The gateway disables response buffering for `/media/*`, so
same-origin delivery preserves that backpressure instead of collecting the
object at either hop. Local development keeps MinIO opt-in. The selected
single-host production topology operates MinIO in the independent
`airtek-infra` project and gives the API only bucket-scoped credentials over the
private `airtek-production` network. Human review remains the only approval
path in every environment.

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
`pnpm db:validate`. The Rust `airtek-migrate` binary and any `airtekctl migrate`
path have been removed. SQLx performs runtime queries and transactions only.

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

Development builds may include the browser terminal UI, PTY API, and
`airtekctl`. The PTY inherits the non-root OS identity that started the service;
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

Production builds exclude DevTools UI chunks, the Rust feature, API route,
WebSocket upstream, and CLI binaries. `production` and `devtools` are mutually
exclusive build features.

## Adapter readiness

PostgreSQL persistence, Flyway-managed schema versions, internal jobs, and
durable outbox claiming are local platform capabilities. Local-file and
S3-compatible media transport are implemented; MinIO is opt-in locally and is
operated as independent stateful infrastructure in the selected single-host
production topology. Feishu network synchronization, GA4
loading, CDN purge, external search providers, email/CRM/webhooks, backup
executors, and isolated restore executors are not
connected. Environment placeholders or Admin screens must not be treated as
proof of an active external integration.
