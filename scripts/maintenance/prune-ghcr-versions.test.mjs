import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const script = fileURLToPath(new URL('./prune-ghcr-versions.sh', import.meta.url))

function version(id, createdAt, tags) {
  return { id, created_at: createdAt, metadata: { container: { tags } } }
}

// Two release streams plus one version that carries tags from both streams.
const versions = [
  version(1, '2026-01-01T00:00:00Z', ['dev-arm64-01']),
  version(2, '2026-01-02T00:00:00Z', ['dev-arm64-02']),
  version(3, '2026-01-03T00:00:00Z', ['release-x86-01']),
  version(4, '2026-01-04T00:00:00Z', ['dev-arm64-03']),
  version(5, '2026-01-05T00:00:00Z', ['release-x86-02']),
  version(6, '2026-01-06T00:00:00Z', ['dev-arm64-04']),
  version(7, '2026-01-07T00:00:00Z', ['dev-arm64-05', 'release-x86-03']),
  version(8, '2026-01-08T00:00:00Z', ['dev-arm64-06']),
  version(9, '2026-01-09T00:00:00Z', ['release-x86-04']),
]

function runScript({ tagPrefix = 'dev-arm64-', keep = '2' } = {}) {
  const directory = mkdtempSync(join(tmpdir(), 'airtek-prune-'))
  const deleted = join(directory, 'deleted.txt')
  writeFileSync(join(directory, 'versions.json'), JSON.stringify([versions]))
  writeFileSync(join(directory, 'gh'), `#!/usr/bin/env bash
set -euo pipefail
case "\$1 \$2" in
  'api users/'*) printf 'User\\n' ;;
  'api --paginate') cat "${directory}/versions.json" ;;
  'api --method')
    printf '%s\\n' "\${4##*/}" >>"${deleted}"
    ;;
  *) echo "unexpected gh invocation: \$*" >&2; exit 1 ;;
esac
`, { mode: 0o755 })
  chmodSync(join(directory, 'gh'), 0o755)
  const result = (() => {
    try {
      return {
        status: 0,
        stdout: execFileSync('bash', [script], {
          encoding: 'utf8',
          env: {
            ...process.env,
            PATH: `${directory}:${process.env.PATH}`,
            GH_TOKEN: 'test-token',
            PACKAGE_OWNER: 'he0xd4c0',
            PACKAGE_NAME: 'airtekpower-platform',
            KEEP_RELEASE_VERSIONS: keep,
            RETENTION_TAG_PREFIX: tagPrefix,
          },
          stdio: ['ignore', 'pipe', 'pipe'],
        }),
      }
    } catch (error) {
      return { status: error.status, stdout: error.stdout ?? '', stderr: error.stderr ?? '' }
    }
  })()
  const removed = existsSync(deleted) ? readFileSync(deleted, 'utf8').trim().split('\n').filter(Boolean) : []
  rmSync(directory, { recursive: true, force: true })
  return { ...result, removed }
}

test('the development stream prunes only its own oldest versions', () => {
  const result = runScript({ tagPrefix: 'dev-arm64-', keep: '2' })
  assert.equal(result.status, 0, result.stderr)
  // Newest two dev versions are 6 and 8; version 7 carries a release tag and is never touched.
  assert.deepEqual(result.removed.sort(), ['1', '2', '4'])
  assert.match(result.stdout, /keeping the newest 2/u)
})

test('the production stream keeps its own independent version set', () => {
  const result = runScript({ tagPrefix: 'release-x86-', keep: '2' })
  assert.equal(result.status, 0, result.stderr)
  // Newest two release versions are 9 and 5; mixed version 7 is excluded entirely.
  assert.deepEqual(result.removed.sort(), ['3'])
})

test('nothing is removed when a stream is already within its retention window', () => {
  const result = runScript({ tagPrefix: 'release-x86-', keep: '5' })
  assert.equal(result.status, 0, result.stderr)
  assert.deepEqual(result.removed, [])
  assert.match(result.stdout, /nothing to prune/u)
})

test('an unset stream prefix prunes the package as a whole', () => {
  const result = runScript({ tagPrefix: '', keep: '2' })
  assert.equal(result.status, 0, result.stderr)
  // Every tagged version competes once no stream filter is applied: the two
  // newest (ids 8 and 9) survive and the rest are pruned oldest-first.
  assert.deepEqual(result.removed.sort(), ['1', '2', '3', '4', '5', '6', '7'])
})

test('a malformed stream prefix is rejected', () => {
  const result = runScript({ tagPrefix: 'dev', keep: '2' })
  assert.notEqual(result.status, 0)
  assert.equal(result.removed.length, 0)
})
