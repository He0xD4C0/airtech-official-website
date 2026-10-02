// Migration release contract: exercises infra/docker/flyway-release.sh against
// a disposable PostgreSQL 17 instance with a stubbed Flyway entrypoint, so the
// dump/restore/failure-record contract is verified without applying real
// migrations or touching any existing database.
import { execFileSync, spawnSync } from 'node:child_process'
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..', '..')
const wrapper = join(root, 'infra/docker/flyway-release.sh')
const testImage = 'airtek-migration-release-contract:local'
const containerName = `airtek-migration-contract-${process.pid}`

function run(command, args, options = {}) {
  return execFileSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], ...options })
}

function attempt(command, args, options = {}) {
  return spawnSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], ...options })
}

function assert(condition, message) {
  if (!condition) throw new Error(message)
}

function runDetail(result) {
  return `exit=${result.status}\nstdout=${result.stdout ?? ''}\nstderr=${result.stderr ?? ''}`
}

function psql(port, database, statement) {
  return run('docker', [
    'exec', containerName, 'psql', '-X', '-U', 'airtek', '-d', database,
    '-v', 'ON_ERROR_STOP=1', '-q', '-c', statement,
  ])
}

function sqlValue(port, database, statement) {
  return run('docker', [
    'exec', containerName, 'psql', '-X', '-U', 'airtek', '-d', database,
    '-A', '-t', '-v', 'ON_ERROR_STOP=1', '-c', statement,
  ]).trim()
}

function writeFixture(directory, { restoreFails = false } = {}) {
  const state = join(directory, 'state')
  const migrations = join(directory, 'migrations')
  mkdirSync(state, { recursive: true })
  mkdirSync(migrations, { recursive: true })
  writeFileSync(join(migrations, 'V0001__contract_base.sql'), 'select 1;\n')
  writeFileSync(join(migrations, 'V0002__contract_next.sql'), 'select 2;\n')
  copyFileSync(wrapper, join(directory, 'flyway-release.sh'))
  writeFileSync(
    join(directory, 'airtek-flyway'),
    `#!/bin/sh
case "\${STUB_MODE:-success}" in
  success)
    psql -X -q -v ON_ERROR_STOP=1 -c "create table if not exists public.flyway_schema_history(version text primary key, success boolean not null);"
    psql -X -q -v ON_ERROR_STOP=1 -c "insert into public.flyway_schema_history(version,success) values ('2',true) on conflict (version) do update set success=true;"
    ;;
  fail)
    psql -X -q -v ON_ERROR_STOP=1 -c "insert into public.contract_marker(id) values (2);"
    echo "ERROR: contract migration failure" >&2
    exit 1
    ;;
  hang)
    sleep 600
    ;;
esac
exit 0
`,
    { mode: 0o755 },
  )
  if (restoreFails) {
    writeFileSync(join(directory, 'pg-restore'), '#!/bin/sh\necho "contract restore failure" >&2\nexit 1\n', { mode: 0o755 })
  }
  return state
}

function runWrapper(directory, port, database, extraEnv = {}, { detached = false } = {}) {
  const env = [
    `FLYWAY_URL=jdbc:postgresql://host.docker.internal:${port}/${database}`,
    'FLYWAY_USER=airtek',
    'FLYWAY_PASSWORD=airtek',
    'AIRTEK_RELEASE_TAG=contract-1',
    'AIRTEK_MIGRATIONS_DIR=/work/migrations',
    'AIRTEK_MIGRATION_STATE_DIR=/work/state',
    'AIRTEK_FLYWAY_ENTRYPOINT=/work/airtek-flyway',
    ...Object.entries(extraEnv).map(([key, value]) => `${key}=${value}`),
  ].flatMap((entry) => ['-e', entry])
  const args = [
    'run', '--rm',
    ...(detached ? ['-d', '--name', `${containerName}-wrapper`] : []),
    '--add-host', 'host.docker.internal:host-gateway',
    '-v', `${directory}:/work`,
    ...env,
    testImage,
    'sh', '/work/flyway-release.sh',
  ]
  if (detached) {
    run('docker', args)
    return null
  }
  return attempt('docker', args)
}

