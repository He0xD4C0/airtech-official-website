import { test } from 'node:test'
import assert from 'node:assert/strict'
import { contractObjectStore } from './contract-object-store.mjs'

const name = 'airtek-e2e-0123456789ab-postgres-objects'

test('refuses arbitrary container targets', () => {
  assert.throws(() => contractObjectStore('business-minio'), /isolated test container/)
})
test('refuses an occupied name and never removes the existing container', async () => {
  const calls = []
  const store = contractObjectStore(name, async (...args) => calls.push(args), async () => ({ status: 0, stdout: 'occupied' }))
  await assert.rejects(store.start(), /already in use/)
  assert.equal(await store.cleanup(), true)
  assert.deepEqual(calls, [])
})
test('uses tmpfs, a dynamic loopback port, and removes only its owned container', async () => {
  const calls = []
  const run = async (command, args, options) => { calls.push({ command, args, options }); return 0 }
  const capture = async (_command, args) => ({ status: 0, stdout: args[0] === 'port' ? '127.0.0.1:19099\n' : '' })
  const store = contractObjectStore(name, run, capture)
  assert.equal(await store.start(), 'http://127.0.0.1:19099')
  assert.deepEqual(calls[0].args.slice(0, 4), ['build', '--file', 'infra/docker/Dockerfile.minio', '--tag'])
  assert.deepEqual(calls[1].args.slice(0, 4), ['build', '--file', 'infra/docker/Dockerfile.minio-mc', '--tag'])
  assert.ok(calls[2].args.includes('127.0.0.1::9000'))
  assert.ok(calls[2].args.includes('/data:rw'))
  assert.ok(calls[3].args.includes(`container:${name}`))
  assert.equal(await store.cleanup(), true)
  assert.deepEqual(calls.at(-1), { command: 'docker', args: ['rm', '--force', name], options: { cleanup: true } })
})
