# Public-site recovery and readiness

The daily Compose project and local production-feature builds use the same
business database. Do not copy test volumes, reset the database, or publish
test content to make readiness pass.

## Local application build

Use the existing business environment file and project name with both
`compose.yaml` and `compose.local-production.yaml`. The latter selects production
Rust features and removes development seed settings. Use `up --build --no-deps`
for application services only; do not recreate PostgreSQL or MinIO. Preserve the
existing encryption keys. The local origins are:

- `http://www.airtek.localhost:8088`
- `http://admin.airtek.localhost:8088`
- `http://api.airtek.localhost:8088`

`www.localhost:8088` redirects permanently, including path and query. Native
diagnostic ports remain separate. Web `/healthz` proves process liveness, not
editorial readiness; Admin remains reachable while Public returns 503.

## Editorial recovery

Run `airtek-maintenance inspect-public-site` using the existing database
connection. Its read-only transaction returns the three shell identities,
15 core pages, existing draft paths, and publish-validation field issues.

To prepare missing drafts only, pass that JSON report to
`node scripts/prepare-public-drafts.mjs <report.json>`. Supply
`AIRTEK_DRAFT_API_ORIGIN`, `AIRTEK_DRAFT_ADMIN_ORIGIN`,
`AIRTEK_DRAFT_SESSION_COOKIE`, and `AIRTEK_DRAFT_CSRF_TOKEN` through a protected
operator environment from an authenticated Admin session. Never commit or log
cookies or tokens. The helper reuses Admin template defaults and authenticated
CMS creation; it does not change existing drafts or submit, approve, or publish.

Complete the reported fields in Admin. Company and legal wording requires
owner approval. Publish linked targets before navigation/footer; clear the
placeholder classification only after editorial verification. Real product
records remain authoritative; missing curves require engineering review.

## Independent readiness

`airtek-maintenance check-public-readiness` requires the three
`AIRTEK_{PUBLIC,ADMIN,API}_ORIGIN` values and `PUBLIC_HOST`, `ADMIN_HOST`, `API_HOST`.
It checks the database read-only, shell/routes, POST preflight headers,
canonical URLs, robots, every sitemap XML, delivered icon bytes, manifest, and
browser bundle API origins. An unpublished or placeholder site must fail.
Missing custom icon produces a warning, not a release blocker.

Production Compose exposes this command as `public-readiness`. The deployment
script runs it first through the internal gateway and then with
`AIRTEK_READINESS_EXTERNAL=true` through external HTTPS. The current-release
link changes only after both succeed. A failed upgrade restores application
images, never rolls back the database. First installation can stay accessible
to Admin without being marked ready. No remote deployment is configured here.

For loopback-only checks explicitly set `AIRTEK_READINESS_ALLOW_HTTP=true`.
Never use that override for production acceptance.

## Test data boundary

`pnpm test:postgres` and `pnpm test:e2e:stack` refuse inherited business database
connection variables. They allocate uniquely named `airtek_test_*` databases
inside disposable containers/volumes and clean only their own resources on
completion, failure, or interrupt. They do not use main-database test schemas.
E2E host ports and subnet have `AIRTEK_E2E_*` overrides for concurrent runs.

Report code validation, business-content approval, and production readiness
separately. Passing an isolated production-image test is not evidence that the
business site has approved published content.
