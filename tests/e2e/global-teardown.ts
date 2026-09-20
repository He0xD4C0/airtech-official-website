import { rm } from 'node:fs/promises'
import type { FullConfig } from '@playwright/test'
import { adminStorageStatePath, adminSecondaryStorageStatePath, adminTotpSecretPath, runAdminWorkflows } from './support/environment'

export default async function globalTeardown(_config: FullConfig): Promise<void> {
  if (!runAdminWorkflows) return
  // The owning runner removes the complete isolated volume; never delete rows from an inherited DB.
  for (const path of [adminStorageStatePath, adminSecondaryStorageStatePath, adminTotpSecretPath]) await rm(path, { force: true })
}
