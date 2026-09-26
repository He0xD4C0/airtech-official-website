import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const uiBase = readFileSync('packages/ui/src/base.css', 'utf8')
const layout = readFileSync('packages/ui/src/layout.css', 'utf8')
const editor = readFileSync('apps/admin/src/features/content/components/ContentEditorShell.vue', 'utf8')

test('declares the native cascade layers in one canonical order', () => {
  assert.match(uiBase, /@import '\.\/layers\.css'/)
  assert.match(readFileSync('packages/ui/src/layers.css', 'utf8'), /reset, tokens, base, layout, components, utilities, overrides/)
})

test('ships the five shared layout primitives', () => {
  for (const className of ['stack', 'cluster', 'sidebar-layout', 'auto-grid', 'scroll-region']) {
    assert.match(layout, new RegExp(`\\.${className}\\b`))
  }
})

test('uses progressive container queries for the content editor', () => {
  assert.match(editor, /container:\s*content-editor\s*\/\s*inline-size/)
  assert.match(editor, /@container content-editor \(min-width: 52rem\)/)
  assert.match(editor, /@container content-editor \(min-width: 72rem\)/)
})
