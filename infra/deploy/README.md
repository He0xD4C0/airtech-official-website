# Production deployment contract

Owner decision recorded 2026-09-26: the selected first production provider and
region are Alibaba Cloud ECS in Singapore. The implemented topology uses one ECS
instance with two independent operational boundaries:

- `airtek-infra` owns the long-lived PostgreSQL process. Operators start, back
  up, restore and upgrade it independently from application releases.
- `airtek-app` owns Flyway, Platform API/Worker, Public Web, Admin Web and the
  HTTP Gateway. GitHub Actions may replace only these application containers.

Object storage is not application Compose configuration. An administrator
connects an external S3-compatible service through the Admin GUI after startup;
the application stores that configuration in PostgreSQL.

Both projects join the pre-created external `airtek-production` Docker network.
Application deployment must never run `docker compose --project-directory . down` against the
infrastructure project and must never remove its bind-mounted data directories.

This is a single-host topology, not high availability. PostgreSQL backups must
leave the ECS instance and must be restore-tested. The selected object-storage
provider needs its own reviewed durability, backup and restore controls.
The decision does not approve an instance size, price, production domain,
object-storage provider, or production-launch status.

## Stateful infrastructure boundary

Copy `infrastructure.env.example` to
`/etc/airtek/infrastructure.env`, replace every secret and zero digest, then
install the Compose file and bootstrap scripts below `/opt/airtek/infra`, create
the shared network and start the infrastructure once:

```sh
docker network create --subnet 172.29.0.0/24 airtek-production
docker compose --project-directory . \
  --env-file /etc/airtek/infrastructure.env \
  -f infra/compose/production.infrastructure.yaml \
  --profile bootstrap up -d
```

The bootstrap profile creates separate migration/runtime PostgreSQL roles.
PostgreSQL restarts with the server; bootstrap tasks and Flyway remain one-shot.
Its administrative port binds only to `127.0.0.1` and is reachable remotely
only through an authenticated SSH tunnel.

## Release artifacts

Build and scan five immutable artifacts:

- Public Web from `infra/docker/Dockerfile.web`, with the final public and API
  browser origins passed as Vite build arguments.
- Admin Web from `infra/docker/Dockerfile.admin`, with the final Admin API
  browser origin passed at build time. Its production build forces DevTools off.
- Platform from `infra/docker/Dockerfile.platform`. It supplies the API, Worker,
  and restricted `airtek-maintenance` binary; it contains no schema migration
  tools or DevTools PTY/WebSocket dependencies.
- Migrations from `infra/docker/Dockerfile.flyway`, pinned to Flyway `13.4.0`.
  It is an independent, non-root one-shot artifact and is the sole owner of
  PostgreSQL schema versions.
- Gateway from `infra/docker/Dockerfile.gateway`, containing only the
  production Host router. It has no DevTools/WebSocket upstream.

Use a digest or immutable release tag for every reference in the deployment
environment. Public and Admin references must remain separate release units so
one can be promoted or rolled back without restarting the other.

## Application configuration

Before deployment:

1. Copy `production.env.example` to `/etc/airtek/production.env`. Keep it only
   on the ECS host with mode `0600`; never upload it as a release artifact.
2. Terminate TLS `443` at the hosting provider or an outer ingress and route
   only its internal connection to Gateway `8088`.
3. Keep Public `3000`, Admin `3100`, API `8080`, PostgreSQL and object storage
   off public interfaces. The production Compose file publishes none of them.
4. Set the three exact HTTPS origins and three distinct Hosts. Build-time Vite
   origins must match their runtime values.
5. Supply the API/Worker `DATABASE_URL` runtime credential from the protected
   host environment. SQLx uses it only for runtime data access.
6. Pre-provision the bounded lowercase PostgreSQL role named by
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
7. After Flyway succeeds, run the Platform image's one-shot
   `airtek-maintenance prepare-runtime` command with the runtime `DATABASE_URL`.
   Start API and Worker only after it exits successfully.
8. Supply any initial setup token from a secret manager and remove it after the
   first Super Admin has been created.
9. Configure object storage after startup through Admin. Use a least-privilege
   identity and a reviewed public delivery base URL; do not place S3 fields or
   credentials in `production.env` or application Compose.
10. Trust only the exact Gateway address plus any exact outer-proxy hops needed
   to interpret `X-Forwarded-For`; never trust a whole private range by default.
11. Configure provider logs, health probes, off-host backups, restore targets,
    retention, alerting and image/SBOM policy. These provider resources are
    intentionally absent from this repository.
12. Verify Search Console and webmaster files only on the Public origin. Admin
    and API must keep their crawl-denial and sitemap `404` behavior.

The fixed local-development administrator is not a deployment mechanism.
Production images compile only the `production` feature and expose only the
maintenance commands `prepare-runtime`, `inspect-public-site`, and
`check-public-readiness`; they receive no `AIRTEK_DEV_ADMIN_*` configuration.
A fresh production database therefore remains without users until the one-time
setup flow is completed.

## Direct media object identity

