import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { findDependencyCopies } from './check-dependency-hygiene.mjs'

test('detects numbered package links including scoped and dangling links', () => {
  const root = mkdtempSync(join(tmpdir(), 'airtek-dependencies-'))
  try {
    const directory = join(root, 'apps/admin/node_modules/@scope')
    mkdirSync(directory, { recursive: true })
    symlinkSync('missing', join(directory, 'package'))
    symlinkSync('missing-old', join(directory, 'package 2'))
    writeFileSync(join(directory, 'legitimate 2'), '')
    assert.deepEqual(findDependencyCopies(root), ['apps/admin/node_modules/@scope/package 2'])
    assert.equal(findDependencyCopies(root).length, 1)
  } finally { rmSync(root, { recursive: true }) }
})
