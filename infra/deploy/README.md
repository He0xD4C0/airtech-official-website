# Production deployment contract

The selected first production topology is one Alibaba Cloud ECS instance in
Singapore with two independent Docker Compose projects:

- `airtek-infra` owns the long-lived PostgreSQL and MinIO processes. Operators
  start, back up, restore and upgrade it independently from application releases.
- `airtek-app` owns Flyway, Platform API/Worker, Public Web, Admin Web and the
  HTTP Gateway. GitHub Actions may replace only these application containers.

Both projects join the pre-created external `airtek-production` Docker network.
Application deployment must never run `docker compose down` against the
infrastructure project and must never remove its bind-mounted data directories.

This is a single-host topology, not high availability. PostgreSQL and MinIO
backups must leave the ECS instance and must be restore-tested.

## Stateful infrastructure boundary

Copy `infrastructure.env.example` to
`/etc/airtek/infrastructure.env`, replace every secret and zero digest, then
install the Compose file and bootstrap scripts below `/opt/airtek/infra`, create
the shared network and start the infrastructure once:

```sh
docker network create --subnet 172.29.0.0/24 airtek-production
docker compose \
  --env-file /etc/airtek/infrastructure.env \
  -f compose.infrastructure.production.yaml \
  --profile bootstrap up -d
```

The bootstrap profile creates separate migration/runtime PostgreSQL roles, a
private media bucket and a bucket-scoped MinIO application user. PostgreSQL and
MinIO restart with the server; bootstrap tasks and Flyway remain one-shot.
Administrative ports bind only to `127.0.0.1` and are reachable remotely only
through an authenticated SSH tunnel.

## Release artifacts

Build and scan five immutable artifacts:

- Public Web from `infra/docker/Dockerfile.web`, with the final public and API
  browser origins passed as Vite build arguments.
- Admin Web from `infra/docker/Dockerfile.admin`, with the final Admin API
  browser origin passed at build time. Its production build forces DevTools off.
- Platform from `infra/docker/Dockerfile.platform`. It supplies the API and
  Worker only; it contains neither schema migration tools, `airtekctl`, nor
  PTY/WebSocket dependencies.
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
7. Supply any initial setup token from protected configuration and remove it after the
   first Super Admin has been created.
8. Configure the Platform API with `AIRTEK_MEDIA_STORAGE=s3`, the internal
   `http://minio:9000` endpoint and the bucket-scoped application credentials.
   Never give the application MinIO root credentials.
9. Trust only the exact Gateway address plus any exact outer-proxy hops needed
   to interpret `X-Forwarded-For`; never trust a whole private range by default.
10. Configure provider logs, health probes, off-host backups, restore targets, retention,
   alerting and image/SBOM policy. These provider resources are intentionally
   absent from this repository.
11. Verify Search Console and webmaster files only on the Public origin. Admin
   and API must keep their crawl-denial and sitemap `404` behavior.

Run configuration and repository assertions before promotion:

```sh
node scripts/assert-deployment-config.mjs
node scripts/assert-production-infrastructure.mjs
docker compose --env-file infra/deploy/production.env.example -f compose.production.yaml config --quiet
docker compose --env-file infra/deploy/infrastructure.env.example -f compose.infrastructure.production.yaml --profile bootstrap config --quiet
pnpm check:production
pnpm check:contracts
```

## GitHub application release

`.github/workflows/release-production.yml` starts only after a successful `CI`
run on `main`, or from an explicit manual dispatch with a full commit SHA. It
builds five images, pushes immutable SHA tags, uploads only the application
Compose file and `deploy-app.sh`, then updates `airtek-app`.

The server keeps `/etc/airtek/production.env` and its registry pull credential.
The workflow never receives PostgreSQL superuser or MinIO root secrets. A failed
application health check attempts to restore the previous image set; schema
migrations are forward-only and are never automatically reversed.

Create these GitHub repository variables before enabling the workflow:

- `PRODUCTION_RELEASE_ENABLED=false` until the ECS and every secret are ready;
  change it to `true` only after the first manual release succeeds.
- `ACR_REGISTRY`, `ACR_NAMESPACE`, and optional
  `PRODUCTION_CONTAINER_PLATFORM` (defaults to `linux/amd64`).
- `PRODUCTION_PUBLIC_ORIGIN`, `PRODUCTION_ADMIN_ORIGIN`, and
  `PRODUCTION_API_ORIGIN`, all exact HTTPS origins.

Create `ACR_USERNAME` and `ACR_PASSWORD` as repository secrets used only for
image push. Give that registry identity no ECS or cloud-administration access.

Create these secrets in the protected `production` Environment:

- `ECS_HOST`, `ECS_PORT`, `ECS_USER`, `ECS_SSH_PRIVATE_KEY`, and the pinned
  `ECS_SSH_KNOWN_HOSTS` entry for application deployment.

The ECS deploy user needs write access only below `/opt/airtek`, read access to
`/etc/airtek/production.env`, and permission to control Docker. Do not use the
root account or place the host private key in the repository.

Run the non-root `flyway-migrate` artifact as a one-shot task before API and
Worker start; both services wait for its successful completion. Database schema
changes must remain compatible with the currently running Public/API release
during a rolling update. Application rollback never replaces the database;
restore operations require the separately configured isolated-restore flow.

For a new empty production database, run only the migrations artifact with
`migrate`. For an existing database with SQLx versions 1 through 10, first back
up the database, verify restoration, and confirm the selected environment,
database, dedicated DDL credentials, runtime role, and ownership/grant topology.
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
