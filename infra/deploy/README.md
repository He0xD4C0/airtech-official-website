# Production deployment contract

This directory is provider-neutral because the approved architecture leaves
the cloud, region, domain, registry, credential store, and TLS product as
deployment choices. `compose.production.yaml` defines the required process and
network boundary without inventing any of them.

## Release artifacts

Build and scan five immutable artifacts:

- Public Web from `infra/docker/Dockerfile.web`, with the final public and API
  browser origins passed as Vite build arguments.
- Admin Web from `infra/docker/Dockerfile.admin`, with the final Admin API
  browser origin passed at build time. Its production build forces DevTools off.
- Platform from `infra/docker/Dockerfile.platform`. It supplies only the API
  and Worker; it contains no application-owned CLI, schema migration tools, or
  DevTools PTY/WebSocket dependencies.
- Migrations from `infra/docker/Dockerfile.flyway`, pinned to Flyway `13.4.0`.
  It is an independent, non-root one-shot artifact and is the sole owner of
  PostgreSQL schema versions.
- Gateway from `infra/docker/Dockerfile.gateway`, containing only the
  production Host router. It has no DevTools/WebSocket upstream.

Use a digest or immutable release tag for every reference in the deployment
environment. Public and Admin references must remain separate release units so
one can be promoted or rolled back without restarting the other.

## External configuration

Before deployment:

1. Terminate TLS `443` at the hosting provider or an outer ingress and route
   only its internal connection to Gateway `8088`.
2. Keep Public `3000`, Admin `3100`, API `8080`, PostgreSQL and object storage
   off public interfaces. The production Compose file publishes none of them.
3. Set the three exact HTTPS origins and three distinct Hosts. Build-time Vite
   origins must match their runtime values.
4. Supply the API/Worker `DATABASE_URL` runtime credential from a secret
   manager. SQLx uses it only for runtime data access.
5. Pre-provision the bounded lowercase PostgreSQL role named by
   `FLYWAY_PLACEHOLDERS_RUNTIME_ROLE`; it must be the login in `DATABASE_URL`.
   Supply `FLYWAY_URL`, `FLYWAY_USER`, and `FLYWAY_PASSWORD` separately. The
   Flyway role is a deployment-only DDL identity and must own new schema objects;
   for legacy takeover it must own the old objects or hold their grant option.
   The runtime role must have `LOGIN`, must not own the database, schema, tables,
   or functions, and must not retain `CREATE` on `public`. `afterMigrate`
   enforces that boundary, grants `CONNECT`, DML, and the Worker's required
   database `TEMPORARY` privilege, and leaves schema history read-only. The
   Flyway role must be able to manage those
   database/schema grants; the production boundary rejects a shared DDL/runtime
   identity.
6. After Flyway succeeds, run the Platform image's one-shot
   `airtek-maintenance prepare-runtime` command with the runtime `DATABASE_URL`.
   Start API and Worker only after it exits successfully.
7. Supply any initial setup token from a secret manager and remove it after the
   first Super Admin has been created.
8. Trust only the exact Gateway address plus any exact outer-proxy hops needed
   to interpret `X-Forwarded-For`; never trust a whole private range by default.
9. Configure provider logs, health probes, retention, alerting and image/SBOM
   policy. These provider resources are intentionally
   absent from this repository.
10. Verify Search Console and webmaster files only on the Public origin. Admin
   and API must keep their crawl-denial and sitemap `404` behavior.

## Direct media object identity

Provision one private object-store identity from
`infra/object-storage/media-api-policy.json`, replacing the bucket placeholder
before attachment. It grants only GetObject, PutObject, and DeleteObject beneath
the configured `media/*` prefix. DeleteObject is used solely to compensate an
object whose catalogue transaction failed. The identity cannot list the bucket,
alter bucket policy, or make the bucket public.

The browser never receives object-store credentials or provider URLs. Successful
PNG, JPEG, and WebP uploads are immediately served without authentication by
the platform's public media route. Production must provide a private HTTPS
endpoint, scoped credentials, monitoring, and a smoke test that covers
upload, immediate GET, idempotent replay, conflict, and database-failure
compensation.

Run configuration and repository assertions before promotion:

```sh
node scripts/assert-deployment-config.mjs
docker compose --env-file infra/deploy/production.env.example -f compose.production.yaml config --quiet
pnpm check:production
pnpm check:contracts
```

## Deployment order

The current schema target is V19. Deploy the migration artifact first, then the
API and ordinary Worker, and finally Admin and Public Web. V17 introduces
private drafts and review, V18 removes persisted content history, and V19 adds
current-state query indexes. These migrations are forward-only.

Before promotion, verify a fresh database migrates directly to V19 and a
controlled legacy SQLx v1-v10 database passes
`baseline -> migrate -> validate`.

Run the non-root `flyway-migrate` artifact as a one-shot task before API and
Worker start; both services wait for its successful completion. Database schema
changes must remain compatible with the currently running Public/API release
during a rolling update.

For a new empty production database, run only the migrations artifact with
`migrate`. For an existing database with SQLx versions 1 through 10, confirm the
selected environment, database, dedicated DDL credentials, runtime role, and
ownership/grant topology.
Then perform the one-time takeover using the production Compose boundary and
the real secret-managed environment file:

```sh
docker compose --env-file /path/to/production.env -f compose.production.yaml run --rm flyway-migrate baseline
docker compose --env-file /path/to/production.env -f compose.production.yaml run --rm flyway-migrate migrate
docker compose --env-file /path/to/production.env -f compose.production.yaml run --rm flyway-migrate validate
```

The `beforeBaseline` callback permits the `baselineVersion=10` baseline only
after it verifies the exact successful SQLx v1-v10 history and checksums. Normal
migrate then leaves V1-V10 untouched and applies only later versions, if any.
Stop on any mismatch. Keep `baselineOnMigrate` disabled and never bypass the
controlled takeover.
