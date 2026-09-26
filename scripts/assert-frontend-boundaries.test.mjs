import assert from 'node:assert/strict'
import test from 'node:test'
import { inspectImports } from './assert-frontend-boundaries.mjs'

const resolve = specifier => specifier.replace(/^@\//u, '')

test('rejects shared-to-feature and feature-to-app imports', () => {
  assert.equal(inspectImports("import x from '@/features/catalog/index.ts'", 'shared/a.ts', resolve).length, 1)
  assert.equal(inspectImports("import x from '@/app/layout.ts'", 'features/catalog/a.ts', resolve).length, 1)
})

test('permits public feature entrypoints but rejects deep static and dynamic imports', () => {
  assert.deepEqual(inspectImports("import x from '@/features/media/index.ts'", 'features/catalog/a.ts', resolve), [])
  assert.equal(inspectImports("const x = import('@/features/media/private.ts')", 'features/catalog/a.ts', resolve).length, 1)
  assert.equal(inspectImports("export { x } from '@/features/media/private.ts'", 'features/catalog/index.ts', resolve).length, 1)
})

test('checks Vue scripts and keeps server imports out of browser features', () => {
  const source = "<script setup>import x from '@/server/database.ts'</script><template>safe</template>"
  assert.equal(inspectImports(source, 'features/content/Page.vue', resolve).length, 1)
})
