import { execFileSync } from 'node:child_process'
import { rm } from 'node:fs/promises'
import type { FullConfig } from '@playwright/test'
import { adminStorageStatePath, runAdminWorkflows } from './support/environment'

export default async function globalTeardown(_config: FullConfig): Promise<void> {
  if (!runAdminWorkflows) return
  const database = process.env.POSTGRES_DB ?? 'airtek'
  const user = process.env.POSTGRES_USER ?? 'airtek'
  try {
    execFileSync(
      'docker',
      ['compose', 'exec', '-T', 'postgres', 'psql', '-X', '-v', 'ON_ERROR_STOP=1', '-U', user, '-d', database],
      {
        input: "DELETE FROM guest_source_daily WHERE landing_path='/en/e2e-admin-analytics';\n",
        stdio: ['pipe', 'inherit', 'inherit'],
      },
    )
  } finally {
    await rm(adminStorageStatePath, { force: true })
  }
}
