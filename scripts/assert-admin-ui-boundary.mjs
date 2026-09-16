import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { extname, join, relative } from 'node:path'
import { gzipSync } from 'node:zlib'

const ROOT = process.cwd()
const ADMIN_DIST = join(ROOT, 'apps/admin/dist')
const MAX_EDITOR_ENTRY_GZIP = 60 * 1024
const MAX_LAZY_CHUNK_GZIP = 100 * 1024

function javascriptFiles(directory) {
  if (!existsSync(directory)) return []
  const files = []
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) files.push(...javascriptFiles(path))
    else if (extname(entry.name) === '.js') files.push(path)
  }
  return files
}

function gzipBytes(path) {
  return gzipSync(readFileSync(path), { level: 9 }).byteLength
}

if (!existsSync(ADMIN_DIST)) {
  throw new Error('Admin production build is missing. Run pnpm --filter @airtek/admin build:production first.')
}

const javascriptAssets = javascriptFiles(ADMIN_DIST)
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

console.log(`Admin UI boundary passed: ${editorEntries.length} editor entry at ${(editorEntryBytes / 1024).toFixed(2)} KiB gzip; ${javascriptAssets.length} JavaScript chunks below 100 KiB gzip.`)
for (const { path, bytes } of largestChunks) {
  console.log(`- ${relative(ROOT, path)}: ${(bytes / 1024).toFixed(2)} KiB gzip`)
}
