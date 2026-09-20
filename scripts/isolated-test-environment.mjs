import { randomBytes } from 'node:crypto'

export function isolatedTestEnvironment(source = process.env, token = randomBytes(6).toString('hex')) {
  // Never inherit business connections, even if a caller accidentally invokes this from its shell.
  for (const key of ['DATABASE_URL', 'AIRTEK_DATABASE_URL_INTERNAL', 'AIRTEK_TEST_DATABASE_URL', 'FLYWAY_URL']) {
    if (source[key]) throw new Error(`Refusing inherited ${key}; tests allocate their own disposable database.`)
  }
  if (source.E2E_COMPOSE_FILE) throw new Error('Custom test Compose files are not permitted.')
  const prefix = source.E2E_COMPOSE_PROJECT_NAME ?? 'airtek-e2e'
  if (!/^airtek(?:power)?-e2e(?:-[a-z0-9-]+)?$/.test(prefix)) throw new Error('Test project must have an airtek-e2e prefix.')
  if (!/^[a-f0-9]{12}$/.test(token)) throw new Error('Invalid isolation token.')
  const database = `airtek_test_${token}`
  return {
    COMPOSE_PROJECT_NAME: `${prefix}-${token}`,
    COMPOSE_FILE: 'compose.yaml:compose.e2e.yaml',
    COMPOSE_DISABLE_ENV_FILE: '1',
    POSTGRES_DB: database, POSTGRES_USER: 'airtek', POSTGRES_PASSWORD: 'airtek-test-only',
    FLYWAY_URL: `jdbc:postgresql://postgres:5432/${database}`,
    FLYWAY_USER: 'airtek', FLYWAY_PASSWORD: 'airtek-test-only', FLYWAY_PLACEHOLDERS_RUNTIME_ROLE: 'airtek',
    AIRTEK_DATABASE_URL_INTERNAL: `postgres://airtek:airtek-test-only@postgres:5432/${database}`,
  }
}
