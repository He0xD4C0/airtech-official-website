import { existsSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = dirname(dirname(dirname(fileURLToPath(import.meta.url))))
const failures = []

function read(path) {
  const absolute = join(root, path)
  if (!existsSync(absolute)) {
    failures.push(`${path} is missing.`)
    return ''
  }
  return readFileSync(absolute, 'utf8')
}

function requireMatch(body, pattern, failure) {
  if (!pattern.test(body)) failures.push(failure)
}

function forbidMatch(body, pattern, failure) {
  if (pattern.test(body)) failures.push(failure)
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

const localCompose = read('compose.yaml')
const productionCompose = read('infra/compose/production.app.yaml')
const dockerfile = read('infra/docker/Dockerfile.flyway')
const entrypoint = read('infra/docker/flyway-entrypoint.sh')
const wrapper = read('infra/docker/flyway-release.sh')
const deploy = read('infra/deploy/deploy-app.sh')

requireMatch(
  serviceBlock(localCompose, 'flyway-migrate'),
  /command:\s*\["release"\]/u,
  'Local Compose must run the migration release wrapper.',
)
requireMatch(
  serviceBlock(localCompose, 'flyway-migrate'),
  /migration-state:\/var\/lib\/airtek\/migration-state/u,
  'Local Compose must mount the migration state directory.',
)
requireMatch(
  serviceBlock(productionCompose, 'flyway-migrate'),
  /command:\s*\["release"\]/u,
  'Production Compose must run the migration release wrapper.',
)
requireMatch(
  serviceBlock(productionCompose, 'flyway-migrate'),
  /AIRTEK_MIGRATION_STATE_DIR:-/u,
  'Production Compose must mount a configurable migration state directory.',
)
requireMatch(
  serviceBlock(productionCompose, 'flyway-migrate'),
  /AIRTEK_RELEASE_TAG:\s*\$\{AIRTEK_RELEASE_TAG:\?/u,
  'Production Compose must require the release tag for failure records.',
)

requireMatch(dockerfile, /apk add --no-cache postgresql17-client/u, 'Flyway image must install the PostgreSQL 17 client.')
requireMatch(dockerfile, /COPY\s+--chmod=0555\s+infra\/docker\/flyway-release\.sh/u, 'Flyway image must package the release wrapper.')
requireMatch(dockerfile, /install -d -o 10001 -g 10001/u, 'Flyway image must create its migration state directory for the runtime UID.')
requireMatch(entrypoint, /baseline\|migrate\|info\|validate\|release/u, 'Flyway entrypoint must expose exactly the reviewed command set.')
requireMatch(entrypoint, /exec \/usr\/local\/bin\/airtek-flyway-release/u, 'Flyway entrypoint must dispatch release to the wrapper.')

for (const marker of [
  /pg_dump/,
  /pg_restore/,
  /pg_terminate_backend/,
  /operation_runs/,
  /migrationApply/,
  /\$restore_succeeded" != "true"/,
]) {
  requireMatch(wrapper, marker, `Release wrapper is missing required behavior ${marker.source}.`)
}
requireMatch(wrapper, /exit_code=2/, 'Release wrapper must report a restored migration failure as exit code 2.')
requireMatch(wrapper, /exit_code=3/, 'Release wrapper must report an unrecoverable failure as exit code 3.')
forbidMatch(deploy, /run --rm flyway-migrate/u, 'Deployment must let the Compose dependency graph own migration execution.')
forbidMatch(deploy, /run --rm platform-maintenance/u, 'Deployment must let the Compose dependency graph own runtime preparation.')
requireMatch(deploy, /AIRTEK_RELEASE_TAG=\$release_id/u, 'Deployment must stamp the release tag for migration failure records.')

const localEnvironment = read('.env.example')
const productionEnvironment = read('infra/deploy/production.env.example')
const tagRelease = read('.github/workflows/ci.yml')
requireMatch(tagRelease, /suffix=-dev/u, 'The development tag stream must publish into a separate -dev package family.')
requireMatch(
  tagRelease,
  /PACKAGE_NAME: airtekpower-\$\{\{ matrix\.image \}\}\$\{\{ needs\.release-metadata\.outputs\.image_suffix \}\}/u,
  'Retention pruning must stay inside the publishing stream package family.',
)
for (const variable of [
  'AIRTEK_REGISTRY',
  'AIRTEK_REGISTRY_OWNER',
  'AIRTEK_IMAGE_PREFIX',
  'AIRTEK_TAG_PREFIX',
  'AIRTEK_REGISTRY_USERNAME',
  'AIRTEK_REGISTRY_TOKEN',
]) {
  requireMatch(localEnvironment, new RegExp(`^${variable}=`, 'mu'), `.env.example must declare ${variable}.`)
  requireMatch(productionEnvironment, new RegExp(`^${variable}=`, 'mu'), `Production environment example must declare ${variable}.`)
}

const secretBearingFiles = [
  '.env.example',
  '.github/workflows/ci.yml',
  'Jenkinsfile',
  'compose.yaml',
  'infra/compose/production.app.yaml',
  'infra/compose/production.infrastructure.yaml',
  'infra/deploy/deploy-app.sh',
  'infra/deploy/infrastructure.env.example',
  'infra/deploy/production.env.example',
  'infra/jenkins/jenkins.yaml',
]
const tokenPattern = new RegExp(`(?:${['gh', 'p_'].join('')}|${['gh', 'o_'].join('')}|${['github', '_pat_'].join('')}|${['gh', 's_'].join('')})[A-Za-z0-9]{20,}`, 'u')
for (const path of secretBearingFiles) {
  forbidMatch(read(path), tokenPattern, `${path} must never contain a real registry or repository token.`)
}

const agentDocs = read('AGENTS.md')
requireMatch(agentDocs, /-- airtek:destructive: <理由>/u, 'AGENTS.md must document the destructive migration marker.')
requireMatch(agentDocs, /expand\/contract/u, 'AGENTS.md must require backward-compatible migrations.')

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Migration release, failure record and registry configuration checks passed.')
