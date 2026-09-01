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

When `DATABASE_URL` is absent, the API starts with an intentionally empty in-memory
catalog. This mode is suitable for contract tests and UI development only; it never
seeds product specifications or values from the HTML demos.

With PostgreSQL configured:

```sh
cargo run --bin airtek-migrate
cargo run --bin airtek-api
```

`airtek-migrate` needs only `DATABASE_URL`; it can be run repeatedly because
SQLx records applied migration versions. Admin authentication uses an Argon2id
password and a host-only HttpOnly session cookie. The one-time
`AIRTEK_ADMIN_BOOTSTRAP_TOKEN` is accepted only by `/api/admin/v1/auth/setup`
while no users exist. It is not a shared bearer credential.

TOTP secrets are generated from the operating-system CSPRNG and sealed with
AES-256-GCM before persistence. Set `AIRTEK_TOTP_ENCRYPTION_KEY` to Base64 for
exactly 32 random bytes; production builds refuse to start without it. Development
may start without the key, but enrollment and verification then return `503`
instead of storing plaintext or inventing an ephemeral key. TOTP uses SHA-1, six
digits, 30-second steps and a one-step clock tolerance. Recovery codes contain
80 random bits each, are stored only as independently salted Argon2id hashes, and
are consumed atomically once. Active sessions have a 30-minute idle timeout and
a 12-hour absolute timeout; users can list and revoke their own sessions.

Public Contact, RFQ and Analytics writes use fixed-window source limits. With
PostgreSQL the counters are durable; the in-memory development store expires
old counters and refuses new keys at a hard capacity. Raw client addresses are
not stored in the rate-limit table. The TCP peer is authoritative by default.
`X-Forwarded-For` is considered only when that direct peer is inside an explicit
`AIRTEK_TRUSTED_PROXY_CIDRS` entry, and the chain is walked from the nearest hop.
The Compose profile assigns its gateway `172.28.0.10` and trusts only
`172.28.0.10/32`; update the subnet, fixed address and trusted CIDR together if
that local network conflicts. Never trust an entire private address range merely
because it is private.

The development-only CLI and PTY terminal require an explicit feature:

```sh
cargo run --features devtools --bin airtekctl -- diagnose
cargo run --features devtools --bin airtekctl -- validate all
cargo run --features devtools --bin airtekctl -- sync dry-run --mapping-version development
cargo run --features devtools --bin airtekctl -- index rebuild
cargo run --features devtools --bin airtekctl -- cache invalidate
cargo run --features devtools --bin airtekctl -- jobs list
```

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
text and PTY output. `airtekctl` reports provider work only as queued; the worker
fails it explicitly when a Feishu, search or cache provider is not configured.

Production artifacts must be built with `--features production`. The crate rejects
`production,devtools` at compile time and the CLI binary is not compiled unless the
`devtools` feature is present.

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

The exporter is a Cargo example rather than a shipped runtime binary. Production
containers continue to contain only the explicitly selected API, worker, and
migration executables.

## Boundaries

- `/api/public/v1`: published public reads and validated submissions.
- `/api/admin/v1`: cookie-authenticated management operations with RBAC, strict
  Origin checking and CSRF validation for business mutations.
- `/api/devtools/v1`: absent unless compiled with `devtools`.
- `/robots.txt`: disallows the complete API origin.
- sitemap paths are deliberately unregistered and return `404`.

All exact product data must arrive through a traceable Feishu source snapshot,
validation, staging, conflict handling and publication. Demo product values are not
authoritative and are not present in this service.

Content and products keep mutable working records separately from immutable
published revision snapshots. Publishing or rolling back atomically switches the
public revision pointer and records an outbox event. The worker durably claims
those events; provider-specific CDN, search and sitemap adapters remain deployment
configuration and are not represented as already connected.

High-risk operation requests require a Super Admin role and a freshly verified
TOTP. Recovery codes can restore login access but are deliberately not accepted
as high-risk-operation reauthentication.
