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

## Tag-triggered application release

Pushing a release tag runs the full CI gate set and then publishes five
domain-neutral images to GHCR. Only two prefixes are accepted: `dev-arm64-*`
builds `linux/arm64` for the development VM, and `release-x86-*` builds
`linux/amd64` for production. Tags must carry a lexically sortable suffix, for
example `dev-arm64-20261002-01`, because the on-host agent resolves the newest
release from registry tag names. The publishing job keeps the newest five
versions per package and never publishes a mutable `latest` tag.

The two streams never share a container package: development images publish to
`airtekpower-<component>-dev` while release images keep
`airtekpower-<component>`. A development release therefore cannot overwrite or
evict a release artifact, and retention pruning is scoped to the publishing
stream. Packages published from this public repository default to public
visibility, which is the current state, so target hosts pull anonymously. To
make them private, change each package's visibility in GitHub and add a
`read:packages` token to `/etc/airtek/cd.env`; the on-host agent already
supports that credential.

MinIO withdrew its public images and prebuilt binaries, so the release also
publishes `airtekpower-minio[-dev]` and `airtekpower-minio-mc[-dev]` built from
the pinned upstream source commits in `infra/docker/Dockerfile.minio` and
`infra/docker/Dockerfile.mc`. The Jenkins pipeline still publishes only the five
application images and is retained as a legacy path; the tag workflow is the
only publisher of the object-storage images.

The source-built server runs as UID `10001`, so a host media directory created
for the previous root-running public image must be reassigned once before the
first GitHub-backed media release: `chown -R 10001:10001 /srv/airtek/data/minio`.

Set the repository variable `AIRTEK_IMAGE_PREFIX` when the package owner is not
the default `ghcr.io/<repository-owner>/airtekpower`, and `AIRTEK_ARM64_RUNNER`
when the native arm64 runner is unavailable; the `ubuntu-latest` fallback builds
ARM64 through QEMU and is substantially slower. Publishing uses the workflow's
short-lived `GITHUB_TOKEN` with `packages: write`, so no registry secret is
stored in the repository. Images are named
`${AIRTEK_IMAGE_PREFIX}-{public-web,admin-web,platform,migrations,gateway}` with
the exact release tag. Runtime domains are deliberately absent from publishing.

The `airtek-cd-agent` on each target host pulls the published tag, checks out
the same tag, runs Compose and gates the release on readiness. It keeps its
state and failure records under `/opt/airtek/cd`. The server keeps runtime
origins in `/etc/airtek/production.env` and its read-only pull credential in
`/etc/airtek/cd.env` (mode 0600), so a released tag can be redeployed under
different reviewed domains without rebuilding. Publishing never receives
PostgreSQL superuser, object-storage, or server access secrets.

The Jenkins pipeline remains in the repository for the current development VM
but no longer owns deployment; keep `CD_ENABLED` false and retire the pipeline
after the agent has served several releases.

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

Compose runs the non-root `flyway-migrate` artifact's `release` command before
API and Worker start; both services wait for its successful completion. That
command writes a custom-format dump into `/srv/airtek/migration-state` only when
migrations are pending, removes it after a successful promotion, and restores
it when the migration fails. A restored failure exits `2`, writes a retained
JSON record under `migration-state/failures`, and inserts a `migrationApply`
failure row into `operation_runs`; an unrestorable failure exits `3` and keeps
the dump. Database schema changes must remain compatible with the currently
running Public/API release during a rolling update (expand/contract).

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
