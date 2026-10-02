import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(dirname(dirname(fileURLToPath(import.meta.url)))))
const read = (path) => readFileSync(join(root, path), 'utf8')
const failures = []

function requireMatch(path, pattern, message) {
  if (!pattern.test(read(path))) failures.push(`${path}: ${message}`)
}

const productionCompose = 'infra/compose/production.app.yaml'
const productionEnv = 'infra/deploy/production.env.example'
const localEnv = '.env.example'

for (const variable of ['AIRTEK_ADMIN_EMAIL', 'AIRTEK_ADMIN_DISPLAY_NAME', 'AIRTEK_ADMIN_PASSWORD']) {
  requireMatch(
    productionCompose,
    new RegExp(`${variable}:\\s*\\$\\{${variable}:\\?`, 'u'),
    `production must require ${variable}`,
  )
  requireMatch(
    productionEnv,
    new RegExp(`^${variable}=`, 'mu'),
    `production environment example must declare ${variable}`,
  )
}
requireMatch(
  productionCompose,
  /AIRTEK_ADMIN_RECOVERY_KEY_DIR:\s*\/var\/lib\/airtek\/admin-recovery-key/u,
  'API and maintenance must read the recovery key from the fixed container path',
)
requireMatch(
  productionCompose,
  /\$\{AIRTEK_ADMIN_RECOVERY_KEY_DIR:-\/srv\/airtek\/keys\/admin-recovery-key\}:\/var\/lib\/airtek\/admin-recovery-key:Z/u,
  'production must mount the administrator recovery key directory',
)
for (const variable of [
  'AIRTEK_ADMIN_RECOVERY_KEY_DIR',
  'AIRTEK_ADMIN_RECOVERY_KEY_MODE',
]) {
  requireMatch(productionEnv, new RegExp(`^${variable}=`, 'mu'), `production environment example must declare ${variable}`)
  requireMatch(localEnv, new RegExp(`^${variable}=`, 'mu'), `.env.example must document ${variable}`)
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'))
  process.exit(1)
}
console.log('Administrator provisioning configuration checks passed.')
