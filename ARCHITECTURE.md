# Platform architecture

## Process boundary

```text
Public browser ─┐   HTTP gateway :8088 (Host routing) ─> Public SSR :3000 ─┐
Admin browser  ─┼─────────────────────────────────────> Admin SPA  :3100   ├─> API :8080 ─> PostgreSQL
API clients    ─┘                                      API Host             ┘       └──────> Worker / outbox

Public SSR ───────────────── internal service network ────────────────────> API :8080

Flyway 13.4.0 migration job ─────────── schema DDL ───────────────────────> PostgreSQL
```

The public SSR process and both browsers have no database credentials. If an
object-storage adapter is added, its credentials belong only in the Rust API or
worker environment; no adapter is connected by this scaffold. Public and admin
builds do not share a router, cache namespace, service worker, manifest, or
crawl-control files.

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
PostgreSQL is never host-published by the base stack, and the unused MinIO
service is opt-in; their loopback mappings live only in `compose.debug.yaml`.
The image-only `compose.production.yaml` publishes only the Gateway listener;
Public `3000`, Admin `3100`, and API `8080` remain internal. Its five immutable
image references are independent release and rollback units.

## Rendering

- Public content, product, solution, technology, resource, company, contact, and RFQ-entry routes render meaningful HTML on the server.
- Filters, compare tray, PQ exploration, FAQ controls, and multi-step RFQ forms hydrate in the browser.
- Selector content is server-rendered while its calculation workflow is
  client-led. Comparison is a client-rendered route with server configuration
  for title/description and `noindex,follow`; it is not an SSR content page.
- The management portal is an independent SPA.

## Publication boundary

Content and products use mutable working drafts and immutable revisions.
Publishing atomically changes the public projection and emits a durable outbox
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
durable outbox claiming are local platform capabilities. MinIO/S3 media
transport, Feishu network synchronization, GA4 loading, CDN purge, external
search providers, email/CRM/webhooks, backup executors, and isolated restore
executors require separate adapters and production configuration. Their
environment placeholders or admin screens must not be treated as proof of an
active integration.
