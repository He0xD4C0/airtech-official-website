import { isolatedTestEnvironment } from './isolated-test-environment.mjs'
import { testProcess } from './test-process.mjs'
import { repositoryDotenv } from './dotenv.mjs'
import { createHash } from 'node:crypto'
import { readdirSync } from 'node:fs'
import { mkdir, mkdtemp, rm } from 'node:fs/promises'
import { join } from 'node:path'

const isolation = isolatedTestEnvironment()
await mkdir('.local/qa', { recursive: true })
const sessionDirectory = await mkdtemp(join('.local/qa', 'e2e-session-'))
const gatewayPort = process.env.AIRTEK_E2E_GATEWAY_PORT ?? '18089'
const publicPort = process.env.AIRTEK_E2E_PUBLIC_PORT ?? '3300'
const adminPort = process.env.AIRTEK_E2E_ADMIN_PORT ?? '3310'
const apiPort = process.env.AIRTEK_E2E_API_PORT ?? '8800'
const minioPort = process.env.AIRTEK_E2E_MINIO_PORT ?? '19100'
const minioConsolePort = process.env.AIRTEK_E2E_MINIO_CONSOLE_PORT ?? '19101'
const publicOrigin = `http://www.airtek.localhost:${gatewayPort}`
const adminOrigin = `http://admin.airtek.localhost:${gatewayPort}`
const apiOrigin = `http://api.airtek.localhost:${gatewayPort}`
const apiControlOrigin = `http://127.0.0.1:${apiPort}`
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
const latestMigrationVersion = Math.max(...readdirSync('services/platform/migrations')
  .map((name) => /^V(\d+)__.+\.sql$/u.exec(name))
  .filter(Boolean)
  .map((match) => Number(match[1])))
const environment = {
  ...repositoryDotenv(),
  ...process.env,
  ...isolation,
  COMPOSE_PROFILES: 'minio',
  AIRTEK_PUBLIC_HOST_PORT: publicPort,
  AIRTEK_ADMIN_HOST_PORT: adminPort,
  AIRTEK_API_HOST_PORT: apiPort,
  AIRTEK_GATEWAY_HOST_PORT: gatewayPort,
  AIRTEK_OBJECT_STORE_HOST_PORT: minioPort,
  AIRTEK_OBJECT_STORE_CONSOLE_HOST_PORT: minioConsolePort,
  // The API validates object storage through the host gateway, so the isolated
  // stack publishes MinIO beyond loopback for the duration of the run.
  AIRTEK_OBJECT_STORE_BIND_ADDRESS: '0.0.0.0',
  AIRTEK_COMPOSE_PUBLIC_ORIGIN: publicOrigin,
  AIRTEK_COMPOSE_ADMIN_ORIGIN: adminOrigin,
  AIRTEK_COMPOSE_API_ORIGIN: apiOrigin,
  AIRTEK_COMPOSE_MEDIA_ORIGIN: mediaOrigin,
  PUBLIC_HOST: 'www.airtek.localhost',
  ADMIN_HOST: 'admin.airtek.localhost',
  API_HOST: 'api.airtek.localhost',
  AIRTEK_COMPOSE_SUBNET: process.env.AIRTEK_E2E_COMPOSE_SUBNET ?? '172.29.0.0/24',
  AIRTEK_GATEWAY_INTERNAL_IP: process.env.AIRTEK_E2E_GATEWAY_INTERNAL_IP ?? '172.29.0.10',
  AIRTEK_TRUSTED_PROXY_CIDRS: process.env.AIRTEK_E2E_TRUSTED_PROXY_CIDRS ?? '172.29.0.10/32',
  AIRTEK_PLATFORM_FEATURES: 'production',
  AIRTEK_MAINTENANCE_COMMAND: 'prepare-runtime',
  AIRTEK_FLYWAY_TARGET: String(latestMigrationVersion),
  AIRTEK_ADMIN_EMAIL: process.env.E2E_ADMIN_EMAIL ?? 'e2e-admin@airtek.invalid',
  AIRTEK_ADMIN_DISPLAY_NAME: 'AIRTEK E2E Administrator',
  AIRTEK_ADMIN_PASSWORD: process.env.E2E_ADMIN_PASSWORD ?? 'Airtek-E2E-Admin-123!',
  AIRTEK_ADMIN_RECOVERY_KEY_MODE: 'auto',
  AIRTEK_TOTP_ENCRYPTION_KEY: process.env.AIRTEK_TOTP_ENCRYPTION_KEY
    ?? Buffer.alloc(32, 0x42).toString('base64'),
  AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY: process.env.AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY
    ?? Buffer.alloc(32, 0x54).toString('base64'),
  AIRTEK_ANALYTICS_TOKEN_HMAC_KEY: process.env.AIRTEK_ANALYTICS_TOKEN_HMAC_KEY
    ?? Buffer.alloc(32, 0x64).toString('base64'),
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
  E2E_PUBLIC_GATEWAY_HOST: 'www.airtek.localhost',
  E2E_ADMIN_GATEWAY_HOST: 'admin.airtek.localhost',
  E2E_API_GATEWAY_HOST: 'api.airtek.localhost',
  E2E_HOST_RESOLVER_RULES: process.env.E2E_HOST_RESOLVER_RULES
    ?? 'MAP *.airtek.localhost 127.0.0.1',
  E2E_RUN_ADMIN_WORKFLOWS: 'true',
  E2E_ISOLATED_STACK: 'true',
  E2E_ADMIN_STORAGE_STATE: join(sessionDirectory, 'admin.json'),
  E2E_ADMIN_SECONDARY_STORAGE_STATE: join(sessionDirectory, 'secondary.json'),
  E2E_ADMIN_TOTP_SECRET: join(sessionDirectory, 'totp.txt'),
}

