// Cross-source gate for administrator permission keys.
//
// The same keys are declared in five places: the migration seeds (the database
// authority), the Rust super-admin policy, the frontend Permission union, the
// frontend contract whitelist and the generated Admin operation matrix. A key
// that exists in only some of them silently breaks role grants or the Admin UI,
// so this check fails the architecture gate when they drift apart.
import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = dirname(dirname(dirname(fileURLToPath(import.meta.url))))

/// Keys the database seeds that the API never grants through the shared policy:
/// the DevTools PTY key is compiled in only for the devtools feature, and the
/// retired general-operations key has no route policy anymore.
export const SEED_ONLY_KEYS = new Set(['devtools.shell', 'operations.run'])

export const PERMISSION_KEY = /^[a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+$/u

export function extractRustKeys(source) {
  const body = /super_admin_permissions\(\)[\s\S]*?let mut values = \[([\s\S]*?)\]/u.exec(source)?.[1]
  if (body === undefined) throw new Error('Cannot locate super_admin_permissions() in the Rust policy.')
  return new Set([...body.matchAll(/"([^"]+)"/gu)].map((match) => match[1]).filter((key) => PERMISSION_KEY.test(key)))
}

export function extractMigrationKeys(files) {
  const keys = new Set()
  for (const body of files) {
    for (const statement of body.matchAll(/INSERT INTO permissions\s*\(?\s*key[^)]*\)?\s*VALUES([\s\S]*?);/giu)) {
      for (const row of statement[1].matchAll(/\(\s*'([^']+)'\s*,/gu)) {
        if (PERMISSION_KEY.test(row[1])) keys.add(row[1])
      }
    }
  }
  return keys
}

export function extractFrontendUnionKeys(source) {
  const body = /export type Permission =([\s\S]*?)\n\n/u.exec(source)?.[1]
  if (body === undefined) throw new Error('Cannot locate the frontend Permission union.')
  return new Set([...body.matchAll(/\|\s*'([^']+)'/gu)].map((match) => match[1]).filter((key) => PERMISSION_KEY.test(key)))
}

export function extractWhitelistKeys(source) {
  const body = /function permission\(value: string\)[\s\S]*?function sessionUser\(/u.exec(source)?.[0]
  if (body === undefined) throw new Error('Cannot locate the frontend permission whitelist.')
  return new Set([...body.matchAll(/case '([^']+)'/gu)].map((match) => match[1]).filter((key) => PERMISSION_KEY.test(key)))
}

export function extractMatrixKeys(matrix) {
  const keys = new Set()
  for (const operation of matrix.operations ?? []) {
    for (const token of String(operation.permission ?? '').split('|')) {
      const key = token.trim()
      if (PERMISSION_KEY.test(key)) keys.add(key)
    }
  }
  return keys
}

function difference(left, right) {
  return [...left].filter((key) => !right.has(key)).sort()
}

export function auditPermissionKeys({ rustKeys, seedKeys, unionKeys, whitelistKeys, matrixKeys }) {
  const failures = []
  const devtools = [...unionKeys].filter((key) => key === 'devtools.shell')
  const unionWithoutDevtools = new Set([...unionKeys].filter((key) => !SEED_ONLY_KEYS.has(key)))
  const rustWithoutDevtools = new Set([...rustKeys].filter((key) => !SEED_ONLY_KEYS.has(key)))

  const onlyRust = difference(rustWithoutDevtools, unionWithoutDevtools)
  if (onlyRust.length) {
    failures.push(`Rust policy keys missing from the frontend Permission union: ${onlyRust.join(', ')}`)
  }
  const onlyUnion = difference(unionWithoutDevtools, rustWithoutDevtools)
  if (onlyUnion.length) {
    failures.push(`Frontend Permission keys missing from the Rust policy: ${onlyUnion.join(', ')}`)
  }
  const whitelistVsRust = difference(whitelistKeys, rustWithoutDevtools)
  if (whitelistVsRust.length) {
    failures.push(`Frontend whitelist accepts unmanaged keys: ${whitelistVsRust.join(', ')}`)
  }
  const missingWhitelist = difference(rustWithoutDevtools, whitelistKeys)
  if (missingWhitelist.length) {
    failures.push(`Frontend whitelist drops managed keys: ${missingWhitelist.join(', ')}`)
  }
  const missingSeeds = difference(rustWithoutDevtools, seedKeys)
  if (missingSeeds.length) {
    failures.push(`Rust policy keys are not seeded by any migration: ${missingSeeds.join(', ')}`)
  }
  const unlistedSeeds = difference(seedKeys, new Set([...rustWithoutDevtools, ...SEED_ONLY_KEYS]))
  if (unlistedSeeds.length) {
    failures.push(`Migration seeds unknown keys not covered by SEED_ONLY_KEYS: ${unlistedSeeds.join(', ')}`)
  }
  if (devtools.length && !seedKeys.has('devtools.shell')) {
    failures.push('The frontend DevTools permission is not seeded by any migration.')
  }
  const unknownMatrix = difference(matrixKeys, seedKeys)
  if (unknownMatrix.length) {
    failures.push(`Admin operation matrix references unknown keys: ${unknownMatrix.join(', ')}`)
  }
  return failures
}

function loadSources() {
  const migrationsDirectory = join(root, 'services/platform/migrations')
  const migrationFiles = existsSync(migrationsDirectory)
    ? readdirSync(migrationsDirectory)
      .filter((name) => /^V\d{4}__.+\.sql$/u.test(name))
      .sort()
      .map((name) => readFileSync(join(migrationsDirectory, name), 'utf8'))
    : []
  return {
    rustKeys: extractRustKeys(
      readFileSync(join(root, 'services/platform/crates/runtime/src/auth/permissions.rs'), 'utf8'),
    ),
    seedKeys: extractMigrationKeys(migrationFiles),
    unionKeys: extractFrontendUnionKeys(
      readFileSync(join(root, 'apps/admin/src/shared/types/domain.ts'), 'utf8'),
    ),
    whitelistKeys: extractWhitelistKeys(
      readFileSync(join(root, 'apps/admin/src/shared/services/adminAuthApi.ts'), 'utf8'),
    ),
    matrixKeys: extractMatrixKeys(
      JSON.parse(readFileSync(join(root, 'packages/contracts/admin-operation-matrix.json'), 'utf8')),
    ),
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const sources = loadSources()
  const failures = auditPermissionKeys(sources)
  if (failures.length) {
    console.error(failures.map((failure) => `- ${failure}`).join('\n'))
    process.exit(1)
  }
  console.log(
    `Permission key check passed: ${sources.rustKeys.size} managed keys agree across Rust policy, migrations, frontend types, frontend whitelist and the operation matrix.`,
  )
}