const report = []
const directory = mkdtempSync(join(tmpdir(), 'airtek-migration-contract-'))
let port

try {
  execFileSync('docker', ['build', '--tag', testImage, '--file', '-', join(root, 'infra/docker')], {
    input: 'FROM postgres:17-alpine\nRUN apk add --no-cache jq\n',
    stdio: ['pipe', 'pipe', 'pipe'],
  })

  run('docker', [
    'run', '--detach', '--name', containerName,
    '--publish', '127.0.0.1::5432', '--tmpfs', '/var/lib/postgresql/data:rw',
    '-e', 'POSTGRES_USER=airtek', '-e', 'POSTGRES_PASSWORD=airtek', '-e', 'POSTGRES_DB=contract',
    'postgres:17-alpine',
  ])
  let healthy = false
  for (let i = 0; i < 60; i += 1) {
    if (attempt('docker', ['exec', containerName, 'pg_isready', '-U', 'airtek', '-d', 'contract']).status === 0) {
      healthy = true
      break
    }
    execFileSync('sleep', ['0.5'])
  }
  assert(healthy, 'disposable PostgreSQL did not start')
  port = run('docker', ['port', containerName, '5432/tcp']).trim().split(':').at(-1)
  assert(/^\d+$/.test(port), 'no disposable PostgreSQL port')

  const fixtureSql = `
    create table if not exists public.flyway_schema_history(version text primary key, success boolean not null);
    create table if not exists public.contract_marker(id integer primary key);
    create table if not exists public.operation_runs(
      id uuid primary key, kind text not null, status text not null, reason text not null,
      result jsonb, created_at timestamptz not null, updated_at timestamptz not null);`

  // 1. No pending migration: the wrapper must not dump or call Flyway.
  run('docker', ['exec', containerName, 'psql', '-X', '-U', 'airtek', '-d', 'postgres',
    '-v', 'ON_ERROR_STOP=1', '-q', '-c', 'create database contract_current'])
  psql(port, 'contract_current', fixtureSql)
  psql(port, 'contract_current', "insert into public.flyway_schema_history(version,success) values ('2',true)")
  const currentDirectory = mkdtempSync(join(directory, 'current-'))
  const currentState = writeFixture(currentDirectory)
  const currentRun = runWrapper(currentDirectory, port, 'contract_current')
  assert(currentRun.status === 0, `no-pending run must exit 0\n${runDetail(currentRun)}`)
  assert(!existsSync(join(currentState, 'pre-migrate-contract-1.dump')), 'no-pending run must not dump')
  report.push('no-pending migration skips the dump')

  // 2. Successful migration: the dump is removed afterwards.
  run('docker', ['exec', containerName, 'psql', '-X', '-U', 'airtek', '-d', 'postgres',
    '-v', 'ON_ERROR_STOP=1', '-q', '-c', 'create database contract_success'])
  psql(port, 'contract_success', fixtureSql)
  const successDirectory = mkdtempSync(join(directory, 'success-'))
  const successState = writeFixture(successDirectory, { mode: 'success' })
  const successRun = runWrapper(successDirectory, port, 'contract_success', { STUB_MODE: 'success' })
  assert(successRun.status === 0, `successful migration must exit 0\n${runDetail(successRun)}`)
  assert(!existsSync(join(successState, 'pre-migrate-contract-1.dump')), 'successful migration must remove the dump')
  report.push('successful migration removes the pre-migration dump')

  // 3. Failing migration: restored database, retained failure record, operation row.
  run('docker', ['exec', containerName, 'psql', '-X', '-U', 'airtek', '-d', 'postgres',
    '-v', 'ON_ERROR_STOP=1', '-q', '-c', 'create database contract_failure'])
  psql(port, 'contract_failure', fixtureSql)
  const failureDirectory = mkdtempSync(join(directory, 'failure-'))
  const failureState = writeFixture(failureDirectory, { mode: 'fail' })
  const failureRun = runWrapper(failureDirectory, port, 'contract_failure', { STUB_MODE: 'fail' })
  assert(failureRun.status === 2, `restored migration failure must exit 2\n${runDetail(failureRun)}`)
  assert(sqlValue(port, 'contract_failure', 'select count(*) from public.contract_marker') === '0',
    'restore must remove the partially migrated marker row')
  const failureRecords = execFileSync('ls', [join(failureState, 'failures')], { encoding: 'utf8' }).trim().split('\n')
  assert(failureRecords.length === 1, 'exactly one failure record must be written')
  const record = JSON.parse(readFileSync(join(failureState, 'failures', failureRecords[0]), 'utf8'))
  assert(record.restore.succeeded === true, 'failure record must confirm the restore')
  assert(record.databaseStateUnknown === false, 'restored failure must not report an unknown database state')
  assert(record.dump.sha256.length === 64, 'failure record must carry the dump digest')
  assert(Number(sqlValue(port, 'contract_failure',
    "select count(*) from public.operation_runs where kind='migrationApply' and status='failed'")) === 1,
    'restored failure must persist a migrationApply failure row')
  report.push('restored failure exits 2 with a retained record and database row')

  // 4. Unrecoverable failure: the dump is always retained.
  run('docker', ['exec', containerName, 'psql', '-X', '-U', 'airtek', '-d', 'postgres',
    '-v', 'ON_ERROR_STOP=1', '-q', '-c', 'create database contract_unrecoverable'])
  psql(port, 'contract_unrecoverable', fixtureSql)
  const unrecoverableDirectory = mkdtempSync(join(directory, 'unrecoverable-'))
  const unrecoverableState = writeFixture(unrecoverableDirectory, { mode: 'fail', restoreFails: true })
  const unrecoverableRun = runWrapper(unrecoverableDirectory, port, 'contract_unrecoverable', {
    STUB_MODE: 'fail',
    AIRTEK_PG_RESTORE_BIN: '/work/pg-restore',
  })
  assert(unrecoverableRun.status === 3, `unrecoverable failure must exit 3\n${runDetail(unrecoverableRun)}`)
  assert(existsSync(join(unrecoverableState, 'pre-migrate-contract-1.dump')), 'unrecoverable failure must retain the dump')
  const unrecoverableRecordName = execFileSync('ls', [join(unrecoverableState, 'failures')], { encoding: 'utf8' }).trim()
  const unrecoverableRecord = JSON.parse(readFileSync(join(unrecoverableState, 'failures', unrecoverableRecordName), 'utf8'))
  assert(unrecoverableRecord.databaseStateUnknown === true, 'unrecoverable failure must flag the unknown database state')
  report.push('unrecoverable failure exits 3 and keeps the dump')

  // 5. A killed migration container leaves the dump on the mounted volume.
  run('docker', ['exec', containerName, 'psql', '-X', '-U', 'airtek', '-d', 'postgres',
    '-v', 'ON_ERROR_STOP=1', '-q', '-c', 'create database contract_killed'])
  psql(port, 'contract_killed', fixtureSql)
  const killedDirectory = mkdtempSync(join(directory, 'killed-'))
  const killedState = writeFixture(killedDirectory, { mode: 'hang' })
  runWrapper(killedDirectory, port, 'contract_killed', { STUB_MODE: 'hang' }, { detached: true })
  const dumpPath = join(killedState, 'pre-migrate-contract-1.dump')
  let dumped = false
  for (let i = 0; i < 60; i += 1) {
    if (existsSync(dumpPath) && statSync(dumpPath).size > 0) {
      dumped = true
      break
    }
    execFileSync('sleep', ['0.5'])
  }
  assert(dumped, 'killed-migration scenario never produced a dump')
  run('docker', ['kill', `${containerName}-wrapper`])
  assert(existsSync(dumpPath), 'a killed migration container must leave the dump on the volume')
  report.push('killed migration container leaves the dump for manual recovery')

  for (const line of report) console.log(`ok - ${line}`)
  console.log('Migration release contract passed.')
} finally {
  attempt('docker', ['rm', '--force', `${containerName}-wrapper`])
  attempt('docker', ['rm', '--force', containerName])
  rmSync(directory, { recursive: true, force: true })
}
