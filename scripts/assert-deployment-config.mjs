import { existsSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = dirname(dirname(fileURLToPath(import.meta.url)))
const failures = []

function read(relativePath) {
  const path = join(root, relativePath)
  if (!existsSync(path)) {
    failures.push(`${relativePath} is missing.`)
    return ''
  }
  return readFileSync(path, 'utf8')
}

function requireMatch(body, pattern, failure) {
  if (!pattern.test(body)) failures.push(failure)
}

function forbidMatch(body, pattern, failure) {
  if (pattern.test(body)) failures.push(failure)
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&')
}

function serviceBlock(body, name) {
  const marker = `  ${name}:\n`
  const start = body.indexOf(marker)
  if (start < 0) {
    failures.push(`Compose service ${name} is missing.`)
    return ''
  }
  const remainder = body.slice(start + marker.length)
  const nextService = remainder.search(/^  [a-z0-9][a-z0-9-]*:\s*$/mu)
  return nextService < 0 ? remainder : remainder.slice(0, nextService)
}

const dockerignore = read('.dockerignore')
for (const marker of ['.git', '**/node_modules', '**/dist', '**/target', 'docs']) {
  requireMatch(dockerignore, new RegExp(`^${marker.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&')}$`, 'mu'), `.dockerignore must exclude ${marker}.`)
}

const webDockerfile = read('infra/docker/Dockerfile.web')
requireMatch(webDockerfile, /^FROM\s+node:22-alpine\s+AS\s+runtime$/mu, 'Public Web Dockerfile needs a separate runtime stage.')
requireMatch(webDockerfile, /^USER\s+node$/mu, 'Public Web runtime must use the non-root node user.')
requireMatch(webDockerfile, /deploy\s+--prod\s+--legacy\s+\/runtime/u, 'Public Web runtime dependencies must be production-only.')
requireMatch(webDockerfile, /ARG\s+VITE_PUBLIC_API_BASE_URL/u, 'Public Web Dockerfile must accept the browser API build argument.')
requireMatch(webDockerfile, /^EXPOSE\s+3000$/mu, 'Public Web image must expose only its fixed application port 3000.')
forbidMatch(webDockerfile, /ARG\s+PUBLIC_ENV__/u, 'Public Web Dockerfile still uses a non-exposed Vite environment prefix.')

const webVite = read('apps/web/vite.config.ts')
requireMatch(webVite, /port:\s*3000/u, 'Public Vite development and preview servers must use port 3000.')
requireMatch(webVite, /strictPort:\s*true/u, 'Public Vite must fail instead of selecting a different port.')

const platformDockerfile = read('infra/docker/Dockerfile.platform')
requireMatch(platformDockerfile, /--features\s+production/u, 'Platform production image must compile only the production feature set.')
requireMatch(platformDockerfile, /--bin\s+airtek-migrate/u, 'Platform Dockerfile must build airtek-migrate.')
requireMatch(platformDockerfile, /release\/airtek-migrate\s+\/usr\/local\/bin\/airtek-migrate/u, 'Platform runtime must contain airtek-migrate.')
requireMatch(platformDockerfile, /^USER\s+10001$/mu, 'Platform runtime must remain non-root.')
requireMatch(platformDockerfile, /^EXPOSE\s+8080$/mu, 'Platform image must expose only its fixed API port 8080.')
forbidMatch(platformDockerfile, /airtekctl/u, 'Production Platform image must not build or copy airtekctl.')

const adminDockerfile = read('infra/docker/Dockerfile.admin')
requireMatch(adminDockerfile, /\/etc\/nginx\/templates\/default\.conf\.template/u, 'Admin Nginx config must be rendered as an environment-aware template.')
requireMatch(adminDockerfile, /^ENV\s+VITE_ENABLE_DEVTOOLS=false$/mu, 'Admin production image must force DevTools off.')
requireMatch(adminDockerfile, /^USER\s+nginx$/mu, 'Admin runtime must use the non-root nginx user.')
requireMatch(adminDockerfile, /^EXPOSE\s+3100$/mu, 'Admin image must expose only its fixed application port 3100.')

const adminVite = read('apps/admin/vite.config.ts')
requireMatch(adminVite, /port:\s*3100/u, 'Admin Vite development and preview servers must use port 3100.')
requireMatch(adminVite, /strictPort:\s*true/u, 'Admin Vite must fail instead of selecting a different port.')
requireMatch(adminVite, /apiConnectSources\(env\.VITE_ADMIN_API_BASE_URL\)/u, 'Admin development CSP must derive HTTP and WebSocket origins from VITE_ADMIN_API_BASE_URL.')
requireMatch(adminVite, /requestedDevtools\s*=\s*env\.VITE_ENABLE_DEVTOOLS\s*===\s*'true'/u, 'Admin DevTools must require an explicit development opt-in.')
forbidMatch(adminVite, /connect-src 'self' http:\/\/localhost:8080 ws:\/\/localhost:8080/u, 'Admin development CSP must not hard-code an API port that bypasses the root environment.')

const platformConfig = read('services/platform/src/config.rs')
requireMatch(platformConfig, /pub const API_PORT:\s*u16\s*=\s*8080;/u, 'Rust API must use the fixed application port 8080.')
const platformApi = read('services/platform/src/bin/api.rs')
requireMatch(platformApi, /config\.database_url\.is_none\(\)/u, 'The running Rust API must reject the test-only in-memory repository.')

const gatewayDockerfile = read('infra/docker/Dockerfile.gateway')
requireMatch(gatewayDockerfile, /infra\/gateway\/nginx\.conf\.template/u, 'Gateway image must package the checked-in Host router.')
requireMatch(gatewayDockerfile, /^USER\s+nginx$/mu, 'Gateway runtime must use the non-root nginx user.')
requireMatch(gatewayDockerfile, /^EXPOSE\s+8088$/mu, 'Gateway image must expose its unprivileged port 8088.')

const compose = read('compose.yaml')
requireMatch(compose, /^\s{2}platform-migrate:\s*$/mu, 'Compose must define the one-shot migration service.')
requireMatch(compose, /entrypoint:\s*\["\/usr\/local\/bin\/airtek-migrate"\]/u, 'Compose migration service must run airtek-migrate.')
const migrationWaits = compose.match(/condition:\s*service_completed_successfully/gu)?.length ?? 0
if (migrationWaits < 2) failures.push('Both API and Worker must wait for a successful migration service.')
requireMatch(compose, /^\s{2}gateway:\s*$/mu, 'Compose must define the HTTP gateway service.')
requireMatch(compose, /dockerfile:\s*infra\/docker\/Dockerfile\.gateway/u, 'Compose must build the non-root gateway image.')
requireMatch(compose, /PUBLIC_API_INTERNAL_URL:\s*\$\{PUBLIC_API_INTERNAL_URL:-http:\/\/platform-api:8080\/api\/public\/v1\}/u, 'Public SSR must receive a configurable internal API URL with the fixed service default.')
requireMatch(compose, /AIRTEK_ADMIN_BOOTSTRAP_TOKEN:/u, 'Compose must configure the setup-only admin bootstrap secret.')
forbidMatch(compose, /AIRTEK_ADMIN_BEARER_TOKEN:/u, 'Compose must not present the setup secret as a reusable bearer token.')
for (const [variable, hostPort, containerPort] of [
  ['AIRTEK_API_HOST_PORT', 8080, 8080],
  ['AIRTEK_PUBLIC_HOST_PORT', 3000, 3000],
  ['AIRTEK_ADMIN_HOST_PORT', 3100, 3100],
  ['AIRTEK_GATEWAY_HOST_PORT', 8088, 8088],
]) {
  requireMatch(
    compose,
    new RegExp(`\\$\\{AIRTEK_BIND_ADDRESS:-127\\.0\\.0\\.1\\}:\\$\\{${variable}:-${hostPort}\\}:${containerPort}`, 'u'),
    `Local diagnostic port ${containerPort} must have a configurable loopback host mapping.`,
  )
}
for (const variable of ['POSTGRES_DB', 'POSTGRES_USER', 'POSTGRES_PASSWORD', 'AIRTEK_DATABASE_URL_INTERNAL']) {
  requireMatch(compose, new RegExp(`\\$\\{${variable}`, 'u'), `Base Compose must configure ${variable} through .env.`)
}
requireMatch(compose, /AIRTEK_TOTP_ENCRYPTION_KEY:\s*\$\{AIRTEK_TOTP_ENCRYPTION_KEY:-\}/u, 'Local API must receive the optional TOTP encryption key from .env.')
requireMatch(serviceBlock(compose, 'platform-api'), /AIRTEK_PREVIEW_SIGNING_KEY:\s*\$\{AIRTEK_PREVIEW_SIGNING_KEY:-\}/u, 'Local API must receive the optional preview signing key from .env.')
requireMatch(serviceBlock(compose, 'platform-worker'), /AIRTEK_PREVIEW_SIGNING_KEY:\s*\$\{AIRTEK_PREVIEW_SIGNING_KEY:-\}/u, 'Local Worker must receive the optional preview signing key from .env.')
for (const variable of ['AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY', 'AIRTEK_ANALYTICS_TOKEN_HMAC_KEY', 'AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY', 'AIRTEK_GUEST_RAW_RETENTION_DAYS', 'AIRTEK_GUEST_AGGREGATE_RETENTION_MONTHS', 'AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION', 'AIRTEK_APPROVED_PRODUCT_MASTER_SHA256', 'AIRTEK_APPROVED_PRODUCT_MASTER_MAPPING_VERSION', 'AIRTEK_APPROVED_PRODUCT_MASTER_VALID_ROWS', 'AIRTEK_APPROVED_PRODUCT_MASTER_ERROR_ROWS', 'AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES', 'AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS', 'AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS']) {
  requireMatch(serviceBlock(compose, 'platform-api'), new RegExp(`${variable}:\\s*\\$\\{${variable}`, 'u'), `Local API must receive ${variable} from .env.`)
  requireMatch(serviceBlock(compose, 'platform-worker'), new RegExp(`${variable}:\\s*\\$\\{${variable}`, 'u'), `Local Worker must receive ${variable} from .env.`)
}
forbidMatch(serviceBlock(compose, 'postgres'), /^\s+ports:/mu, 'Base Compose must not publish PostgreSQL to the host.')
forbidMatch(serviceBlock(compose, 'minio'), /^\s+ports:/mu, 'Base Compose must not publish MinIO API or console ports to the host.')
requireMatch(serviceBlock(compose, 'minio'), /profiles:\s*\["object-storage"\]/u, 'Unused local object storage must remain opt-in.')
forbidMatch(serviceBlock(compose, 'platform-worker'), /^\s+(?:ports|expose):/mu, 'Worker must not expose or publish a listening port.')
forbidMatch(serviceBlock(compose, 'public-web'), /DATABASE_URL/u, 'Public SSR must not receive database credentials.')
forbidMatch(serviceBlock(compose, 'admin-web'), /DATABASE_URL/u, 'Admin SPA must not receive database credentials.')
requireMatch(compose, /ipv4_address:\s*\$\{AIRTEK_GATEWAY_INTERNAL_IP:-172\.28\.0\.10\}/u, 'Local gateway must have the fixed address used by the trusted-proxy policy.')
requireMatch(compose, /AIRTEK_TRUSTED_PROXY_CIDRS:\s*\$\{AIRTEK_TRUSTED_PROXY_CIDRS:-172\.28\.0\.10\/32\}/u, 'API must trust only the fixed local gateway by default.')
requireMatch(serviceBlock(compose, 'public-web'), /dockerfile:\s*infra\/docker\/Dockerfile\.web/u, 'Public must have its own image build.')
requireMatch(serviceBlock(compose, 'admin-web'), /dockerfile:\s*infra\/docker\/Dockerfile\.admin/u, 'Admin must have its own image build.')
requireMatch(serviceBlock(compose, 'platform-api'), /dockerfile:\s*infra\/docker\/Dockerfile\.platform/u, 'API must have its own process image boundary.')

const debugCompose = read('compose.debug.yaml')
for (const [variable, hostPort, containerPort] of [
  ['AIRTEK_POSTGRES_DEBUG_PORT', 54320, 5432],
  ['AIRTEK_MINIO_DEBUG_PORT', 19000, 9000],
  ['AIRTEK_MINIO_CONSOLE_DEBUG_PORT', 19001, 9001],
]) {
  const mapping = `\\$\\{AIRTEK_BIND_ADDRESS:-127\\.0\\.0\\.1\\}:\\$\\{${variable}:-${hostPort}\\}:${containerPort}`
  requireMatch(debugCompose, new RegExp(mapping, 'u'), `Opt-in ${containerPort} diagnostic mapping must be configurable and loopback-only.`)
}

const productionCompose = read('compose.production.yaml')
forbidMatch(productionCompose, /^\s+build:\s*$/mu, 'Production Compose must promote immutable images instead of building from a checkout.')
for (const imageVariable of ['AIRTEK_PUBLIC_WEB_IMAGE', 'AIRTEK_ADMIN_WEB_IMAGE', 'AIRTEK_PLATFORM_IMAGE', 'AIRTEK_GATEWAY_IMAGE']) {
  requireMatch(
    productionCompose,
    new RegExp(`image:\\s*\\$\\{${imageVariable}:\\?`, 'u'),
    `Production Compose must require ${imageVariable}.`,
  )
}
forbidMatch(productionCompose, /image:\s*[^\n]+:latest\s*$/mu, 'Production Compose must not use a mutable latest image tag.')
for (const [service, port] of [['public-web', 3000], ['admin-web', 3100], ['platform-api', 8080]]) {
  const block = serviceBlock(productionCompose, service)
  requireMatch(block, new RegExp(`expose:[\\s\\S]*- "${port}"`, 'u'), `Production ${service} must expose fixed internal port ${port}.`)
  forbidMatch(block, /^\s+ports:/mu, `Production ${service} must not publish its internal port.`)
}
forbidMatch(serviceBlock(productionCompose, 'platform-worker'), /^\s+(?:ports|expose):/mu, 'Production Worker must not expose or publish a listening port.')
for (const service of ['platform-migrate', 'platform-api', 'platform-worker', 'public-web', 'admin-web']) {
  forbidMatch(serviceBlock(productionCompose, service), /^\s+ports:/mu, `Production ${service} must not publish an application port.`)
}
forbidMatch(serviceBlock(productionCompose, 'public-web'), /DATABASE_URL/u, 'Production Public SSR must not receive database credentials.')
forbidMatch(serviceBlock(productionCompose, 'admin-web'), /DATABASE_URL/u, 'Production Admin SPA must not receive database credentials.')
requireMatch(productionCompose, /AIRTEK_INGRESS_BIND_ADDRESS:-127\.0\.0\.1/u, 'Production gateway must bind its outer-ingress listener to loopback by default.')
const productionMigrationWaits = productionCompose.match(/condition:\s*service_completed_successfully/gu)?.length ?? 0
if (productionMigrationWaits < 2) failures.push('Production API and Worker must wait for a successful migration service.')

const productionEnv = read('infra/deploy/production.env.example')
for (const origin of ['AIRTEK_PUBLIC_ORIGIN=https://', 'AIRTEK_ADMIN_ORIGIN=https://', 'AIRTEK_API_ORIGIN=https://']) {
  requireMatch(productionEnv, new RegExp(`^${origin}`, 'mu'), `Production environment example must configure ${origin.split('=')[0]} as HTTPS.`)
}
requireMatch(productionCompose, /AIRTEK_TOTP_ENCRYPTION_KEY:\s*\$\{AIRTEK_TOTP_ENCRYPTION_KEY:\?/u, 'Production must require a secret-manager TOTP encryption key.')
requireMatch(productionEnv, /^AIRTEK_TOTP_ENCRYPTION_KEY=REPLACE_/mu, 'Production environment example must declare the TOTP key placeholder.')
for (const service of ['platform-api', 'platform-worker']) {
  requireMatch(serviceBlock(productionCompose, service), /AIRTEK_PREVIEW_SIGNING_KEY:\s*\$\{AIRTEK_PREVIEW_SIGNING_KEY:\?/u, `Production ${service} must require a secret-manager preview signing key.`)
}
requireMatch(productionEnv, /^AIRTEK_PREVIEW_SIGNING_KEY=REPLACE_/mu, 'Production environment example must declare the preview signing key placeholder.')
for (const variable of ['AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY', 'AIRTEK_ANALYTICS_TOKEN_HMAC_KEY', 'AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY']) {
  requireMatch(productionCompose, new RegExp(`${variable}:\\s*\\$\\{${variable}:\\?`, 'u'), `Production must require ${variable} from the secret manager.`)
  requireMatch(productionEnv, new RegExp(`^${variable}=REPLACE_`, 'mu'), `Production environment example must declare ${variable}.`)
}
for (const variable of ['AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION', 'AIRTEK_APPROVED_PRODUCT_MASTER_SHA256', 'AIRTEK_APPROVED_PRODUCT_MASTER_MAPPING_VERSION', 'AIRTEK_APPROVED_PRODUCT_MASTER_VALID_ROWS', 'AIRTEK_APPROVED_PRODUCT_MASTER_ERROR_ROWS']) {
  for (const service of ['platform-api', 'platform-worker']) {
    requireMatch(serviceBlock(productionCompose, service), new RegExp(`${variable}:\\s*\\$\\{${variable}:\\?`, 'u'), `Production ${service} must require ${variable}.`)
  }
  requireMatch(productionEnv, new RegExp(`^${variable}=`, 'mu'), `Production environment example must declare ${variable}.`)
}
for (const variable of ['AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES', 'AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS', 'AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS']) {
  requireMatch(serviceBlock(productionCompose, 'platform-api'), new RegExp(`${variable}:\\s*\\$\\{${variable}`, 'u'), `Production API must receive ${variable}.`)
  requireMatch(productionEnv, new RegExp(`^${variable}=`, 'mu'), `Production environment example must declare ${variable}.`)
}

const localEnvExample = read('.env.example')
for (const variable of [
  'AIRTEK_PUBLIC_HOST_PORT',
  'AIRTEK_ADMIN_HOST_PORT',
  'AIRTEK_API_HOST_PORT',
  'AIRTEK_GATEWAY_HOST_PORT',
  'AIRTEK_POSTGRES_DEBUG_PORT',
  'POSTGRES_DB',
  'POSTGRES_USER',
  'POSTGRES_PASSWORD',
  'AIRTEK_DATABASE_URL_INTERNAL',
  'DATABASE_URL',
  'AIRTEK_TOTP_ENCRYPTION_KEY',
  'AIRTEK_PREVIEW_SIGNING_KEY',
  'AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY',
  'AIRTEK_ANALYTICS_TOKEN_HMAC_KEY',
  'AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY',
  'AIRTEK_GUEST_RAW_RETENTION_DAYS',
  'AIRTEK_GUEST_AGGREGATE_RETENTION_MONTHS',
  'AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION',
  'AIRTEK_APPROVED_PRODUCT_MASTER_SHA256',
  'AIRTEK_APPROVED_PRODUCT_MASTER_MAPPING_VERSION',
  'AIRTEK_APPROVED_PRODUCT_MASTER_VALID_ROWS',
  'AIRTEK_APPROVED_PRODUCT_MASTER_ERROR_ROWS',
  'AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES',
  'AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS',
  'AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS',
  'AIRTEK_TRUSTED_PROXY_CIDRS',
  'RUST_LOG',
]) {
  requireMatch(localEnvExample, new RegExp(`^${variable}=`, 'mu'), `.env.example must document ${variable}.`)
}

const adminNginx = read('apps/admin/deploy/nginx.conf')
for (const header of [
  'X-Robots-Tag',
  'X-Content-Type-Options',
  'X-Frame-Options',
  'Referrer-Policy',
  'Permissions-Policy',
  'Content-Security-Policy',
]) {
  requireMatch(adminNginx, new RegExp(`add_header\\s+${escapeRegExp(header)}\\s`, 'u'), `Admin Nginx is missing ${header}.`)
}
requireMatch(adminNginx, /site\\\.webmanifest/u, 'Admin Nginx must return 404 for public manifests.')
requireMatch(adminNginx, /google\[\^\/\]\+\\\.html/u, 'Admin Nginx must return 404 for webmaster verification files.')
requireMatch(adminNginx, /location\s+~\*\s+\^\/sitemap/u, 'Admin Nginx must reject every sitemap variant before SPA fallback.')
requireMatch(adminNginx, /location\s+~\*\s+\^\/en/u, 'Admin Nginx must reject public locale routes before SPA fallback.')
if ((adminNginx.match(/add_header\s+X-Robots-Tag/gu)?.length ?? 0) !== 1) {
  failures.push('Admin security headers must be declared once at server scope to avoid Nginx inheritance loss.')
}

const gateway = read('infra/gateway/nginx.conf.template')
requireMatch(gateway, /listen\s+8088\s+default_server/u, 'Gateway must reject unknown Host values through an unprivileged default server.')
requireMatch(gateway, /location\s+\^~\s+\/api\/devtools\/\s*\{\s*return\s+404;/u, 'Gateway must reject the production DevTools namespace.')
requireMatch(gateway, /location\s+\^~\s+\/admin\/\s*\{\s*return\s+404;/u, 'Public gateway must reject the Admin namespace.')
requireMatch(gateway, /location\s+=\s+\/en\/preview\s*\{[\s\S]*?access_log\s+off;/u, 'Public gateway must not log signed preview query tokens.')
requireMatch(gateway, /location\s+\^~\s+\/en\/\s*\{\s*return\s+404;/u, 'Admin gateway must reject public locale routes.')
requireMatch(gateway, /site\\\.webmanifest/u, 'Gateway must block admin public manifests.')
requireMatch(gateway, /return\s+200\s+"User-agent:\s*\*\\nDisallow:\s*\/\\n"/u, 'API gateway robots.txt must disallow all crawling.')
requireMatch(gateway, /Content-Security-Policy/u, 'Gateway must set per-origin CSP headers.')
if ((gateway.match(/add_header\s+Content-Security-Policy/gu)?.length ?? 0) !== 4) {
  failures.push('Every gateway server block, including the unknown-Host boundary, must define a CSP.')
}
if ((gateway.match(/proxy_hide_header\s+Content-Security-Policy/gu)?.length ?? 0) < 2) {
  failures.push('Gateway must replace upstream Public/Admin CSP values instead of intersecting duplicate policies.')
}
forbidMatch(gateway, /proxy_set_header\s+(?:Upgrade|Connection)/iu, 'Production gateway must not configure a WebSocket upstream.')

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Deployment configuration checks passed.')
