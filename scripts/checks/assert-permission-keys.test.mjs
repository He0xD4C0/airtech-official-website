import assert from 'node:assert/strict'
import test from 'node:test'

import {
  auditPermissionKeys,
  extractFrontendUnionKeys,
  extractMatrixKeys,
  extractMigrationKeys,
  extractRustKeys,
  extractWhitelistKeys,
} from './assert-permission-keys.mjs'

const rustSource = `
pub(super) fn super_admin_permissions() -> Vec<String> {
    let mut values = ["dashboard.read", "settings.manage"]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    #[cfg(feature = "devtools")]
    values.push("devtools.shell".into());
    values
}
`

test('rust policy extracts the managed keys without the feature-gated DevTools key', () => {
  assert.deepEqual([...extractRustKeys(rustSource)].sort(), ['dashboard.read', 'settings.manage'])
})

test('migration seeds and frontend sources extract their key sets', () => {
  const seeds = extractMigrationKeys([
    `INSERT INTO permissions (key, description) VALUES
       ('dashboard.read', 'Read dashboard summaries'),
       ('settings.manage', 'Manage platform settings'),
       ('devtools.shell', 'Open the development-only host PTY')
     ON CONFLICT (key) DO NOTHING;`,
  ])
  assert.deepEqual([...seeds].sort(), ['dashboard.read', 'devtools.shell', 'settings.manage'])

  const union = extractFrontendUnionKeys(`export type Permission =
  | 'dashboard.read'
  | 'settings.manage'
  | 'devtools.shell'

export interface SessionUser {`)
  assert.deepEqual([...union].sort(), ['dashboard.read', 'devtools.shell', 'settings.manage'])

  const whitelist = extractWhitelistKeys(`function permission(value: string): Permission | undefined {
  switch (value) {
    case 'dashboard.read':
    case 'settings.manage':
      return value
    default:
      return undefined
  }
}

function sessionUser(value: unknown): void {}`)
  assert.deepEqual([...whitelist].sort(), ['dashboard.read', 'settings.manage'])
})

test('matrix tokens keep only managed permission keys', () => {
  const matrix = {
    operations: [
      { permission: 'authenticated-or-public-auth-flow' },
      { permission: 'content.read | media.write' },
      { permission: 'authenticated; metrics filtered by caller permissions' },
    ],
  }
  assert.deepEqual([...extractMatrixKeys(matrix)].sort(), ['content.read', 'media.write'])
})

test('audit passes when every source agrees', () => {
  const failures = auditPermissionKeys({
    rustKeys: new Set(['dashboard.read', 'settings.manage']),
    seedKeys: new Set(['dashboard.read', 'settings.manage', 'devtools.shell']),
    unionKeys: new Set(['dashboard.read', 'settings.manage', 'devtools.shell']),
    whitelistKeys: new Set(['dashboard.read', 'settings.manage']),
    matrixKeys: new Set(['dashboard.read']),
  })
  assert.deepEqual(failures, [])
})

test('audit reports drift across the five sources', () => {
  const failures = auditPermissionKeys({
    rustKeys: new Set(['dashboard.read', 'settings.manage']),
    seedKeys: new Set(['dashboard.read']),
    unionKeys: new Set(['dashboard.read', 'settings.manage', 'extra.read']),
    whitelistKeys: new Set(['dashboard.read']),
    matrixKeys: new Set(['dashboard.read', 'unknown.write']),
  })
  const joined = failures.join('\n')
  assert.match(joined, /not seeded by any migration: settings\.manage/u)
  assert.match(joined, /whitelist drops managed keys: settings\.manage/u)
  assert.match(joined, /Permission keys missing from the Rust policy: extra\.read/u)
  assert.match(joined, /matrix references unknown keys: unknown\.write/u)
})
