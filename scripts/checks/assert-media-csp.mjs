import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '../..')
const read = (path) => readFileSync(join(root, path), 'utf8')
const failures = []
const localSources = 'https: http://media.localhost:19000 http://localhost:19000'
const composeSources = 'https: ${AIRTEK_COMPOSE_MEDIA_ORIGIN:-http://media.localhost:19000} http://localhost:${AIRTEK_OBJECT_STORE_HOST_PORT:-19000}'

for (const [label, body, expected] of [
  ['Admin development CSP', read('apps/admin/vite.config.ts'), `DEVELOPMENT_MEDIA_IMAGE_SOURCES = '${localSources}'`],
  ['Local Admin Compose', read('compose.yaml'), `ADMIN_MEDIA_IMAGE_SOURCES: "${composeSources}"`],
  ['Local gateway Compose', read('compose.yaml'), `MEDIA_IMAGE_SOURCES: "${composeSources}"`],
  ['Local API host mapping', read('compose.yaml'), 'media.localhost:host-gateway'],
  ['Production Admin Compose', read('infra/compose/production.app.yaml'), 'ADMIN_MEDIA_IMAGE_SOURCES: "${AIRTEK_API_ORIGIN:?set AIRTEK_API_ORIGIN} ${PUBLIC_MEDIA_ORIGIN:-https:}"'],
  ['Production gateway Compose', read('infra/compose/production.app.yaml'), 'MEDIA_IMAGE_SOURCES: "${AIRTEK_API_ORIGIN:?set AIRTEK_API_ORIGIN} ${PUBLIC_MEDIA_ORIGIN:-https:}"'],
  ['Admin Nginx template', read('apps/admin/deploy/nginx.conf'), "img-src 'self' data: blob: ${ADMIN_MEDIA_IMAGE_SOURCES}"],
]) {
  if (!body.includes(expected)) failures.push(`${label} is missing ${expected}`)
}

const gatewayTemplate = read('infra/gateway/nginx.conf.template')
const renderedMediaPolicies = gatewayTemplate.match(/img-src 'self' data: blob: \$\{MEDIA_IMAGE_SOURCES\}/gu)?.length ?? 0
if (renderedMediaPolicies !== 2) failures.push('Public and Admin gateway CSPs must render MEDIA_IMAGE_SOURCES.')

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Media CSP deployment checks passed.')
