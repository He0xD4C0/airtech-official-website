import { readFileSync } from 'node:fs'

const compose = readFileSync(new URL('../compose.yaml', import.meta.url), 'utf8')
const environment = readFileSync(new URL('../.env.example', import.meta.url), 'utf8')
const e2eStack = readFileSync(new URL('./run-e2e-stack.mjs', import.meta.url), 'utf8')
const isolation = readFileSync(new URL('./isolated-test-environment.mjs', import.meta.url), 'utf8')
const e2eCompose = readFileSync(new URL('../compose.e2e.yaml', import.meta.url), 'utf8')
const emptyProjection = readFileSync(new URL('./assert-empty-public-projection.mjs', import.meta.url), 'utf8')
const failures = []

function requireText(body, expected, detail) {
  if (!body.includes(expected)) failures.push(detail)
}

for (const [name, value] of [
  ['AIRTEK_COMPOSE_PUBLIC_ORIGIN', 'http://www.airtek.localhost:8088'],
  ['AIRTEK_COMPOSE_ADMIN_ORIGIN', 'http://admin.airtek.localhost:8088'],
  ['AIRTEK_COMPOSE_API_ORIGIN', 'http://api.airtek.localhost:8088'],
]) {
  requireText(environment, `${name}=${value}`, `.env.example must declare ${name}=${value}.`)
}

for (const expected of [
  'AIRTEK_PUBLIC_ORIGIN: ${AIRTEK_COMPOSE_PUBLIC_ORIGIN:-http://www.airtek.localhost:8088}',
  'AIRTEK_ADMIN_ORIGIN: ${AIRTEK_COMPOSE_ADMIN_ORIGIN:-http://admin.airtek.localhost:8088}',
  'VITE_PUBLIC_API_BASE_URL: ${AIRTEK_COMPOSE_API_ORIGIN:-http://api.airtek.localhost:8088}/api/public/v1',
  'VITE_PUBLIC_ORIGIN: ${AIRTEK_COMPOSE_PUBLIC_ORIGIN:-http://www.airtek.localhost:8088}',
  'PUBLIC_API_BROWSER_ORIGIN: ${AIRTEK_COMPOSE_API_ORIGIN:-http://api.airtek.localhost:8088}',
  'PUBLIC_ORIGIN: ${AIRTEK_COMPOSE_PUBLIC_ORIGIN:-http://www.airtek.localhost:8088}',
  'VITE_ADMIN_API_BASE_URL: ${AIRTEK_COMPOSE_API_ORIGIN:-http://api.airtek.localhost:8088}/api/admin/v1',
  'ADMIN_API_ORIGIN: ${AIRTEK_COMPOSE_API_ORIGIN:-http://api.airtek.localhost:8088}',
  'API_BROWSER_ORIGIN: ${AIRTEK_COMPOSE_API_ORIGIN:-http://api.airtek.localhost:8088}',
  'AIRTEK_DEV_PUBLIC_SEED: ${AIRTEK_DEV_PUBLIC_SEED:-false}',
  "fetch('http://127.0.0.1:3000/healthz')",
]) {
  requireText(compose, expected, `Local Compose origin contract is missing: ${expected}`)
}

for (const stale of ['http://localhost:3000', 'http://localhost:3100', 'http://localhost:8080']) {
  if (compose.includes(stale)) failures.push(`Complete Compose must not use native-development origin ${stale}.`)
}
for (const name of [
  'AIRTEK_COMPOSE_PUBLIC_ORIGIN: publicOrigin',
  'AIRTEK_COMPOSE_ADMIN_ORIGIN: adminOrigin',
  'AIRTEK_COMPOSE_API_ORIGIN: apiOrigin',
  'E2E_API_GATEWAY_HOST:',
]) {
  requireText(e2eStack, name, `The isolated E2E stack must provide ${name.split(':')[0]}.`)
}
requireText(isolation, 'compose.yaml:compose.e2e.yaml', 'The E2E runner must load its bootstrap-only Compose override.')
requireText(e2eStack, '...isolation', 'The E2E runner must apply the database isolation contract.')
requireText(e2eStack, "run('node', ['scripts/assert-empty-public-projection.mjs'])", 'The E2E runner must verify fail-closed behavior before publishing fixtures.')
requireText(e2eStack, "'platform-maintenance', 'check-public-readiness'", 'The E2E runner must finish with the production-style readiness gate.')
requireText(e2eCompose, 'AIRTEK_DEV_PUBLIC_SEED: !reset null', 'Production-image E2E must omit seed configuration.')
for (const expected of ['/en', '/api/public/v1/site-bootstrap', 'Disallow: /', '/sitemap.xml', '/site-icon', '/site.webmanifest']) {
  requireText(emptyProjection, expected, `The empty-projection acceptance check is missing ${expected}.`)
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}
console.log('Compose browser, SSR, CORS and gateway origins share the reviewed three-origin contract.')
