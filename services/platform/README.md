# AIRTEKPOWER platform service

Rust modular-monolith backend for the public site and management portal. It exposes
the API on fixed port `8080` and runs background work in a separate worker binary.

## Local development

All applications use the repository-root `.env`. Copy and customize the root
`.env.example`; do not create a second service-local environment file because it
can silently diverge from the frontend origins, Compose mappings and database
URLs.

```sh
cargo run --bin airtek-api
cargo run --bin airtek-worker
```

`DATABASE_URL` is required by the API and Worker. PostgreSQL remains the runtime
database, and SQLx is used only for application queries and transactions. The
in-memory repository is available only to isolated Rust tests; it is not a
server runtime mode and never seeds product specifications or values from the
HTML demos.

With PostgreSQL configured:

```sh
pnpm db:migrate
cargo run --bin airtek-api
```

Flyway `13.4.0` exclusively owns schema versions and migration history through
the independent, non-root `flyway-migrate` image. From the repository root, use
`pnpm db:migrate`, `pnpm db:info`, and `pnpm db:validate`; Flyway receives its
JDBC URL and credentials through `FLYWAY_URL`, `FLYWAY_USER`, and
`FLYWAY_PASSWORD`. `FLYWAY_PLACEHOLDERS_RUNTIME_ROLE` names the existing
PostgreSQL role used by the API and Worker. The guarded `afterMigrate` callback
requires it to have `LOGIN`, grants `CONNECT`, DML, and the Worker's required
database `TEMPORARY` capability while keeping migration history read-only,
removes public schema creation, and refuses an incomplete privilege topology.
In production, that runtime role must not own the database or schema objects.
Local Compose alone explicitly
allows its existing shared `airtek` owner for development-volume compatibility.
Application-owned migration and operations CLIs have been removed.

Admin authentication uses an Argon2id password and a host-only HttpOnly session
cookie. The one-time `AIRTEK_ADMIN_BOOTSTRAP_TOKEN` is accepted only by
`/api/admin/v1/auth/setup` while no users exist. It is not a shared bearer
credential.

### Existing SQLx v1-10 database takeover

For a database previously migrated by SQLx versions 1 through 10, first create
a backup, verify the restore path, and confirm the target environment, database,
and credentials. Do not run the ordinary migration path first. Run:

```sh
docker compose run --rm flyway-migrate baseline
pnpm db:migrate
pnpm db:validate
```

The `beforeBaseline` callback validates that `_sqlx_migrations` contains exactly
the successful reviewed v1-10 history and checksums before Flyway records
its `baselineVersion=10` baseline. The normal migrate therefore skips V1-V10 and
applies only later versions. Provision the declared runtime role first; the
migration role must own the legacy objects or have their grant option so
`afterMigrate` can apply the runtime grants. Stop if validation fails.
`baselineOnMigrate` remains disabled so takeover is always explicit; a new
empty database uses only `pnpm db:migrate`.

TOTP secrets are generated from the operating-system CSPRNG and sealed with
AES-256-GCM before persistence. Set `AIRTEK_TOTP_ENCRYPTION_KEY` to Base64 for
exactly 32 random bytes; production builds refuse to start without it. Development
may start without the key, but enrollment and verification then return `503`
instead of storing plaintext or inventing an ephemeral key. TOTP uses SHA-1, six
digits, 30-second steps and a one-step clock tolerance. Recovery codes contain
80 random bits each, are stored only as independently salted Argon2id hashes, and
are consumed atomically once. Active sessions have a 30-minute idle timeout and
a 12-hour absolute timeout; users can list and revoke their own sessions.

Content preview tokens are limited to one immutable revision and carry the
issuing Admin user and session identifiers inside the signature. The Public
preview endpoint performs a live PostgreSQL authorization check on every read:
the user must remain active with confirmed TOTP and current `content.read`, and
the issuing session must remain unrevoked and within both idle and absolute
expiry. Preview responses remain private, `no-store`, and `noindex`.

