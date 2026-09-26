import { createHash } from 'node:crypto'
import { existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, renameSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'

const root = process.cwd()
if (JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).name !== 'airtekpower-platform') {
  throw new Error('Run from the AIRTEKPOWER repository root.')
}
const apply = process.argv.includes('--apply')
const moves = [
  ['deliverables', '.local/deliverables'],
  ['playwright-report', '.local/qa/playwright-report'],
  ['test-results', '.local/qa/test-results'],
  ['dogfood-output', '.local/qa/dogfood'],
].filter(([source]) => existsSync(join(root, source)))

function inventory(directory, prefix = '') {
  return readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))
    .flatMap(entry => {
      const path = join(directory, entry.name)
      const name = join(prefix, entry.name)
      if (entry.isSymbolicLink()) throw new Error(`Refusing to migrate symbolic link: ${path}`)
      if (entry.isDirectory()) return inventory(path, name)
      return [{ path: name, sha256: createHash('sha256').update(readFileSync(path)).digest('hex') }]
    })
}

for (const [source, target] of moves) {
  if (existsSync(join(root, target))) throw new Error(`Destination already exists: ${target}`)
  if (lstatSync(join(root, source)).isSymbolicLink()) throw new Error(`Source is a symbolic link: ${source}`)
  for (let parent = dirname(resolve(root, target)); parent !== root; parent = dirname(parent)) {
    if (existsSync(parent) && lstatSync(parent).isSymbolicLink()) throw new Error(`Symbolic link: ${parent}`)
  }
}
const records = moves.map(([source, target]) => ({ source, target, before: inventory(join(root, source)) }))
if (apply && records.length) {
  const manifestDirectory = join(root, '.local/manifests')
  mkdirSync(manifestDirectory, { recursive: true })
  const manifest = join(manifestDirectory, `artifact-migration-${Date.now()}.json`)
  writeFileSync(manifest, `${JSON.stringify(records, null, 2)}\n`, { flag: 'wx' })
  for (const record of records) {
    mkdirSync(dirname(join(root, record.target)), { recursive: true })
    renameSync(join(root, record.source), join(root, record.target))
    record.after = inventory(join(root, record.target))
    if (JSON.stringify(record.before) !== JSON.stringify(record.after)) throw new Error(`Inventory mismatch: ${record.target}`)
  }
  writeFileSync(manifest, `${JSON.stringify(records, null, 2)}\n`)
  console.log(`Verified migration manifest: ${manifest}`)
}
for (const record of records) console.log(`${record.source} -> ${record.target}: ${record.before.length} files`)
if (!apply) console.log('Preview only; pass --apply to migrate.')
