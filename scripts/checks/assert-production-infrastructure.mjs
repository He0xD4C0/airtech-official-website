import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '../..')
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

const infrastructure = read('infra/compose/production.infrastructure.yaml')
const application = read('infra/compose/production.app.yaml')
const infrastructureEnv = read('infra/deploy/infrastructure.env.example')
const productionEnv = read('infra/deploy/production.env.example')
const deployScript = read('infra/deploy/deploy-app.sh')

requireMatch(infrastructure, /^name: \$\{AIRTEK_INFRA_PROJECT_NAME:-airtek-infra\}$/mu, 'Infrastructure must have an independent Compose project name.')
requireMatch(serviceBlock(infrastructure, 'postgres'), /restart: unless-stopped/u, 'PostgreSQL must survive an ECS restart.')
const postgresBootstrap = serviceBlock(infrastructure, 'postgres-bootstrap')
requireMatch(postgresBootstrap, /profiles: \["bootstrap"\]/u, 'PostgreSQL bootstrap must be explicitly opt-in.')
requireMatch(postgresBootstrap, /restart: "no"/u, 'PostgreSQL bootstrap must remain a one-shot task.')
forbidMatch(infrastructure, /^  (?:minio|minio-bootstrap|platform-api|platform-worker|public-web|admin-web|gateway|flyway-migrate):/mu, 'Infrastructure Compose must own only PostgreSQL and its bootstrap task.')
requireMatch(infrastructure, /external: true\s+name: \$\{AIRTEK_PRODUCTION_NETWORK:-airtek-production\}/u, 'Infrastructure must join the shared external network.')
requireMatch(infrastructure, /AIRTEK_INFRA_BIND_ADDRESS:-127\.0\.0\.1/u, 'Stateful administration ports must bind to loopback by default.')

requireMatch(application, /^name: \$\{AIRTEK_APP_PROJECT_NAME:-airtek-app\}$/mu, 'Application must have an independent Compose project name.')
forbidMatch(application, /^  (?:postgres|minio|minio-bootstrap):/mu, 'Application Compose must not own PostgreSQL or MinIO.')
requireMatch(application, /external: true\s+name: \$\{AIRTEK_PRODUCTION_NETWORK:-airtek-production\}/u, 'Application must join the shared external network.')
const storageEnvironmentPattern = /(?:AIRTEK_MEDIA_|AIRTEK_MINIO_|MINIO_|S3_)/u
forbidMatch(application, storageEnvironmentPattern, 'Application Compose must load object-storage settings from PostgreSQL.')
forbidMatch(productionEnv, storageEnvironmentPattern, 'production.env.example must not contain object-storage settings.')
forbidMatch(infrastructure, storageEnvironmentPattern, 'Production infrastructure Compose must not provision application object storage.')
forbidMatch(infrastructureEnv, storageEnvironmentPattern, 'infrastructure.env.example must not contain object-storage settings.')

for (const variable of [
  'AIRTEK_POSTGRES_IMAGE',
  'POSTGRES_SUPERUSER_PASSWORD',
  'AIRTEK_RUNTIME_DATABASE_PASSWORD',
]) {
  requireMatch(infrastructureEnv, new RegExp(`^${variable}=`, 'mu'), `infrastructure.env.example must declare ${variable}.`)
}

forbidMatch(deployScript, /docker compose[^\n]*(?:down|-v|volume rm)/u, 'Application deployment must never tear down or delete stateful storage.')
requireMatch(deployScript, /run --rm flyway-migrate migrate/u, 'Application deployment must migrate before rollout.')
requireMatch(deployScript, /run --rm flyway-migrate validate/u, 'Application deployment must validate migration history.')
requireMatch(deployScript, /run --rm --no-deps public-readiness/u, 'Application deployment must run the public-site readiness gate.')
requireMatch(deployScript, /restoring previous application images/u, 'Application deployment must attempt image rollback after a failed rollout.')

if (failures.length > 0) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Production infrastructure separation checks passed.')
