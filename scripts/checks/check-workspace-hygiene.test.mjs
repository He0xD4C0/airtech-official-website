import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import test, { afterEach } from 'node:test'

import {
  checkWorkspaceHygiene,
  findEmptyDirectories,
  findNumberedCopies,
} from './check-workspace-hygiene.mjs'

const fixtureRoots = []

function fixture(files) {
  const root = mkdtempSync(join(tmpdir(), 'airtek-workspace-hygiene-'))
  fixtureRoots.push(root)
  writeFileSync(join(root, 'package.json'), '{"name":"airtekpower-platform"}\n')
  for (const [path, content] of Object.entries(files)) {
    const absolutePath = join(root, path)
    mkdirSync(dirname(absolutePath), { recursive: true })
    writeFileSync(absolutePath, content)
  }
  return root
}

afterEach(() => {
  for (const root of fixtureRoots.splice(0)) rmSync(root, { force: true, recursive: true })
})

test('reports an identical numbered copy beside its canonical file', () => {
  const root = fixture({ 'apps/view.ts': 'same', 'apps/view 2.ts': 'same' })
  assert.deepEqual(findNumberedCopies(root), [{
    canonical: 'apps/view.ts',
    duplicate: 'apps/view 2.ts',
    identical: true,
  }])
})

test('reports a divergent numbered copy', () => {
  const root = fixture({ 'apps/view.ts': 'original', 'apps/view 2.ts': 'changed' })
  assert.equal(findNumberedCopies(root)[0].identical, false)
})

test('accepts a legitimate numbered filename without a canonical counterpart', () => {
  const root = fixture({ 'docs/chapter 2.md': 'content' })
  assert.doesNotThrow(() => checkWorkspaceHygiene({ root, cwd: root, log: () => {} }))
})

test('ignores generated and dependency directories', () => {
  const root = fixture({
    'node_modules/tool.js': 'same',
    'node_modules/tool 2.js': 'same',
    'services/platform/target/item.rs': 'same',
    'services/platform/target/item 2.rs': 'same',
  })
  assert.deepEqual(findNumberedCopies(root), [])
  assert.deepEqual(findEmptyDirectories(root), [])
})

test('fails the workspace check when a copy conflict exists', () => {
  const root = fixture({ 'scripts/task.mjs': 'same', 'scripts/task 2.mjs': 'same' })
  assert.throws(
    () => checkWorkspaceHygiene({ root, cwd: root, log: () => {} }),
    /1 numbered copies and 0 empty directories/u,
  )
})

test('reports an empty source directory', () => {
  const root = fixture({})
  mkdirSync(join(root, 'apps/empty'), { recursive: true })
  assert.deepEqual(findEmptyDirectories(root), ['apps/empty'])
  assert.throws(
    () => checkWorkspaceHygiene({ root, cwd: root, log: () => {} }),
    /0 numbered copies and 1 empty directories/u,
  )
})
