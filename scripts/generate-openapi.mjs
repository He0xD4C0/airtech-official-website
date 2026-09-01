import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const snapshot = join(root, 'packages/contracts/openapi/openapi.production.json')
const generated = join(root, 'packages/contracts/src/generated/openapi.ts')
const check = process.argv.includes('--check')
const scratch = mkdtempSync(join(tmpdir(), 'airtek-openapi-'))
const candidateSnapshot = check ? join(scratch, 'openapi.production.json') : snapshot
const candidateGenerated = check ? join(scratch, 'openapi.ts') : generated

try {
  const cargo = process.env.CARGO || 'cargo'
  const raw = execFileSync(cargo, [
    'run',
    '--quiet',
    '--locked',
    '--manifest-path',
    'services/platform/Cargo.toml',
    '--example',
    'export_openapi',
    '--features',
    'production',
  ], {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
  })
  const document = JSON.parse(raw)
  assertProductionDocument(document)
  assertRustRouteInventory(document)
  mkdirSync(dirname(candidateSnapshot), { recursive: true })
  mkdirSync(dirname(candidateGenerated), { recursive: true })
  writeFileSync(candidateSnapshot, `${JSON.stringify(document, null, 2)}\n`)

  const pnpm = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm'
  execFileSync(pnpm, [
    'exec',
    'openapi-typescript',
    candidateSnapshot,
    '--output',
    candidateGenerated,
    '--alphabetize',
    '--default-non-nullable',
    'false',
    '--root-types',
    '--root-types-no-schema-prefix',
  ], {
    cwd: root,
    stdio: 'inherit',
  })

  if (check) {
    compare(snapshot, candidateSnapshot, 'OpenAPI snapshot')
    compare(generated, candidateGenerated, 'generated TypeScript contract')
    console.log('OpenAPI snapshot and generated TypeScript contract are current.')
  } else {
    console.log('Updated production OpenAPI snapshot and generated TypeScript contract.')
  }
} finally {
  rmSync(scratch, { recursive: true, force: true })
}

function assertProductionDocument(document) {
  if (document?.['x-airtek-build'] !== 'production') {
    throw new Error('Contract export must be compiled with the production feature.')
  }
  const serialized = JSON.stringify(document).toLowerCase()
  if (serialized.includes('/api/devtools/') || serialized.includes('terminaltoken')) {
    throw new Error('Production OpenAPI contract contains a DevTools route or schema.')
  }
  if (!document?.components?.schemas?.ProblemDetails) {
    throw new Error('ProblemDetails is missing from the production OpenAPI contract.')
  }
}

function assertRustRouteInventory(document) {
  const sources = [
    ['services/platform/src/routes/system.rs', ''],
    ['services/platform/src/routes/public.rs', '/api/public/v1'],
    ['services/platform/src/auth.rs', '/api/admin/v1'],
    ['services/platform/src/routes/admin.rs', '/api/admin/v1'],
  ]
  const registered = new Set()
  for (const [relativePath, prefix] of sources) {
    const source = readFileSync(join(root, relativePath), 'utf8')
    const routerStart = source.indexOf('pub fn router()')
    if (routerStart < 0) throw new Error(`Unable to find router() in ${relativePath}.`)
    const routerEnd = source.indexOf('\n}', routerStart)
    const router = source.slice(routerStart, routerEnd < 0 ? source.length : routerEnd)
    const matches = [...router.matchAll(/\.route\(\s*"([^"]+)"/g)]
    for (let index = 0; index < matches.length; index += 1) {
      const match = matches[index]
      const next = matches[index + 1]
      const segment = router.slice(match.index, next?.index ?? router.length)
      const methods = [...segment.matchAll(/\b(get|post|put|patch|delete)\s*\(/g)]
      for (const method of methods) registered.add(`${method[1]} ${prefix}${match[1]}`)
    }
  }

  const documented = new Set()
  for (const [path, pathItem] of Object.entries(document.paths || {})) {
    for (const method of Object.keys(pathItem)) {
      if (['get', 'post', 'put', 'patch', 'delete'].includes(method)) documented.add(`${method} ${path}`)
    }
  }
  const missing = [...registered].filter((operation) => !documented.has(operation)).sort()
  const stale = [...documented].filter((operation) => !registered.has(operation)).sort()
  if (missing.length || stale.length) {
    throw new Error([
      'Rust router and production OpenAPI operation inventory differ.',
      missing.length ? `Missing from OpenAPI: ${missing.join(', ')}` : '',
      stale.length ? `Not registered by Rust: ${stale.join(', ')}` : '',
    ].filter(Boolean).join('\n'))
  }
}

function compare(expectedPath, actualPath, label) {
  if (!existsSync(expectedPath)) {
    throw new Error(`${label} is missing. Run pnpm generate:contracts.`)
  }
  const expected = readFileSync(expectedPath, 'utf8').replaceAll('\r\n', '\n')
  const actual = readFileSync(actualPath, 'utf8').replaceAll('\r\n', '\n')
  if (expected !== actual) {
    throw new Error(`${label} drift detected. Run pnpm generate:contracts and commit the result.`)
  }
}