const { run, capture } = testProcess(environment)

async function waitForSuccessfulJobs(services, timeoutMs = 300_000) {
  const deadline = Date.now() + timeoutMs
  const pending = new Set(services)
  while (pending.size > 0 && Date.now() < deadline) {
    for (const service of pending) {
      const listed = await capture('docker', ['compose', '--project-directory', '.', 'ps', '--all', '--quiet', service])
      const containerId = listed.stdout.trim()
      if (listed.status !== 0 || !containerId) continue
      const inspected = await capture('docker', [
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
    if (pending.size > 0) await new Promise((resolve) => setTimeout(resolve, 500))
  }
  if (pending.size > 0) {
    console.error(`Timed out waiting for one-shot services: ${[...pending].join(', ')}`)
    return 1
  }
  return 0
}

let testStatus = 1
let owned = false
try {
  const existing = await capture('docker', ['compose', '--project-directory', '.', 'ps', '--all', '--quiet'])
  if (existing.status !== 0 || existing.stdout.trim()) throw new Error('Test project is not empty; refusing to use it.')
  owned = true
  const launchStatus = await run('docker', [
    'compose', '--project-directory', '.', 'up', composeBuildArgument, '--detach',
  ])
  const initStatus = launchStatus === 0
    ? await waitForSuccessfulJobs(['flyway-migrate', 'platform-maintenance', 'minio-create-bucket'])
    : launchStatus
  const readyStatus = initStatus === 0
    ? await run('docker', [
        'compose', '--project-directory', '.', 'up', '--detach', '--wait', '--wait-timeout', '300', '--no-deps',
        'postgres', 'minio', 'platform-api', 'platform-worker', 'public-web', 'admin-web', 'gateway',
      ])
    : initStatus
  if (readyStatus !== 0) {
    await run('docker', ['compose', '--project-directory', '.', 'logs', '--no-color'])
    process.exitCode = readyStatus
  }
  else {
    const emptyProjectionStatus = await run('node', ['scripts/checks/assert-empty-public-projection.mjs'])
    const playwrightStatus = emptyProjectionStatus === 0
      ? await run('pnpm', ['exec', 'playwright', 'test', ...playwrightArgs])
      : emptyProjectionStatus
    const runtimeReadinessStatus = await run('docker', [
      'compose', '--project-directory', '.', 'run', '--rm', '--no-deps',
      '-e', `AIRTEK_PUBLIC_ORIGIN=${publicOrigin}`,
      '-e', `AIRTEK_ADMIN_ORIGIN=${adminOrigin}`,
      '-e', `AIRTEK_API_ORIGIN=${apiOrigin}`,
      '-e', 'AIRTEK_READINESS_GATEWAY_ORIGIN=http://gateway:8088',
      '-e', 'AIRTEK_READINESS_ALLOW_HTTP=true',
      '-e', `PUBLIC_HOST=${environment.E2E_PUBLIC_GATEWAY_HOST}`,
      '-e', `ADMIN_HOST=${environment.E2E_ADMIN_GATEWAY_HOST}`,
      '-e', `API_HOST=${environment.E2E_API_GATEWAY_HOST}`,
      'platform-maintenance', 'check-public-readiness',
    ])
    const endpointReadinessStatus = await run('node', [
      'scripts/checks/check-public-readiness.mjs',
      '--public-base', publicOrigin,
      '--api-base', apiOrigin,
      '--expected-public-origin', publicOrigin,
      '--expected-api-origin', apiOrigin,
    ])
    testStatus = playwrightStatus || runtimeReadinessStatus || endpointReadinessStatus
    if (testStatus !== 0) await run('docker', ['compose', '--project-directory', '.', 'logs', '--no-color'])
    process.exitCode = testStatus
  }
} catch (error) {
  console.error(error.message)
  process.exitCode = 1
} finally {
  const downStatus = owned ? await run('docker', ['compose', '--project-directory', '.', 'down', '--volumes', '--remove-orphans'], { cleanup: true }) : 0
  if (downStatus !== 0 && (process.exitCode ?? 0) === 0) process.exitCode = downStatus
  // Generated authentication files belong only to this run, including interrupted runs.
  await rm(sessionDirectory, { recursive: true, force: true })
}
