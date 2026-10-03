// Runs last: the account-wide failure counter it creates would otherwise make
// every later password sign-in demand the risk SMS challenge.
import { readFile } from 'node:fs/promises'
import { expect, request, test } from '@playwright/test'
import {
  absolute,
  administrator,
  adminOrigin,
  adminTotpSecretPath,
  apiOrigin,
  gatewayControlOrigin,
  gatewayHostHeaders,
  runAdminWorkflows,
} from './support/environment'
import { adminPhone, waitForSmsCode } from './support/delivery'
import { totp } from './support/totp'

test.describe('Admin sign-in risk challenge', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack so the disposable administrator exists.')

  test('requires an SMS challenge after repeated password failures', async ({ browser, request: api }) => {
    // Ten failures in the risk window come from the same source as the browser
    // session, which is what the account-wide risk counter records.
    const gateway = await request.newContext({
      baseURL: gatewayControlOrigin,
      extraHTTPHeaders: { ...gatewayHostHeaders(apiOrigin), Origin: adminOrigin },
    })
    try {
      for (let attempt = 0; attempt < 10; attempt += 1) {
        const identify = await gateway.post('/api/admin/v1/auth/identify', {
          data: { email: administrator.email },
        })
        expect(identify.status()).toBe(200)
        const flowToken = String((await identify.json() as { flowToken?: string }).flowToken ?? '')
        const failed = await gateway.post('/api/admin/v1/auth/attempt', {
          data: { flowToken, method: 'password', password: 'wrong-password-1234' },
        })
        expect(failed.status()).toBe(401)
      }
    } finally {
      await gateway.dispose()
    }

    const context = await browser.newContext()
    const page = await context.newPage()
    try {
      await page.goto(absolute(adminOrigin, '/login'))
      await page.getByLabel('工作邮箱').fill(administrator.email)
      await page.getByRole('button', { name: '下一步' }).click()
      await page.getByLabel('登录密码').fill(administrator.password)
      await page.getByRole('button', { name: '使用密码登录' }).click()
      await expect(page.getByLabel('风控短信验证码')).toBeVisible()
      const code = await waitForSmsCode(api, adminPhone)
      await page.getByLabel('风控短信验证码').fill(code)
      await page.getByRole('button', { name: '继续' }).click()
      const secret = (await readFile(adminTotpSecretPath, 'utf8')).trim()
      await page.getByLabel('验证器 6 位验证码').fill(totp(secret))
      await page.getByRole('button', { name: '继续' }).click()
      await expect(page).toHaveURL(absolute(adminOrigin, '/'))
    } finally {
      await context.close()
    }
  })
})
