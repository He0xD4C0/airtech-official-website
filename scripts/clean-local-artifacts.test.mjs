import assert from 'node:assert/strict'
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import test, { afterEach } from 'node:test'

import { runCleanup } from './clean-local-artifacts.mjs'

const fixtureRoots = []

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'airtek-clean-local-'))
  fixtureRoots.push(root)
  writeFileSync(join(root, 'package.json'), '{"name":"airtekpower-platform"}\n')
  mkdirSync(join(root, 'services/platform'), { recursive: true })
  writeFileSync(join(root, 'services/platform/Cargo.toml'), '[package]\nname="fixture"\n')
  return root
}

function addFile(root, path, content = 'generated') {
  const absolutePath = join(root, path)
  mkdirSync(dirname(absolutePath), { recursive: true })
  writeFileSync(absolutePath, content)
}

afterEach(() => {
  for (const root of fixtureRoots.splice(0)) rmSync(root, { force: true, recursive: true })
})

test('defaults to a dry run and preserves every target', () => {
  const root = fixture()
  addFile(root, 'services/platform/target/debug/cache')
  addFile(root, '.local/qa/result.txt')

  const result = runCleanup({ root, cwd: root, log: () => {} })

  assert.equal(result.applied, false)
  assert.equal(existsSync(join(root, 'services/platform/target/debug/cache')), true)
  assert.equal(existsSync(join(root, '.local/qa/result.txt')), true)
})

test('apply removes only whitelisted outputs and uses Cargo for its target', () => {
  const root = fixture()
  addFile(root, 'services/platform/target/debug/cache')
  addFile(root, 'apps/admin/dist/index.html')
  addFile(root, '.local/qa/result.txt')
  addFile(root, 'node_modules/keep.txt')
  addFile(root, '.local/deliverables/contracts/keep.md')

  const result = runCleanup({
    root,
    cwd: root,
    apply: true,
    log: () => {},
    cargoAvailable: () => true,
    cargoClean: () => rmSync(join(root, 'services/platform/target'), { recursive: true }),
  })

  assert.equal(result.applied, true)
  assert.equal(existsSync(join(root, 'services/platform/target')), false)
  assert.equal(existsSync(join(root, 'apps/admin/dist')), false)
  assert.equal(existsSync(join(root, '.local/qa')), false)
  assert.equal(existsSync(join(root, 'node_modules/keep.txt')), true)
  assert.equal(existsSync(join(root, '.local/deliverables/contracts/keep.md')), true)
})

test('Cargo preflight failure leaves all targets untouched', () => {
  const root = fixture()
  addFile(root, 'services/platform/target/debug/cache')
  addFile(root, '.local/qa/result.txt')

  assert.throws(
    () => runCleanup({
      root,
      cwd: root,
      apply: true,
      log: () => {},
      cargoAvailable: () => false,
    }),
    /Cargo is unavailable/u,
  )
  assert.equal(existsSync(join(root, 'services/platform/target/debug/cache')), true)
  assert.equal(existsSync(join(root, '.local/qa/result.txt')), true)
})

test('rejects execution outside the repository root', () => {
  const root = fixture()
  assert.throws(
    () => runCleanup({ root, cwd: tmpdir(), log: () => {} }),
    /Run this command from the repository root/u,
  )
})

test('rejects a symlinked cleanup target before deleting anything', () => {
  const root = fixture()
  const outside = mkdtempSync(join(tmpdir(), 'airtek-clean-outside-'))
  fixtureRoots.push(outside)
  writeFileSync(join(outside, 'keep.txt'), 'keep')
  mkdirSync(join(root, '.local'))
  symlinkSync(outside, join(root, '.local/qa'))

  assert.throws(
    () => runCleanup({ root, cwd: root, apply: true, log: () => {} }),
    /symbolic link/u,
  )
  assert.equal(existsSync(join(outside, 'keep.txt')), true)
})
