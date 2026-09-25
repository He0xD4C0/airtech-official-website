import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'

const project = process.env.E2E_COMPOSE_PROJECT_NAME ?? 'airtekpower-e2e'
const gatewayPort = process.env.AIRTEK_E2E_GATEWAY_PORT ?? '8088'
const publicPort = process.env.AIRTEK_E2E_PUBLIC_PORT ?? '3300'
const adminPort = process.env.AIRTEK_E2E_ADMIN_PORT ?? '3310'
const apiPort = process.env.AIRTEK_E2E_API_PORT ?? '8800'
const minioPort = process.env.AIRTEK_E2E_MINIO_PORT ?? '19000'
const minioConsolePort = process.env.AIRTEK_E2E_MINIO_CONSOLE_PORT ?? '19001'
const publicOrigin = `http://localhost:${publicPort}`
const adminOrigin = `http://localhost:${adminPort}`
const apiOrigin = `http://localhost:${apiPort}`
const apiControlOrigin = apiOrigin
const gatewayControlOrigin = `http://localhost:${gatewayPort}`
const mediaOrigin = `http://media.localhost:${minioPort}`
const e2eProductMasterCsv = [
  'stable_id,model,family,title,样品报价（sample）',
  'E2E-PRODUCT-001,E2E-MODEL-001,Axial,E2E imported product,100.00',
  '',
].join('\n')
const e2eProductMasterBytes = Buffer.from(e2eProductMasterCsv, 'utf8')
const e2eProductMasterSha256 = createHash('sha256').update(e2eProductMasterBytes).digest('hex')
const e2eProductMasterMapping = process.env.AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION
  ?? 'airtek-basic-v1'
const playwrightArgs = (process.env.E2E_PLAYWRIGHT_ARGS ?? '').trim()
  ? (process.env.E2E_PLAYWRIGHT_ARGS ?? '').trim().split(/\s+/)
  : []
const composeBuildArgument = process.env.E2E_SKIP_BUILD === 'true' ? '--no-build' : '--build'
const environment = {
  ...process.env,
  COMPOSE_PROJECT_NAME: project,
  COMPOSE_FILE: process.env.E2E_COMPOSE_FILE ?? 'compose.yaml:compose.e2e.yaml',
  COMPOSE_PROFILES: 'minio',
  AIRTEK_PUBLIC_HOST_PORT: publicPort,
  AIRTEK_ADMIN_HOST_PORT: adminPort,
  AIRTEK_API_HOST_PORT: apiPort,
  AIRTEK_GATEWAY_HOST_PORT: gatewayPort,
  AIRTEK_OBJECT_STORE_HOST_PORT: minioPort,
  AIRTEK_OBJECT_STORE_CONSOLE_HOST_PORT: minioConsolePort,
  AIRTEK_COMPOSE_PUBLIC_ORIGIN: publicOrigin,
  AIRTEK_COMPOSE_ADMIN_ORIGIN: adminOrigin,
  AIRTEK_COMPOSE_API_ORIGIN: apiOrigin,
  AIRTEK_COMPOSE_MEDIA_ORIGIN: mediaOrigin,
  AIRTEK_PUBLIC_ASSET_HOSTNAME: 'media.localhost',
  PUBLIC_HOST: 'www.localhost',
  ADMIN_HOST: 'admin.localhost',
  API_HOST: 'api.localhost',
  AIRTEK_COMPOSE_SUBNET: process.env.AIRTEK_E2E_COMPOSE_SUBNET ?? '172.29.0.0/24',
  AIRTEK_GATEWAY_INTERNAL_IP: process.env.AIRTEK_E2E_GATEWAY_INTERNAL_IP ?? '172.29.0.10',
  AIRTEK_TRUSTED_PROXY_CIDRS: process.env.AIRTEK_E2E_TRUSTED_PROXY_CIDRS ?? '172.29.0.10/32',
  AIRTEK_PLATFORM_FEATURES: 'production',
  AIRTEK_MAINTENANCE_COMMAND: 'prepare-runtime',
  AIRTEK_DEV_ADMIN_SEED: 'false',
  AIRTEK_FLYWAY_TARGET: '27',
  AIRTEK_ADMIN_BOOTSTRAP_TOKEN: process.env.E2E_ADMIN_BOOTSTRAP_TOKEN
    ?? 'airtek-e2e-bootstrap-token-change-me',
  AIRTEK_TOTP_ENCRYPTION_KEY: process.env.AIRTEK_TOTP_ENCRYPTION_KEY
    ?? Buffer.alloc(32, 0x54).toString('base64'),
  AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY: process.env.AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY
    ?? Buffer.alloc(32, 0x50).toString('base64'),
  AIRTEK_ANALYTICS_TOKEN_HMAC_KEY: process.env.AIRTEK_ANALYTICS_TOKEN_HMAC_KEY
    ?? Buffer.alloc(32, 0x41).toString('base64'),
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
  E2E_GATEWAY_CONTROL_ORIGIN: gatewayControlOrigin,
  E2E_MEDIA_PUBLIC_BASE_URL: `${mediaOrigin}/airtek-media`,
  E2E_PUBLIC_GATEWAY_HOST: environmentHost('PUBLIC_HOST', 'www.localhost'),
  E2E_ADMIN_GATEWAY_HOST: environmentHost('ADMIN_HOST', 'admin.localhost'),
  E2E_RUN_ADMIN_WORKFLOWS: 'true',
  E2E_ISOLATED_STACK: 'true',
}

