import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { extname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(fileURLToPath(new URL('.', import.meta.url)), '../..')
const failures = []
const read = (path) => readFileSync(join(root, path), 'utf8')

function filesUnder(path) {
  const absolute = join(root, path)
  if (!existsSync(absolute)) return []
  return readdirSync(absolute).flatMap((name) => {
    const child = join(absolute, name)
    const relativeChild = relative(root, child).replaceAll('\\', '/')
    return statSync(child).isDirectory()
      ? filesUnder(relativeChild)
      : [relativeChild]
  })
}

function requireText(path, pattern, message) {
  if (!pattern.test(read(path))) failures.push(message)
}

function forbidText(path, pattern, message) {
  if (pattern.test(read(path))) failures.push(message)
}

const migrations = filesUnder('services/platform/migrations')
  .filter((path) => /\/V\d{4}__.+\.sql$/u.test(path))
  .sort()
for (const version of ['0017', '0018', '0019', '0020', '0021']) {
  const matches = migrations.filter((path) => path.includes(`/V${version}__`))
  if (matches.length !== 1) failures.push(`Expected exactly one V${version} migration.`)
  for (const path of matches) {
    const lines = read(path).split(/\r?\n/u).filter((line) => line.trim()).length
    if (lines > 500) failures.push(`${path} has ${lines} logical lines; limit is 500.`)
  }
}
const latestMigration = Number(migrations.at(-1)?.match(/\/V(\d{4})__/u)?.[1] ?? 0)
if (!latestMigration) failures.push('Unable to derive the latest Flyway migration version.')

requireText('services/platform/Cargo.toml', /^rust-version = "1\.88"$/mu, 'Rust MSRV must be 1.88.')
for (const path of [
  '.env.example',
  '.github/workflows/ci.yml',
  'compose.yaml',
  'infra/compose/production.app.yaml',
  'infra/deploy/production.env.example',
  'infra/docker/flyway-entrypoint.sh',
]) {
  requireText(
    path,
    new RegExp(`AIRTEK_FLYWAY_TARGET[^\\n]*${latestMigration}`, 'u'),
    `${path} must target the latest Flyway migration V${latestMigration}.`,
  )
}
requireText(
  'scripts/testing/run-e2e-stack.mjs',
  /readdirSync\('services\/platform\/migrations'\)/u,
  'scripts/testing/run-e2e-stack.mjs must derive the Flyway target from the migration directory.',
)
requireText(
  'scripts/testing/run-e2e-stack.mjs',
  /AIRTEK_FLYWAY_TARGET:\s*String\(latestMigrationVersion\)/u,
  'scripts/testing/run-e2e-stack.mjs must use its derived latest Flyway target.',
)

const rustFiles = [
  ...filesUnder('services/platform/crates'),
  ...filesUnder('services/platform/apps'),
  ...filesUnder('services/platform/tests'),
].filter((path) => extname(path) === '.rs')
const includeUses = rustFiles.flatMap((path) => {
  const matches = [...read(path).matchAll(/include!\s*\(/gu)]
  return matches.map(() => path)
})
if (includeUses.length !== 1 || includeUses[0] !== 'services/platform/crates/runtime/src/flyway.rs') {
  failures.push(`Only the generated Flyway version may use include!: ${includeUses.join(', ')}`)
}

const routeFiles = filesUnder('services/platform/crates/http/src/routes').filter((path) => extname(path) === '.rs')
for (const path of routeFiles) {
  forbidText(
    path,
    /sqlx::|\.fetch_(?:all|one|optional)\s*\(|\.execute\s*\(/u,
    `${path} must not contain route-layer SQL or database execution.`,
  )
}

const productionSources = [
  ...filesUnder('apps/admin/src'),
  ...filesUnder('apps/web/src'),
  ...filesUnder('apps/web/pages'),
  ...filesUnder('services/platform/crates'),
  ...filesUnder('services/platform/apps'),
].filter((path) => /\.(?:rs|ts|tsx|vue)$/u.test(path)
  && !/(?:\/tests?(?:\/|\.)|\.(?:test|spec)\.)/u.test(path))
const retiredCmsPatterns = [
  /\/content\/(?:[^'"`\s/]+\/)?(?:snapshots|revisions|diff)(?:[/'"`?]|$)/u,
  /content-preview/u,
  /RevisionTimeline|RevisionDiff|restoreRevision/u,
]
for (const path of productionSources) {
  const body = read(path)
  if (/infra\/compose\/|compose\.(?:debug|e2e|production|infrastructure|local-production)\./u.test(body)) {
    failures.push(`${path} must not depend on Compose file locations.`)
  }
  for (const pattern of retiredCmsPatterns) {
    if (pattern.test(body)) failures.push(`${path} still references retired CMS history or preview behavior.`)
  }
  if (/\.(?:ts|tsx|vue)$/u.test(path) && /as unknown as/u.test(body)) {
    failures.push(`${path} contains a duplicate unsafe decoder cast.`)
  }
}

for (const entry of readdirSync(join(root, 'scripts'), { withFileTypes: true })) {
  if (entry.isFile() && entry.name.endsWith('.mjs')) failures.push(`Executable script is not grouped by responsibility: ${entry.name}`)
  if (entry.isDirectory() && !['checks', 'generation', 'testing', 'maintenance', 'brand', 'ci'].includes(entry.name)) {
    failures.push(`Unexpected scripts directory: ${entry.name}`)
  }
}

const webPackage = JSON.parse(read('apps/web/package.json'))
const adminPackage = JSON.parse(read('apps/admin/package.json'))
for (const dependency of ['vue', 'pinia', 'typescript', 'vite', 'vue-tsc']) {
  const webVersion = webPackage.dependencies?.[dependency] ?? webPackage.devDependencies?.[dependency]
  const adminVersion = adminPackage.dependencies?.[dependency] ?? adminPackage.devDependencies?.[dependency]
  if (!webVersion || webVersion !== adminVersion) {
    failures.push(`Admin ${dependency} must match Web (${webVersion ?? 'missing'}).`)
  }
}

if (failures.length) {
  throw new Error(`Architecture checks failed:\n${failures.map((failure) => `- ${failure}`).join('\n')}`)
}
console.log(`Architecture checks passed: ${routeFiles.length} route files are SQL-free; CMS V${latestMigration}, MSRV, retired-path, parser, and toolchain boundaries are enforced.`)
