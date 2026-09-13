import { loadEnvFile } from 'node:process'
import { defineConfig, devices } from '@playwright/test'

try {
  loadEnvFile()
} catch (error) {
  if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
}

const hostResolverRules = process.env.E2E_HOST_RESOLVER_RULES
const adminOrigin = process.env.E2E_ADMIN_ORIGIN
const mutableAdminStack = process.env.E2E_RUN_ADMIN_WORKFLOWS === 'true'
const browserArgs = hostResolverRules
  ? [
      `--host-resolver-rules=${hostResolverRules}`,
      // The disposable gateway is HTTP, while the real Admin deployment is
      // HTTPS. Grant only that exact E2E origin a secure context so browser
      // APIs such as crypto.randomUUID remain production-equivalent.
      ...(adminOrigin?.startsWith('http://')
        ? [`--unsafely-treat-insecure-origin-as-secure=${adminOrigin}`]
        : []),
    ]
  : undefined

export default defineConfig({
  testDir: './tests/e2e',
  globalSetup: './tests/e2e/global-setup.ts',
  globalTeardown: './tests/e2e/global-teardown.ts',
  outputDir: 'test-results/playwright',
  // Authenticated mutations rotate a single CSRF token stored in the shared
  // admin session. The disposable full stack intentionally reuses that session,
  // so run it serially instead of invalidating sibling workers' tokens.
  fullyParallel: !mutableAdminStack,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  ...(process.env.CI || mutableAdminStack ? { workers: 1 } : {}),
  reporter: process.env.CI
    ? [['line'], ['html', { open: 'never', outputFolder: 'playwright-report' }]]
    : [['list'], ['html', { open: 'never', outputFolder: 'playwright-report' }]],
  timeout: 30_000,
  expect: {
    timeout: 5_000,
  },
  use: {
    ...devices['Desktop Chrome'],
    ...(browserArgs ? { launchOptions: { args: browserArgs } } : {}),
    actionTimeout: 10_000,
    navigationTimeout: 20_000,
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
})
