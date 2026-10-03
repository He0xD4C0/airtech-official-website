import { readFile } from 'node:fs/promises'
import { expect, test } from '@playwright/test'
import {
  absolute,
  administrator,
  adminOrigin,
  adminTotpSecretPath,
  runAdminWorkflows,
} from './support/environment'
import { totp } from './support/totp'

test.describe('Admin browser login', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack so the disposable administrator exists.')

  test('establishes a session through the form without injected cookies', async ({ browser }) => {
    const context = await browser.newContext()
    const page = await context.newPage()
    try {
      await page.goto(absolute(adminOrigin, '/login'))
      await page.getByLabel('工作邮箱').fill(administrator.email)
      await page.getByRole('button', { name: '下一步' }).click()

      // Second stage: choose the password method. CAPTCHA is not configured in
      // the isolated stack, so the flow intentionally skips the widget.
      await page.getByLabel('密码 显示密码').fill(administrator.password)
      await page.getByRole('button', { name: '使用密码登录' }).click()

      // The E2E administrator enrolls TOTP during global setup, so the flow
      // asks for the authenticator code as the final factor.
      const secret = (await readFile(adminTotpSecretPath, 'utf8')).trim()
      await page.getByLabel('验证器 6 位验证码').fill(totp(secret))
      await page.getByRole('button', { name: '继续' }).click()

      await expect(page).toHaveURL(absolute(adminOrigin, '/'))
      await expect(page.getByRole('heading', { name: /开始今天的发布工作/u })).toBeVisible()
      await expect(page.getByRole('region', { name: '关键指标' })).toBeVisible()
      const cookies = await context.cookies()
      expect(cookies.filter((cookie) => cookie.httpOnly).length).toBeGreaterThan(0)
      // The API scopes the Secure attribute to the configured Admin origin, so the
      // http E2E stack must receive browser-storable cookies while an https
      // deployment keeps them Secure.
      const requiresSecureCookie = adminOrigin.startsWith('https://')
      expect(
        cookies
          .filter((cookie) => cookie.httpOnly)
          .every((cookie) => cookie.secure === requiresSecureCookie && cookie.sameSite === 'Strict'),
      ).toBe(true)
      await page.reload()
      await expect(page).toHaveURL(absolute(adminOrigin, '/'))
      await expect(page.getByRole('region', { name: '关键指标' })).toBeVisible()
      await page.getByRole('button', { name: /退出登录/u }).click()
      await expect(page).toHaveURL(absolute(adminOrigin, '/login'))
      await page.reload()
      await expect(page).toHaveURL(absolute(adminOrigin, '/login'))
    } finally {
      await context.close()
    }
  })
})
