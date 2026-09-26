import { test } from 'node:test'
import assert from 'node:assert/strict'
import { layers, validateLayers } from './assert-platform-workspace.mjs'

function workspace() {
  return Object.entries(layers).map(([name, dependencies]) => ({
    name,
    dependencies: dependencies.map((name) => ({ name })),
    features: Object.fromEntries(['production', 'devtools'].map((feature) => [feature,
      dependencies.filter((name) => name !== 'airtek-domain').map((name) => `${name}/${feature}`),
    ])),
  }))
}

test('accepts the one-way platform dependency graph', () => {
  assert.deepEqual(validateLayers(workspace()), [])
})
test('rejects reverse dependencies and infrastructure in domain', () => {
  const packages = workspace()
  packages[0].dependencies.push({ name: 'axum' })
  packages[1].dependencies.push({ name: 'airtek-http' })
  assert.equal(validateLayers(packages).length, 2)
})
test('rejects missing feature forwarding', () => {
  const packages = workspace()
  packages.find((item) => item.name === 'airtek-api').features.production = []
  assert.equal(validateLayers(packages).length, 2)
})
