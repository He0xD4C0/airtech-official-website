# Production deployment contract

This directory is provider-neutral because the approved architecture leaves
the cloud, region, domain, registry, credential store, and TLS product as
deployment choices. `compose.production.yaml` defines the required process and
network boundary without inventing any of them.

## Release artifacts

Build and scan four immutable artifacts:

- Public Web from `infra/docker/Dockerfile.web`, with the final public and API
  browser origins passed as Vite build arguments.
- Admin Web from `infra/docker/Dockerfile.admin`, with the final Admin API
  browser origin passed at build time. Its production build forces DevTools off.
- Platform from `infra/docker/Dockerfile.platform`. The same image supplies the
  API, Worker and one-shot migration binaries; it contains neither `airtekctl`
  nor PTY/WebSocket dependencies.
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
4. Supply `DATABASE_URL` and any initial setup token from a secret manager.
   Remove the setup token after the first Super Admin has been created.
5. Trust only the exact Gateway address plus any exact outer-proxy hops needed
   to interpret `X-Forwarded-For`; never trust a whole private range by default.
6. Configure provider logs, health probes, backups, restore targets, retention,
   alerting and image/SBOM policy. These provider resources are intentionally
   absent from this repository.
7. Verify Search Console and webmaster files only on the Public origin. Admin
   and API must keep their crawl-denial and sitemap `404` behavior.

Run configuration and repository assertions before promotion:

```sh
node scripts/assert-deployment-config.mjs
docker compose --env-file infra/deploy/production.env.example -f compose.production.yaml config --quiet
pnpm check:production
pnpm check:contracts
```

Run migrations as a one-shot task before API and Worker start. Database schema
changes must remain compatible with the currently running Public/API release
during a rolling update. Application rollback never replaces the database;
restore operations require the separately configured isolated-restore flow.
