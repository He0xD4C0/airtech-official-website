import { isolatedTestEnvironment } from './isolated-test-environment.mjs'
import { testProcess } from './test-process.mjs'
import { contractObjectStore } from './contract-object-store.mjs'
import { repositoryDotenv } from './dotenv.mjs'

const isolated = isolatedTestEnvironment()
const name = isolated.COMPOSE_PROJECT_NAME + '-postgres'
const env = {
  ...repositoryDotenv(), ...process.env, ...isolated,
  AIRTEK_TEST_TEMPLATE_DATABASE: 'airtek_test_template',
}
const { run, capture } = testProcess(env)
const storage = contractObjectStore(`${name}-objects`, run, capture)
const postgres = ['docker', ['exec', name, 'psql', '-X', '-U', 'airtek', '-d', 'postgres', '-v', 'ON_ERROR_STOP=1']]
async function checked(command, args) {
  if (await run(command, args) !== 0) throw new Error(`${command} failed`)
}
let owned = false
try {
  const existing = await capture('docker', ['container', 'ls', '--all', '--quiet', '--filter', `name=^/${name}$`])
  if (existing.status !== 0 || existing.stdout.trim()) throw new Error('Disposable database container name is already in use.')
  owned = true
  // No host volume or network is reused. The tmpfs and all cloned test databases die with this container.
  await checked('docker', ['run', '--detach', '--name', name, '--publish', '127.0.0.1::5432',
    '--tmpfs', '/var/lib/postgresql/data:rw', '-e', 'POSTGRES_USER=airtek', '-e', 'POSTGRES_PASSWORD=airtek-test-only',
    '-e', `POSTGRES_DB=${isolated.POSTGRES_DB}`, 'postgres:17-alpine', '-c', 'max_locks_per_transaction=256'])
  let healthy = false
  for (let attempt = 0; attempt < 60; attempt++) {
    if ((await capture('docker', ['exec', name, 'pg_isready', '-U', 'airtek', '-d', isolated.POSTGRES_DB])).status === 0) {
      healthy = true
      break
    }
    await new Promise((resolve) => setTimeout(resolve, 500))
  }
  if (!healthy) throw new Error('Disposable PostgreSQL did not start.')
  const port = (await capture('docker', ['port', name, '5432/tcp'])).stdout.trim().split(':').at(-1)
  if (!/^\d+$/.test(port)) throw new Error('No isolated database port.')
  env.DATABASE_URL = `postgres://airtek:airtek-test-only@127.0.0.1:${port}/${isolated.POSTGRES_DB}`
  env.AIRTEK_TEST_DATABASE_URL = env.DATABASE_URL
  const flywayImage = 'airtek-contract-flyway:local'
  await checked('docker', ['build', '-f', 'infra/docker/Dockerfile.flyway', '-t', flywayImage, '.'])
  await checked('docker', ['run', '--rm', '--network', `container:${name}`,
    '-e', `FLYWAY_URL=jdbc:postgresql://127.0.0.1:5432/${isolated.POSTGRES_DB}`,
    '-e', 'FLYWAY_USER=airtek', '-e', 'FLYWAY_PASSWORD=airtek-test-only',
    '-e', 'FLYWAY_PLACEHOLDERS_RUNTIME_ROLE=airtek', '-e', 'AIRTEK_FLYWAY_ALLOW_SHARED_ROLE=true',
    '-e', 'AIRTEK_FLYWAY_TARGET=30', flywayImage, 'migrate'])
  const cargo = ['--manifest-path', 'services/platform/Cargo.toml']
  await checked('cargo', ['run', ...cargo, '-p', 'airtek-maintenance', '--bin', 'airtek-maintenance', '--', 'prepare-runtime'])
  await checked(postgres[0], [...postgres[1], '-c', `CREATE DATABASE airtek_test_template TEMPLATE ${isolated.POSTGRES_DB}`])
  env.AIRTEK_TEST_S3_ENDPOINT = await storage.start()
  for (const suite of ['postgres_contract', 'retention_contract', 'migration_contract', 'cms_current_dependencies_contract', 'publication_dependency_migration_contract']) {
    await checked('cargo', ['test', ...cargo, '-p', 'airtek-platform-contract-tests', '--features', 'devtools', '--test', suite, '--', '--ignored'])
  }
  for (const filter of ['synthetic_master::', 'feishu_takeover_and_expiry::']) {
    await checked('cargo', ['test', ...cargo, '-p', 'airtek-platform-contract-tests', '--test', 'product_import_contract', filter, '--', '--ignored', '--test-threads=1'])
  }
  if (env.AIRTEK_PRODUCT_MASTER_TEST_CSV) {
    await checked('cargo', ['test', ...cargo, '-p', 'airtek-platform-contract-tests', '--test', 'product_import_contract', 'verified_master::', '--', '--ignored', '--test-threads=1'])
  } else {
    console.log('Controlled Product Master contract skipped: AIRTEK_PRODUCT_MASTER_TEST_CSV is not supplied.')
  }
} catch (error) {
  console.error(error.message)
  process.exitCode = 1
} finally {
  if (!await storage.cleanup()) process.exitCode = 1
  if (owned && await run('docker', ['rm', '--force', name], { cleanup: true }) !== 0) process.exitCode = 1
}
