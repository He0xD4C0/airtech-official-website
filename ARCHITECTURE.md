# Platform architecture

## Process boundary

```text
Public browser ─┐   HTTP gateway :8088 (Host routing) ─> Public SSR :3000 ─┐
Admin browser  ─┼─────────────────────────────────────> Admin SPA  :3100   ├─> API :8080 ─> PostgreSQL
API clients    ─┘                                      API Host             ┘       └──────> Worker / outbox

Public SSR ───────────────── internal service network ────────────────────> API :8080
```

The public SSR process and both browsers have no database credentials. If an
object-storage adapter is added, its credentials belong only in the Rust API or
worker environment; no adapter is connected by this scaffold. Public and admin
builds do not share a router, cache namespace, service worker, manifest, or
crawl-control files.

The checked-in gateway is an executable HTTP integration boundary. It rejects
unknown Hosts, routes the three configured Hosts to separate upstreams, and has
no production DevTools/WebSocket upstream. Production TLS terminates at an
outer ingress or hosting provider rather than inside this Compose stack.

Public SSR, Admin Nginx, Platform, and Gateway production images run as
non-root users. The local Compose diagnostic ports bind to loopback by default.
PostgreSQL is never host-published by the base stack, and the unused MinIO
service is opt-in; their loopback mappings live only in `compose.debug.yaml`.
The image-only `compose.production.yaml` publishes only the Gateway listener;
Public `3000`, Admin `3100`, and API `8080` remain internal. Its four immutable
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

The required product-master path is:

```text
Feishu snapshot -> staging -> schema/unit/reference validation -> three-way diff
                -> conflict resolution or accepted change -> revision -> publish
```

The staging, validation, diff, conflict, and revision boundaries exist in the
platform contract. The Feishu transport adapter is not currently connected, so
reserved credentials do not cause remote synchronization.

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

`airtek-migrate` is a non-root, one-shot production binary that reads only
`DATABASE_URL` and applies the embedded SQLx migrations. Compose waits for
PostgreSQL health, requires the migration container to exit successfully, and
only then starts the API and worker. Re-running the migration is idempotent
through SQLx's migration history table.

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

PostgreSQL persistence, embedded migrations, internal jobs, and durable outbox
claiming are local platform capabilities. MinIO/S3 media transport, Feishu
network synchronization, GA4 loading, CDN purge, external search providers,
email/CRM/webhooks, backup executors, and isolated restore executors require
separate adapters and production configuration. Their environment placeholders
or admin screens must not be treated as proof of an active integration.
