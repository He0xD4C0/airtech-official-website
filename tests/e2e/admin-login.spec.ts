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
      await page.getByRole('textbox', { name: '密码 显示密码' }).fill(administrator.password)
      const secret = (await readFile(adminTotpSecretPath, 'utf8')).trim()
      await page.getByLabel(/TOTP 或恢复码/u).fill(totp(secret))
      await page.getByRole('button', { name: '进入管理平台' }).click()

      await expect(page).toHaveURL(absolute(adminOrigin, '/'))
      await expect(page.getByRole('heading', { name: /开始今天的发布工作/u })).toBeVisible()
      await expect(page.getByRole('region', { name: '关键指标' })).toBeVisible()
      const cookies = await context.cookies()
      expect(cookies.filter((cookie) => cookie.httpOnly).length).toBeGreaterThan(0)
      expect(cookies.filter((cookie) => cookie.httpOnly).every((cookie) => cookie.secure && cookie.sameSite === 'Strict')).toBe(true)
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
