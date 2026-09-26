import { test } from 'node:test'
import assert from 'node:assert/strict'
import { isolatedTestEnvironment } from './isolated-test-environment.mjs'

test('pins every database connection to the same disposable database', () => {
  const env = isolatedTestEnvironment({}, '123456abcdef')
  for (const key of ['POSTGRES_DB', 'FLYWAY_URL', 'AIRTEK_DATABASE_URL_INTERNAL']) {
    assert.ok(env[key].endsWith('airtek_test_123456abcdef'))
  }
  assert.equal(env.COMPOSE_DISABLE_ENV_FILE, '1')
  assert.notEqual(isolatedTestEnvironment({}).COMPOSE_PROJECT_NAME, isolatedTestEnvironment({}).COMPOSE_PROJECT_NAME)
})
test('refuses inherited connections, business projects and arbitrary Compose files', () => {
  for (const key of ['DATABASE_URL', 'AIRTEK_DATABASE_URL_INTERNAL', 'AIRTEK_TEST_DATABASE_URL', 'FLYWAY_URL']) {
    assert.throws(() => isolatedTestEnvironment({ [key]: 'postgres://localhost/airtek' }), /Refusing inherited/)
  }
  assert.throws(() => isolatedTestEnvironment({ E2E_COMPOSE_PROJECT_NAME: 'airtekpower' }), /prefix/)
  assert.throws(() => isolatedTestEnvironment({ E2E_COMPOSE_FILE: 'infra/compose/production.app.yaml' }), /not permitted/)
})