function environmentHost(name, fallback) {
  return process.env[name] || fallback
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

function capture(command, args) {
  return spawnSync(command, args, {
    cwd: process.cwd(),
    env: environment,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  })
}

function waitForSuccessfulJobs(services, timeoutMs = 300_000) {
  const deadline = Date.now() + timeoutMs
  const pending = new Set(services)
  while (pending.size > 0 && Date.now() < deadline) {
    for (const service of pending) {
      const listed = capture('docker', ['compose', 'ps', '--all', '--quiet', service])
      const containerId = listed.stdout.trim()
      if (listed.status !== 0 || !containerId) continue
      const inspected = capture('docker', [
        'inspect', '--format', '{{.State.Status}} {{.State.ExitCode}}', containerId,
      ])
      const [status, exitCode] = inspected.stdout.trim().split(/\s+/)
      if (status !== 'exited') continue
      if (exitCode !== '0') {
        console.error(`${service} exited with status ${exitCode || 'unknown'}.`)
        return 1
      }
      pending.delete(service)
    }
    if (pending.size > 0) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500)
  }
  if (pending.size > 0) {
    console.error(`Timed out waiting for one-shot services: ${[...pending].join(', ')}`)
    return 1
  }
  return 0
}

let testStatus = 1
try {
  run('docker', ['compose', 'down', '--volumes', '--remove-orphans'])
  const launchStatus = run('docker', [
    'compose', 'up', composeBuildArgument, '--detach',
  ])
  const initStatus = launchStatus === 0
    ? waitForSuccessfulJobs(['flyway-migrate', 'platform-maintenance', 'minio-create-bucket'])
    : launchStatus
  const readyStatus = initStatus === 0
    ? run('docker', [
        'compose', 'up', '--detach', '--wait', '--wait-timeout', '300', '--no-deps',
        'postgres', 'minio', 'platform-api', 'platform-worker', 'public-web', 'admin-web', 'gateway',
      ])
    : initStatus
  if (readyStatus !== 0) {
    run('docker', ['compose', 'logs', '--no-color'])
    process.exitCode = readyStatus
  }
  else {
    const playwrightStatus = run('pnpm', ['exec', 'playwright', 'test', ...playwrightArgs])
    const readinessStatus = run('node', [
      'scripts/check-public-readiness.mjs',
      '--public-base', publicOrigin,
      '--api-base', apiOrigin,
      '--expected-public-origin', publicOrigin,
      '--expected-api-origin', apiOrigin,
      '--allow-placeholders',
    ])
    testStatus = playwrightStatus || readinessStatus
    if (testStatus !== 0) run('docker', ['compose', 'logs', '--no-color'])
    process.exitCode = testStatus
  }
} finally {
  const downStatus = run('docker', ['compose', 'down', '--volumes', '--remove-orphans'])
  if (downStatus !== 0 && (process.exitCode ?? 0) === 0) process.exitCode = downStatus
}
