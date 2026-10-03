import { readFile } from 'node:fs/promises'
import { expect, request, test, type Page } from '@playwright/test'
import {
  absolute,
  administrator,
  adminOrigin,
  adminTotpSecretPath,
  runAdminWorkflows,
} from './support/environment'
import { adminPhone, waitForMailCode, waitForSmsCode } from './support/delivery'
import { totp } from './support/totp'

test.describe.configure({ mode: 'serial' })

test.describe('Admin sign-in channels', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack so the disposable administrator exists.')

  async function openSecondStage(page: Page): Promise<void> {
    await page.goto(absolute(adminOrigin, '/login'))
    await page.getByLabel('工作邮箱').fill(administrator.email)
    await page.getByRole('button', { name: '下一步' }).click()
    await expect(page.getByRole('button', { name: '使用密码登录' })).toBeVisible()
  }

  async function finishTotp(page: Page): Promise<void> {
    const secret = (await readFile(adminTotpSecretPath, 'utf8')).trim()
    await page.getByLabel('验证器 6 位验证码').fill(totp(secret))
    await page.getByRole('button', { name: '继续' }).click()
    await expect(page).toHaveURL(absolute(adminOrigin, '/'))
    await expect(page.getByRole('region', { name: '关键指标' })).toBeVisible()
  }

  test('signs in with an email verification code', async ({ browser, request: api }) => {
    const context = await browser.newContext()
    const page = await context.newPage()
    try {
      await openSecondStage(page)
      await page.getByRole('button', { name: '发送邮箱验证码' }).click()
      await expect(page.getByLabel('邮箱验证码')).toBeVisible()
      const code = await waitForMailCode(api, administrator.email)
      await page.getByLabel('邮箱验证码').fill(code)
      await page.getByRole('button', { name: '继续' }).click()
      await finishTotp(page)
    } finally {
      await context.close()
    }
  })

  test('signs in with an SMS verification code for the bound phone', async ({ browser, request: api }) => {
    const context = await browser.newContext()
    const page = await context.newPage()
    try {
      await openSecondStage(page)
      await page.getByRole('button', { name: '发送短信验证码' }).click()
      await expect(page.getByLabel('短信验证码')).toBeVisible()
      const code = await waitForSmsCode(api, adminPhone)
      await page.getByLabel('短信验证码').fill(code)
      await page.getByRole('button', { name: '继续' }).click()
      await finishTotp(page)
    } finally {
      await context.close()
    }
  })

})
