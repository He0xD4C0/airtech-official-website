# HTTP gateway boundary

The Compose stack packages `nginx.conf.template` in a dedicated image and
renders it through the official Nginx container entrypoint. Nginx runs as its
unprivileged `nginx` user and listens on internal port `8088`. It expects three
distinct hosts:

- `PUBLIC_HOST` → `public-web:3000`
- `ADMIN_HOST` → `admin-web:3100`
- `API_HOST` → `platform-api:8080`

`API_BROWSER_ORIGIN` is inserted into the public and admin CSP `connect-src`.
Keep it equal to the browser-visible API origin; the public SSR process uses its
separate internal `PUBLIC_API_INTERNAL_URL` and does not route server-side reads
back through this gateway. The gateway replaces upstream CSP fields for the two
web origins so browsers enforce one Host-aware policy rather than the
intersection of two independently configured policies.

For the checked-in local defaults, start the complete stack with:

```sh
docker compose --project-directory . up --build
```

Then use `http://www.localhost:8088`,
`http://admin.localhost:8088`, and `http://api.localhost:8088`. A Host-header
probe that does not depend on local wildcard resolution is also possible:

```sh
curl -H 'Host: admin.localhost' http://127.0.0.1:8088/robots.txt
```

Unknown Hosts are rejected by the default server. The public Host rejects
`/admin`; the admin Host rejects `/en`, all sitemap variants, public manifests,
Vite manifests, and common webmaster verification files; the API Host rejects
sitemaps and the production DevTools namespace. The gateway intentionally has
no WebSocket or DevTools upstream.

TLS terminates at the hosting provider or an outer ingress. Public and admin may
both use external port 443 while the application processes remain on distinct
fixed ports. The Compose gateway is HTTP-only and is suitable for local
integration checks, not direct Internet exposure. Replace local hostnames,
origins, and all example credentials before any production deployment.

Local diagnostic listeners bind to `127.0.0.1`. The provider-neutral
`infra/compose/production.app.yaml` accepts independently versioned images and exposes
only this Gateway listener to the outer ingress; it does not publish Public,
Admin, or API application ports.
