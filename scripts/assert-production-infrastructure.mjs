import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const read = (path) => readFileSync(resolve(root, path), 'utf8')
const failures = []

function requireMatch(text, pattern, message) {
  if (!pattern.test(text)) failures.push(message)
}

function forbidMatch(text, pattern, message) {
  if (pattern.test(text)) failures.push(message)
}

function serviceBlock(compose, service) {
  const match = compose.match(new RegExp(`^  ${service}:\\s*$([\\s\\S]*?)(?=^  [a-z0-9-]+:\\s*$|^networks:|(?![\\s\\S]))`, 'mu'))
  if (!match) {
    failures.push(`Missing service ${service}.`)
    return ''
  }
  return match[1]
}

const infrastructure = read('compose.infrastructure.production.yaml')
const application = read('compose.production.yaml')
const infrastructureEnv = read('infra/deploy/infrastructure.env.example')
const productionEnv = read('infra/deploy/production.env.example')
const deployScript = read('infra/deploy/deploy-app.sh')

requireMatch(infrastructure, /^name: \$\{AIRTEK_INFRA_PROJECT_NAME:-airtek-infra\}$/mu, 'Infrastructure must have an independent Compose project name.')
for (const service of ['postgres', 'minio']) {
  requireMatch(serviceBlock(infrastructure, service), /restart: unless-stopped/u, `${service} must survive an ECS restart.`)
}
for (const service of ['postgres-bootstrap', 'minio-bootstrap']) {
  const block = serviceBlock(infrastructure, service)
  requireMatch(block, /profiles: \["bootstrap"\]/u, `${service} must be explicitly opt-in.`)
  requireMatch(block, /restart: "no"/u, `${service} must remain a one-shot task.`)
}
forbidMatch(infrastructure, /^  (?:platform-api|platform-worker|public-web|admin-web|gateway|flyway-migrate):/mu, 'Infrastructure Compose must not own application services.')
requireMatch(infrastructure, /external: true\s+name: \$\{AIRTEK_PRODUCTION_NETWORK:-airtek-production\}/u, 'Infrastructure must join the shared external network.')
requireMatch(infrastructure, /AIRTEK_INFRA_BIND_ADDRESS:-127\.0\.0\.1/u, 'Stateful administration ports must bind to loopback by default.')

requireMatch(application, /^name: \$\{AIRTEK_APP_PROJECT_NAME:-airtek-app\}$/mu, 'Application must have an independent Compose project name.')
forbidMatch(application, /^  (?:postgres|minio|minio-bootstrap):/mu, 'Application Compose must not own PostgreSQL or MinIO.')
requireMatch(application, /external: true\s+name: \$\{AIRTEK_PRODUCTION_NETWORK:-airtek-production\}/u, 'Application must join the shared external network.')
for (const variable of [
  'AIRTEK_MEDIA_STORAGE',
  'AIRTEK_MEDIA_S3_ENDPOINT',
  'AIRTEK_MEDIA_S3_BUCKET',
]) {
  requireMatch(serviceBlock(application, 'platform-api'), new RegExp(`${variable}:\\s*\\$\\{${variable}`, 'u'), `Production API must receive ${variable}.`)
  requireMatch(productionEnv, new RegExp(`^${variable}=`, 'mu'), `production.env.example must declare ${variable}.`)
}
for (const field of ['ACCESS_KEY_ID', 'SECRET_ACCESS_KEY']) {
  requireMatch(serviceBlock(application, 'platform-api'), new RegExp(`AIRTEK_MEDIA_S3_${field}:\\s*\\$\\{AIRTEK_MEDIA_API_S3_${field}:\\?`, 'u'), `Production API must require its MinIO ${field}.`)
  requireMatch(productionEnv, new RegExp(`^AIRTEK_MEDIA_API_S3_${field}=`, 'mu'), `production.env.example must declare the API MinIO ${field}.`)
  requireMatch(serviceBlock(infrastructure, 'minio-bootstrap'), new RegExp(`AIRTEK_MEDIA_S3_${field}:\\s*\\$\\{AIRTEK_MEDIA_API_S3_${field}:\\?`, 'u'), `MinIO bootstrap must receive the same API ${field}.`)
  requireMatch(infrastructureEnv, new RegExp(`^AIRTEK_MEDIA_API_S3_${field}=`, 'mu'), `infrastructure.env.example must declare the API MinIO ${field}.`)
}

for (const variable of [
  'AIRTEK_POSTGRES_IMAGE',
  'AIRTEK_MINIO_IMAGE',
  'AIRTEK_MINIO_MC_IMAGE',
  'POSTGRES_SUPERUSER_PASSWORD',
  'AIRTEK_RUNTIME_DATABASE_PASSWORD',
  'MINIO_ROOT_PASSWORD',
  'AIRTEK_MEDIA_API_S3_SECRET_ACCESS_KEY',
]) {
  requireMatch(infrastructureEnv, new RegExp(`^${variable}=`, 'mu'), `infrastructure.env.example must declare ${variable}.`)
}

forbidMatch(deployScript, /docker compose[^\n]*(?:down|-v|volume rm)/u, 'Application deployment must never tear down or delete stateful storage.')
requireMatch(deployScript, /run --rm flyway-migrate migrate/u, 'Application deployment must migrate before rollout.')
requireMatch(deployScript, /run --rm flyway-migrate validate/u, 'Application deployment must validate migration history.')
requireMatch(deployScript, /restoring previous application images/u, 'Application deployment must attempt image rollback after a failed rollout.')

if (failures.length > 0) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Production infrastructure separation checks passed.')
