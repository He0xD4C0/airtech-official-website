import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const root = dirname(dirname(dirname(fileURLToPath(import.meta.url))))
const failures = []

function read(path) {
  return readFileSync(join(root, path), 'utf8')
}

function requireIncludes(path, body, text, detail) {
  if (!body.includes(text)) failures.push(`${path}: ${detail}`)
}

function requireMatch(path, body, pattern, detail) {
  if (!pattern.test(body)) failures.push(`${path}: ${detail}`)
}

function forbidMatch(path, body, pattern, detail) {
  if (pattern.test(body)) failures.push(`${path}: ${detail}`)
}

const productionCompose = read('infra/compose/production.app.yaml')
const productionEnvironment = read('infra/deploy/production.env.example')
const platformDockerfile = read('infra/docker/Dockerfile.platform')
const environmentConfig = read('services/platform/crates/runtime/src/config/environment.rs')
const maintenance = read('services/platform/apps/maintenance/src/main.rs')
const promotion = read('services/platform/crates/runtime/src/services/feishu/promotion.rs')
const productStorage = read('services/platform/crates/runtime/src/state/product_storage.rs')

for (const [path, body] of [
  ['infra/compose/production.app.yaml', productionCompose],
  ['infra/deploy/production.env.example', productionEnvironment],
]) {
  forbidMatch(
    path,
    body,
    /AIRTEK_DEV_(?:ADMIN|PUBLIC)_/u,
    'must not expose any development seed or password-only switch',
  )
  forbidMatch(
    path,
    body,
    /prepare-development-runtime|reset-development-admin|local-admin@airtek\.invalid/u,
    'must not reference development fixture provisioning or fixed credentials',
  )
}

requireMatch(
  'infra/compose/production.app.yaml',
  productionCompose,
  /platform-maintenance:[\s\S]*?command:\s*\["prepare-runtime"\]/u,
  'production maintenance must execute prepare-runtime only',
)
requireMatch(
  'infra/docker/Dockerfile.platform',
  platformDockerfile,
  /ARG\s+AIRTEK_PLATFORM_FEATURES=production/u,
  'platform image must default to the production feature set',
)

for (const name of ['AIRTEK_DEV_PUBLIC_SEED']) {
  requireIncludes(
    'services/platform/crates/runtime/src/config/environment.rs',
    environmentConfig,
    name,
    `runtime must reject ${name} without the devtools feature`,
  )
}
requireIncludes(
  'services/platform/apps/maintenance/src/main.rs',
  maintenance,
  'reject_development_seed_configuration()',
  'maintenance must reject development seeds before connecting to PostgreSQL',
)
requireIncludes(
  'services/platform/apps/maintenance/src/main.rs',
  maintenance,
  'Some("prepare-runtime")',
  'maintenance must expose a production-only preparation path',
)

for (const [path, body] of [
  ['services/platform/crates/runtime/src/services/feishu/promotion.rs', promotion],
  ['services/platform/crates/runtime/src/state/product_storage.rs', productStorage],
]) {
  requireMatch(
    path,
    body,
    /developmentFixture[\s\S]{0,200}cannot be taken over/u,
    'development fixtures must be rejected by every Feishu takeover path',
  )
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}

console.log('Production seed isolation and bootstrap contract passed.')
