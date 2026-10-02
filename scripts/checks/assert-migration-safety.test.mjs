import assert from 'node:assert/strict'
import test from 'node:test'

import {
  DESTRUCTIVE_MARKER,
  GRANDFATHERED_MIGRATIONS,
  auditMigrationSafety,
} from './assert-migration-safety.mjs'

test('grandfathered migrations keep their released destructive statements', () => {
  const failures = auditMigrationSafety([
    { name: 'V0018__remove_content_history.sql', body: 'DROP TABLE content_drafts;' },
  ])
  assert.deepEqual(failures, [])
  assert.ok(GRANDFATHERED_MIGRATIONS.has('V0018__remove_content_history.sql'))
})

test('later destructive migrations require the explicit marker', () => {
  const failures = auditMigrationSafety([
    { name: 'V0029__drop_legacy_table.sql', body: 'DROP TABLE legacy_records;' },
    { name: 'V0030__clear_rows.sql', body: 'DELETE FROM products WHERE false;' },
  ])
  assert.equal(failures.length, 2)
  assert.match(failures[0], /DROP TABLE/u)
  assert.match(failures[1], /DELETE FROM/u)
})

test('marked destructive migrations and additive migrations pass', () => {
  const marked = '-- airtek:destructive: retires the legacy table after archival\nDROP TABLE legacy_records;'
  assert.ok(DESTRUCTIVE_MARKER.test(marked))
  const failures = auditMigrationSafety([
    { name: 'V0031__retire_legacy.sql', body: marked },
    { name: 'V0032__add_column.sql', body: 'ALTER TABLE products ADD COLUMN note text;' },
  ])
  assert.deepEqual(failures, [])
})

test('marker must carry a non-empty reason', () => {
  const failures = auditMigrationSafety([
    { name: 'V0033__drop_without_reason.sql', body: '-- airtek:destructive:\nDROP TABLE legacy_records;' },
  ])
  assert.equal(failures.length, 1)
})
