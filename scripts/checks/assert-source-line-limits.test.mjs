import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test, { afterEach } from 'node:test'

import { auditSourceLines, logicalLineCount, sha256 } from './assert-source-line-limits.mjs'

const fixtureRoots = []

function fixture(files) {
  const root = mkdtempSync(join(tmpdir(), 'airtek-source-lines-'))
  fixtureRoots.push(root)
  for (const [path, content] of Object.entries(files)) {
    const absolutePath = join(root, path)
    mkdirSync(join(absolutePath, '..'), { recursive: true })
    writeFileSync(absolutePath, content)
  }
  return root
}

afterEach(() => {
  for (const root of fixtureRoots.splice(0)) rmSync(root, { force: true, recursive: true })
})

const lines = (count, value = 'line') => Array.from({ length: count }, () => value).join('\n') + '\n'

test('logicalLineCount ignores one terminal newline', () => {
  assert.equal(logicalLineCount(''), 0)
  assert.equal(logicalLineCount('one\n'), 1)
  assert.equal(logicalLineCount('one\ntwo\n'), 2)
})

test('accepts source files at the limit', () => {
  const root = fixture({ 'apps/example.ts': lines(500) })
  const result = auditSourceLines(root, { grandfatheredSources: new Map() })
  assert.deepEqual(result.violations, [])
})

test('rejects ordinary and SQL source files above the limit', () => {
  const root = fixture({
    'apps/example.ts': lines(501),
    'playwright.config.ts': lines(501),
    'services/platform/migrations/V0099__too_large.sql': lines(501, 'SELECT 1;'),
  })
  const result = auditSourceLines(root, { grandfatheredSources: new Map() })
  assert.equal(result.violations.length, 3)
  assert.match(result.violations[0].detail, /501 lines exceeds/u)
  assert.match(result.violations[1].detail, /501 lines exceeds/u)
  assert.match(result.violations[2].detail, /501 lines exceeds/u)
})

test('accepts an unchanged pinned historical exception', () => {
  const content = lines(522, 'SELECT 1;')
  const path = 'services/platform/migrations/V0001__platform_foundation.sql'
  const root = fixture({ [path]: content })
  const grandfatheredSources = new Map([[path, { lines: 522, sha256: sha256(content) }]])
  assert.deepEqual(auditSourceLines(root, { grandfatheredSources }).violations, [])
})

test('rejects growth or same-length edits to a pinned exception', () => {
  const path = 'services/platform/migrations/V0001__platform_foundation.sql'
  const original = lines(522, 'SELECT 1;')
  const grandfatheredSources = new Map([[path, { lines: 522, sha256: sha256(original) }]])
  const grown = auditSourceLines(fixture({ [path]: lines(523, 'SELECT 1;') }), {
    grandfatheredSources,
  })
  const edited = auditSourceLines(fixture({ [path]: lines(522, 'SELECT 2;') }), {
    grandfatheredSources,
  })
  assert.match(grown.violations[0].detail, /immutable exception changed/u)
  assert.match(edited.violations[0].detail, /immutable exception changed/u)
})

test('ignores docs and non-source data files', () => {
  const root = fixture({
    'docs/demo.html': lines(800),
    'packages/contracts/openapi.json': lines(800, '{}'),
    'services/platform/Cargo.lock': lines(800),
  })
  assert.deepEqual(
    auditSourceLines(root, { grandfatheredSources: new Map() }).violations,
    [],
  )
})