Public Contact, RFQ and Analytics writes use fixed-window source limits. With
PostgreSQL the counters are durable; the isolated in-memory test store expires
old counters and refuses new keys at a hard capacity. Raw client addresses are
not stored in the rate-limit table. The TCP peer is authoritative by default.
UTM source, medium and campaign values are persisted only when their normalized
identifier appears in the corresponding deployment allowlist. Unknown text,
including names or copied form values, is rejected. Filter values and editable
FAQ category labels are not accepted as analytics event properties.
`X-Forwarded-For` is considered only when that direct peer is inside an explicit
`AIRTEK_TRUSTED_PROXY_CIDRS` entry, and the chain is walked from the nearest hop.
The Compose profile assigns its gateway `172.28.0.10` and trusts only
`172.28.0.10/32`; update the subnet, fixed address and trusted CIDR together if
that local network conflicts. Never trust an entire private address range merely
because it is private.

The development-only browser PTY terminal requires the explicit `devtools`
feature. It opens the host `$SHELL` with the same non-root identity and working
directory as the API process; there is no application-owned command language.

The development seed is the only writer allowed to establish
`developmentFixture` ownership. When an editor clears a seeded record's
placeholder flag, the content/News/General Information transaction changes its
origin to `editorial`; that transition is one-way, and later seed runs use the
retained ledger only to recognize and skip the taken-over record. Content,
News, General Information, Navigation, and Footer now share the unified CMS V2
`/api/admin/v1/content` draft, snapshot, publish, diff, and restore lifecycle;
the former dedicated News and General Information mutation APIs are removed.

A devtools build refuses to start unless either PostgreSQL is configured for
existing admin sessions or a sufficiently long setup bootstrap token is
configured. The PTY token endpoint additionally requires an authenticated user
with `devtools.shell`, a valid CSRF token and the configured Admin origin. Tokens
expire after 60 seconds, are stored only as hashes and are consumed once. A PTY
upgrade is bound to the same still-active Admin session that requested its token.
At most four terminal sessions and 32 unused authorizations exist at once. The
server enforces a 15-minute idle timeout, a two-hour absolute timeout, 64 MiB
per-session output cap, bounded input frames and terminal dimensions, and emits resize-aware start/end events
with the shell exit code. The shell inherits the API process UID and development
working directory; no `sudo`, `setuid` or OS-user mapping is used.

Terminal establishment, termination and input-frame metadata are written to the
immutable platform audit log. For command frames the audit includes actor,
session, time, byte count and a fixed reason, but deliberately excludes command
text and PTY output. Feishu synchronization remains disabled until its provider
adapter is connected; no placeholder job is queued.

Production artifacts must be built with `--features production`. The crate
rejects `production,devtools` at compile time. The production Platform image
contains only API and Worker. The browser terminal and PTY tooling remain gated
by `devtools` and are absent from production.

## OpenAPI contract

`src/openapi.rs` documents every registered system, public, and admin route with
named request, response, and Problem Details schemas. Development builds include
the PTY routes only when `devtools` is compiled; the production export cannot
contain them. From the repository root, regenerate or verify the checked-in
snapshot and TypeScript types with:

```sh
pnpm generate:contracts
pnpm check:contracts
```

The exporter is a Cargo example rather than a shipped runtime binary. The
production Platform container contains the selected API and Worker plus the
restricted operations CLI; schema migration ships as a separate non-root Flyway
artifact.

## Boundaries

- `/api/public/v1`: published public reads and validated submissions.
- `/api/admin/v1`: cookie-authenticated management operations with RBAC, strict
  Origin checking and CSRF validation for business mutations.
- `/api/devtools/v1`: absent unless compiled with `devtools`.
- `/robots.txt`: disallows the complete API origin.
- sitemap paths are deliberately unregistered and return `404`.

All exact product data must arrive through a traceable, owner-approved Product
Master snapshot, validation, staging, conflict handling, and publication.
Future Feishu synchronization is an input to that governed flow, not proof of a
current approved snapshot. Demo product values are not authoritative and are
not present in this service.

Content and products keep mutable working records separately from immutable
published revision snapshots. Publishing or rolling back atomically switches the
public revision pointer and records an outbox event. The worker durably claims
those events; provider-specific CDN, search and sitemap adapters remain deployment
configuration and are not represented as already connected.

High-risk operation requests require a Super Admin role and a freshly verified
TOTP. Recovery codes can restore login access but are deliberately not accepted
as high-risk-operation reauthentication.
