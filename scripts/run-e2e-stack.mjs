import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'

const project = process.env.E2E_COMPOSE_PROJECT_NAME ?? 'airtekpower-e2e'
const gatewayPort = process.env.AIRTEK_E2E_GATEWAY_PORT ?? '8088'
const publicPort = process.env.AIRTEK_E2E_PUBLIC_PORT ?? '3300'
const adminPort = process.env.AIRTEK_E2E_ADMIN_PORT ?? '3310'
const apiPort = process.env.AIRTEK_E2E_API_PORT ?? '8800'
const publicOrigin = `http://www.airtek.test:${gatewayPort}`
const adminOrigin = `http://admin.airtek.test:${gatewayPort}`
const apiOrigin = `http://api.airtek.test:${gatewayPort}`
const apiControlOrigin = `http://127.0.0.1:${gatewayPort}`
const e2eProductMasterCsv = [
  'stable_id,model,family,title,样品报价（sample）',
  'E2E-PRODUCT-001,E2E-MODEL-001,Axial,E2E imported product,100.00',
  '',
].join('\n')
const e2eProductMasterBytes = Buffer.from(e2eProductMasterCsv, 'utf8')
const e2eProductMasterSha256 = createHash('sha256').update(e2eProductMasterBytes).digest('hex')
const e2eProductMasterMapping = process.env.AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION
  ?? 'airtek-basic-v1'
const environment = {
  ...process.env,
  COMPOSE_PROJECT_NAME: project,
  AIRTEK_PUBLIC_HOST_PORT: publicPort,
  AIRTEK_ADMIN_HOST_PORT: adminPort,
  AIRTEK_API_HOST_PORT: apiPort,
  AIRTEK_GATEWAY_HOST_PORT: gatewayPort,
  AIRTEK_PUBLIC_ORIGIN: publicOrigin,
  AIRTEK_ADMIN_ORIGIN: adminOrigin,
  VITE_PUBLIC_ORIGIN: publicOrigin,
  PUBLIC_ORIGIN: publicOrigin,
  VITE_PUBLIC_API_BASE_URL: `${apiOrigin}/api/public/v1`,
  PUBLIC_API_BROWSER_ORIGIN: apiOrigin,
  VITE_ADMIN_API_BASE_URL: `${apiOrigin}/api/admin/v1`,
  API_BROWSER_ORIGIN: apiOrigin,
  PUBLIC_HOST: 'www.airtek.test',
  ADMIN_HOST: 'admin.airtek.test',
  API_HOST: 'api.airtek.test',
  AIRTEK_COMPOSE_SUBNET: process.env.AIRTEK_E2E_COMPOSE_SUBNET ?? '172.29.0.0/24',
  AIRTEK_GATEWAY_INTERNAL_IP: process.env.AIRTEK_E2E_GATEWAY_INTERNAL_IP ?? '172.29.0.10',
  AIRTEK_TRUSTED_PROXY_CIDRS: process.env.AIRTEK_E2E_TRUSTED_PROXY_CIDRS ?? '172.29.0.10/32',
  AIRTEK_ADMIN_BOOTSTRAP_TOKEN: process.env.E2E_ADMIN_BOOTSTRAP_TOKEN
    ?? 'airtek-e2e-bootstrap-token-change-me',
  AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY: process.env.AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY
    ?? Buffer.alloc(32, 0x45).toString('base64'),
  AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION: e2eProductMasterMapping,
  // This isolated production-build test stack registers only its synthetic
  // fixture. Deployment Compose and production.env.example retain the exact
  // owner-approved 370/5 authority tuple.
  AIRTEK_APPROVED_PRODUCT_MASTER_SHA256: e2eProductMasterSha256,
  AIRTEK_APPROVED_PRODUCT_MASTER_MAPPING_VERSION: e2eProductMasterMapping,
  AIRTEK_APPROVED_PRODUCT_MASTER_VALID_ROWS: '1',
  AIRTEK_APPROVED_PRODUCT_MASTER_ERROR_ROWS: '0',
  E2E_PRODUCT_MASTER_CSV_BASE64: e2eProductMasterBytes.toString('base64'),
  AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES: process.env.AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES
    ?? 'e2e-source',
  AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS: process.env.AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS
    ?? 'integration-test',
  AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS: process.env.AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS
    ?? 'admin-acceptance',
  E2E_PUBLIC_ORIGIN: publicOrigin,
  E2E_ADMIN_ORIGIN: adminOrigin,
  E2E_API_ORIGIN: apiOrigin,
  E2E_API_CONTROL_ORIGIN: apiControlOrigin,
  E2E_GATEWAY_CONTROL_ORIGIN: apiControlOrigin,
  E2E_HOST_RESOLVER_RULES: 'MAP www.airtek.test 127.0.0.1, MAP admin.airtek.test 127.0.0.1, MAP api.airtek.test 127.0.0.1',
  E2E_RUN_ADMIN_WORKFLOWS: 'true',
  E2E_ISOLATED_STACK: 'true',
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: process.cwd(),
    env: environment,
    stdio: 'inherit',
    ...options,
  })
  return result.status ?? 1
}

let testStatus = 1
try {
  run('docker', ['compose', 'down', '--volumes', '--remove-orphans'])
  const upStatus = run('docker', [
    'compose', 'up', '--build', '--detach', '--wait', '--wait-timeout', '300',
  ])
  if (upStatus !== 0) {
    run('docker', ['compose', 'logs', '--no-color'])
    process.exitCode = upStatus
  }
  else {
    testStatus = run('pnpm', ['exec', 'playwright', 'test'])
    if (testStatus !== 0) run('docker', ['compose', 'logs', '--no-color'])
    process.exitCode = testStatus
  }
} finally {
  const downStatus = run('docker', ['compose', 'down', '--volumes', '--remove-orphans'])
  if (downStatus !== 0 && (process.exitCode ?? 0) === 0) process.exitCode = downStatus
}
