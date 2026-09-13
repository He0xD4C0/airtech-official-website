import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { extname, join, relative } from 'node:path'
import { gzipSync } from 'node:zlib'

const ROOT = process.cwd()
const ADMIN_DIST = join(ROOT, 'apps/admin/dist')
const SOURCE_ROOTS = ['apps', 'packages', 'services', 'scripts', 'e2e']
const SOURCE_EXTENSIONS = new Set(['.css', '.html', '.js', '.mjs', '.rs', '.ts', '.tsx', '.vue'])
const SKIPPED_DIRECTORIES = new Set(['dist', 'docs', 'node_modules', 'target'])
const MAX_SOURCE_LINES = 500
const MAX_EDITOR_ENTRY_GZIP = 60 * 1024
const MAX_LAZY_CHUNK_GZIP = 100 * 1024

function sourceFiles(directory) {
  if (!existsSync(directory)) return []
  const files = []
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && SKIPPED_DIRECTORIES.has(entry.name)) continue
    const path = join(directory, entry.name)
    if (entry.isDirectory()) files.push(...sourceFiles(path))
    else if (SOURCE_EXTENSIONS.has(extname(entry.name))) files.push(path)
  }
  return files
}

function lineCount(path) {
  const content = readFileSync(path, 'utf8')
  if (!content) return 0
  return content.split(/\r?\n/u).length
}

function gzipBytes(path) {
  return gzipSync(readFileSync(path), { level: 9 }).byteLength
}

const oversizedSources = SOURCE_ROOTS
  .flatMap((directory) => sourceFiles(join(ROOT, directory)))
  .map((path) => ({ path, lines: lineCount(path) }))
  .filter(({ lines }) => lines > MAX_SOURCE_LINES)

if (oversizedSources.length) {
  throw new Error(`Source files exceed ${MAX_SOURCE_LINES} lines:\n${oversizedSources
    .map(({ path, lines }) => `- ${relative(ROOT, path)}: ${lines}`)
    .join('\n')}`)
}

if (!existsSync(ADMIN_DIST)) {
  throw new Error('Admin production build is missing. Run pnpm --filter @airtek/admin build:production first.')
}

const javascriptAssets = sourceFiles(ADMIN_DIST).filter((path) => extname(path) === '.js')
const oversizedChunks = javascriptAssets
  .map((path) => ({ path, bytes: gzipBytes(path) }))
  .filter(({ bytes }) => bytes >= MAX_LAZY_CHUNK_GZIP)

if (oversizedChunks.length) {
  throw new Error(`Admin chunks exceed the ${MAX_LAZY_CHUNK_GZIP / 1024} KiB gzip budget:\n${oversizedChunks
    .map(({ path, bytes }) => `- ${relative(ROOT, path)}: ${(bytes / 1024).toFixed(2)} KiB`)
    .join('\n')}`)
}

const editorEntries = javascriptAssets.filter((path) => /\/ContentEditorShell-[^/]+\.js$/u.test(path))
if (editorEntries.length !== 1) {
  throw new Error(`Expected exactly one ContentEditorShell JavaScript chunk, found ${editorEntries.length}.`)
}

const editorEntryBytes = gzipBytes(editorEntries[0])
if (editorEntryBytes >= MAX_EDITOR_ENTRY_GZIP) {
  throw new Error(`ContentEditorShell exceeds the ${MAX_EDITOR_ENTRY_GZIP / 1024} KiB gzip budget: ${(editorEntryBytes / 1024).toFixed(2)} KiB.`)
}

const largestChunks = javascriptAssets
  .map((path) => ({ path, bytes: gzipBytes(path) }))
  .sort((left, right) => right.bytes - left.bytes)
  .slice(0, 3)

console.log(`Admin UI boundary passed: ${editorEntries.length} editor entry at ${(editorEntryBytes / 1024).toFixed(2)} KiB gzip; ${javascriptAssets.length} JavaScript chunks below 100 KiB gzip; all source files at or below 500 lines.`)
for (const { path, bytes } of largestChunks) {
  console.log(`- ${relative(ROOT, path)}: ${(bytes / 1024).toFixed(2)} KiB gzip`)
}
