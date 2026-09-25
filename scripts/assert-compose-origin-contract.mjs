import { readFileSync } from 'node:fs'

const compose = readFileSync(new URL('../compose.yaml', import.meta.url), 'utf8')
const environment = readFileSync(new URL('../.env.example', import.meta.url), 'utf8')
const e2eStack = readFileSync(new URL('./run-e2e-stack.mjs', import.meta.url), 'utf8')
const e2eCompose = readFileSync(new URL('../compose.e2e.yaml', import.meta.url), 'utf8')
const failures = []

function requireText(body, expected, detail) {
  if (!body.includes(expected)) failures.push(detail)
}

for (const [name, value] of [
  ['AIRTEK_COMPOSE_PUBLIC_ORIGIN', 'http://www.localhost:8088'],
  ['AIRTEK_COMPOSE_ADMIN_ORIGIN', 'http://admin.localhost:8088'],
  ['AIRTEK_COMPOSE_API_ORIGIN', 'http://api.localhost:8088'],
]) {
  requireText(environment, `${name}=${value}`, `.env.example must declare ${name}=${value}.`)
}

for (const expected of [
  'AIRTEK_PUBLIC_ORIGIN: ${AIRTEK_COMPOSE_PUBLIC_ORIGIN:-http://www.localhost:8088}',
  'AIRTEK_ADMIN_ORIGIN: ${AIRTEK_COMPOSE_ADMIN_ORIGIN:-http://admin.localhost:8088}',
  'VITE_PUBLIC_API_BASE_URL: ${AIRTEK_COMPOSE_API_ORIGIN:-http://api.localhost:8088}/api/public/v1',
  'VITE_PUBLIC_ORIGIN: ${AIRTEK_COMPOSE_PUBLIC_ORIGIN:-http://www.localhost:8088}',
  'PUBLIC_API_BROWSER_ORIGIN: ${AIRTEK_COMPOSE_API_ORIGIN:-http://api.localhost:8088}',
  'PUBLIC_ORIGIN: ${AIRTEK_COMPOSE_PUBLIC_ORIGIN:-http://www.localhost:8088}',
  'VITE_ADMIN_API_BASE_URL: ${AIRTEK_COMPOSE_API_ORIGIN:-http://api.localhost:8088}/api/admin/v1',
  'ADMIN_API_ORIGIN: ${AIRTEK_COMPOSE_API_ORIGIN:-http://api.localhost:8088}',
  'API_BROWSER_ORIGIN: ${AIRTEK_COMPOSE_API_ORIGIN:-http://api.localhost:8088}',
]) {
  requireText(compose, expected, `Local Compose origin contract is missing: ${expected}`)
}

for (const stale of ['http://localhost:3000', 'http://localhost:3100', 'http://localhost:8080']) {
  if (compose.includes(stale)) failures.push(`Complete Compose must not use the native-development origin ${stale}.`)
}

for (const name of [
  'AIRTEK_COMPOSE_PUBLIC_ORIGIN: publicOrigin',
  'AIRTEK_COMPOSE_ADMIN_ORIGIN: adminOrigin',
  'AIRTEK_COMPOSE_API_ORIGIN: apiOrigin',
]) {
  requireText(e2eStack, name, `The isolated E2E stack must provide ${name.split(':')[0]}.`)
}
requireText(e2eStack, 'compose.yaml:compose.e2e.yaml', 'The E2E runner must load its bootstrap-only Compose override.')
requireText(e2eCompose, '3000/robots.txt', 'The E2E override must expose the pre-publication bootstrap probe.')

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}
console.log('Compose browser, SSR, CORS and gateway origins share the reviewed three-origin contract.')