Provision one private object-store identity from
`infra/object-storage/media-api-policy.json`, replacing the bucket placeholder
before attachment. It grants only GetObject, PutObject, and DeleteObject beneath
the configured `media/*` prefix. DeleteObject is used solely to compensate an
object whose catalogue transaction failed. The identity cannot list the bucket,
alter bucket policy, or make the bucket public.

The browser never receives object-store credentials. Administrators configure
the S3-compatible endpoint, bucket, scoped credentials, and public delivery
base URL in Admin; the API stores them in PostgreSQL. Successful PNG, JPEG, and
WebP uploads persist an immutable external public URL. Production must provide
a private HTTPS API endpoint, a public CDN or bucket URL, monitoring, and a
smoke test covering upload, anonymous GET, idempotent replay, conflict, and
database-failure compensation.

Run configuration and repository assertions before promotion:

```sh
node scripts/checks/assert-deployment-config.mjs
node scripts/checks/assert-production-infrastructure.mjs
docker compose --project-directory . --env-file infra/deploy/production.env.example -f infra/compose/production.app.yaml config --quiet
docker compose --project-directory . --env-file infra/deploy/infrastructure.env.example -f infra/compose/production.infrastructure.yaml --profile bootstrap config --quiet
pnpm check:production
pnpm check:contracts
```

After the candidate containers are healthy, `deploy-app.sh` runs the same
read-only public readiness gate from the Public Web image. The release is not
activated when a core route, CMS shell, CORS preflight, canonical URL,
sitemap, manifest, icon, or origin contract fails, or when published
development placeholders are present. A missing custom icon is warning-only.

## Jenkins application release

The Jenkins multibranch job polls `main` and `cicd`. Both branches run the same
CI gates. `main` stops after CI; `cicd` builds and pushes the five domain-neutral
images to GHCR using the full 40-character Git SHA. With `CD_ENABLED=false`, the
run ends as `PUBLISHED_NOT_DEPLOYED` and never attempts SSH. The target host
keeps runtime origins in `/etc/airtek/production.env`, so a released SHA can be
reapplied under different reviewed domains without rebuilding.

`.github/workflows/release-production.yml` is retained only as a migration-time
manual publishing fallback. It requires an explicit full commit SHA, publishes
the same immutable tags, accepts no deployment credentials, and has no server
deployment job.

The server keeps `/etc/airtek/production.env` and its registry pull credential.
The publishing workflow never receives PostgreSQL superuser, object-storage,
or server access secrets. The separate deployment script retains its application
health rollback; schema migrations are forward-only and are never automatically
reversed.

The optional `PRODUCTION_CONTAINER_PLATFORM` GitHub repository variable defaults
to `linux/amd64`. Runtime domains are deliberately absent from image publishing.

Image publishing uses the workflow's short-lived `GITHUB_TOKEN` with
`packages: write`; no external registry credentials are required. Images use
the names `ghcr.io/<owner>/airtekpower-{public-web,admin-web,platform,migrations,gateway}`
and the full release commit SHA.

Before a later deployment, authenticate the server's Docker client to `ghcr.io` with a
dedicated read-only GitHub token that has `read:packages`. Keep that credential
only in the server's Docker credential store; never add it to the repository or
the workflow. Public packages may be pulled anonymously if their visibility is
deliberately changed after review.

Jenkins deployment credentials are added only after the target host and
operational controls are ready. They remain in Jenkins Credentials and are not
serialized by JCasC or committed to the repository.

## Deployment order

The current schema target is V28. Deploy the migration artifact first, then the
API and ordinary Worker, and finally Admin and Public Web. V17 introduces
private drafts and review, V18 removes persisted content history, and V19 adds
current-state query indexes. V20 moves application-side object-storage settings
into PostgreSQL and adds immutable public media URLs. These migrations are
forward-only.

V21 adds immutable media preview derivatives. V22-V24 add the Feishu product
synchronization foundation, source-asset bindings, and revision provenance.
V25 stores the Admin-managed Feishu App ID and write-only App Secret as
plaintext PostgreSQL columns; database operators and backups are therefore part
of the trusted boundary. V26 adds dynamic selected sources, independent interval
and daily schedules, connection-test revisions, and frozen run configuration.
V27 adds explicit source ownership, per-table reconciliation, durable product
purge/object compensation, and removes Feishu rollback/conflict state. V28 adds
admin-only supplier and brand archive provenance. Before promotion, verify both
a fresh database and a V21 database migrate to V28, and a
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
docker compose --project-directory . --env-file /path/to/production.env -f infra/compose/production.app.yaml run --rm flyway-migrate baseline
docker compose --project-directory . --env-file /path/to/production.env -f infra/compose/production.app.yaml run --rm flyway-migrate migrate
docker compose --project-directory . --env-file /path/to/production.env -f infra/compose/production.app.yaml run --rm flyway-migrate validate
```

The `beforeBaseline` callback permits the `baselineVersion=10` baseline only
after it verifies the exact successful SQLx v1-v10 history and checksums. Normal
migrate then leaves V1-V10 untouched and applies only later versions, if any.
Stop on any mismatch. Keep `baselineOnMigrate` disabled and never bypass the
controlled takeover.
