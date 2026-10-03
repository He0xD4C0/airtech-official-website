// Runs last: the account-wide failure counter it creates would otherwise make
// every later password sign-in demand the risk SMS challenge.
//
// Super Admins are exempt from the risk challenge by design, so this journey
// provisions a separate invited operator with a verified phone.
import { randomUUID } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { expect, request, test, type APIRequestContext } from '@playwright/test'
import {
  absolute,
  administrator,
  adminOrigin,
  adminTotpSecretPath,
  apiControlHeaders,
  apiControlOrigin,
  apiOrigin,
  gatewayControlOrigin,
  gatewayHostHeaders,
  runAdminWorkflows,
} from './support/environment'
import { waitForSmsCode } from './support/delivery'
import { totp } from './support/totp'

const riskUser = {
  displayName: 'AIRTEK E2E Risk Operator',
  email: process.env.E2E_RISK_EMAIL ?? 'e2e-risk@airtek.invalid',
  password: process.env.E2E_RISK_PASSWORD ?? 'Airtek-E2E-Risk-123!',
  phone: process.env.E2E_RISK_PHONE ?? '+8613800138001',
}

async function csrfToken(context: APIRequestContext, path: string): Promise<string> {
  const response = await context.get(path)
  expect(response.ok()).toBe(true)
  const token = response.headers()['x-csrf-token']
  if (!token) throw new Error(`${path} did not return a CSRF token.`)
  return token
}

test.describe('Admin sign-in risk challenge', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack so the disposable administrator exists.')

  test('requires an SMS challenge after repeated password failures', async ({ browser, request: api }) => {
    const secret = (await readFile(adminTotpSecretPath, 'utf8')).trim()
    const adminApi = await request.newContext({
      baseURL: apiControlOrigin,
      extraHTTPHeaders: apiControlHeaders(),
    })
    let operatorToken: string
    try {
      const adminLogin = await adminApi.post('/api/admin/v1/auth/login', {
        data: { email: administrator.email, password: administrator.password, otp: totp(secret) },
      })
      expect(adminLogin.status()).toBe(200)
      const adminCsrf = adminLogin.headers()['x-csrf-token']
      if (!adminCsrf) throw new Error('The administrator login did not return a CSRF token.')
      const invitation = await adminApi.post('/api/admin/v1/user-invitations', {
        headers: { 'X-CSRF-Token': adminCsrf, 'Idempotency-Key': randomUUID() },
        data: {
          email: riskUser.email,
          displayName: riskUser.displayName,
          roleKeys: ['content-editor'],
        },
      })
      expect(invitation.status()).toBe(201)
      operatorToken = String((await invitation.json() as { invitationToken?: string }).invitationToken ?? '')
      if (!operatorToken) throw new Error('The invitation did not return a one-time token.')
    } finally {
      await adminApi.dispose()
    }

    const operatorApi = await request.newContext({
      baseURL: apiControlOrigin,
      extraHTTPHeaders: apiControlHeaders(),
    })
    try {
      const accepted = await operatorApi.post('/api/admin/v1/auth/invitations/accept', {
        data: { token: operatorToken, password: riskUser.password },
      })
      expect(accepted.status()).toBe(201)
      const login = await operatorApi.post('/api/admin/v1/auth/login', {
        data: { email: riskUser.email, password: riskUser.password },
      })
      expect(login.status()).toBe(200)
      const csrf = await csrfToken(operatorApi, '/api/admin/v1/auth/session')
      const start = await operatorApi.post('/api/admin/v1/auth/phone/verification', {
        headers: { 'X-CSRF-Token': csrf },
        data: { phone: riskUser.phone, currentPassword: riskUser.password },
      })
      expect(start.status()).toBe(202)
      const confirm = await operatorApi.post('/api/admin/v1/auth/phone/confirm', {
        headers: { 'X-CSRF-Token': csrf },
        data: { code: await waitForSmsCode(api, riskUser.phone) },
      })
      expect(confirm.status()).toBe(204)
    } finally {
      await operatorApi.dispose()
    }

    // Ten failures in the risk window come from the same source as the browser
    // session, which is what the account-wide risk counter records.
    const gateway = await request.newContext({
      baseURL: gatewayControlOrigin,
      extraHTTPHeaders: { ...gatewayHostHeaders(apiOrigin), Origin: adminOrigin },
    })
    try {
      for (let attempt = 0; attempt < 10; attempt += 1) {
        const identify = await gateway.post('/api/admin/v1/auth/identify', {
          data: { email: riskUser.email },
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
      await page.getByLabel('工作邮箱').fill(riskUser.email)
      await page.getByRole('button', { name: '下一步' }).click()
      await page.getByLabel('登录密码').fill(riskUser.password)
      await page.getByRole('button', { name: '使用密码登录' }).click()
      await expect(page.getByLabel('风控短信验证码')).toBeVisible()
      const code = await waitForSmsCode(api, riskUser.phone)
      await page.getByLabel('风控短信验证码').fill(code)
      await page.getByRole('button', { name: '继续' }).click()
      await expect(page).toHaveURL(absolute(adminOrigin, '/'))
    } finally {
      await context.close()
    }
  })
})
