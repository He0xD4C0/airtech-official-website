// Runs last: configuring the CAPTCHA singleton cannot be undone through the
// Admin API, so every earlier sign-in journey stays CAPTCHA-free.
import { readFile } from 'node:fs/promises'
import { expect, request, test, type APIRequestContext } from '@playwright/test'
import {
  administrator,
  apiControlHeaders,
  apiControlOrigin,
  adminTotpSecretPath,
  runAdminWorkflows,
} from './support/environment'
import { configureCaptchaSettings } from './support/delivery'
import { totp } from './support/totp'

test.describe.configure({ mode: 'serial' })

async function identifyFlow(api: APIRequestContext): Promise<string> {
  const identified = await api.post('/api/admin/v1/auth/identify', {
    data: { email: administrator.email },
  })
  expect(identified.status()).toBe(200)
  const body = await identified.json() as { flowToken?: string; captchaRequired?: boolean }
  expect(body.captchaRequired).toBe(true)
  return String(body.flowToken ?? '')
}

test.describe('Admin CAPTCHA enforcement', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack so the disposable administrator exists.')

  test('enforces the configured provider and keeps password fallback on outage', async ({ request: api }) => {
    const secret = (await readFile(adminTotpSecretPath, 'utf8')).trim()
    const adminApi = await request.newContext({
      baseURL: apiControlOrigin,
      extraHTTPHeaders: apiControlHeaders(),
    })
    try {
      const login = await adminApi.post('/api/admin/v1/auth/login', {
        data: { email: administrator.email, password: administrator.password, otp: totp(secret) },
      })
      expect(login.status()).toBe(200)
      const session = await adminApi.get('/api/admin/v1/auth/session')
      const csrf = session.headers()['x-csrf-token']
      if (!csrf) throw new Error('The administrator session did not return a CSRF token.')
      await configureCaptchaSettings(adminApi, csrf)
    } finally {
      await adminApi.dispose()
    }

    const loginApi = await request.newContext({
      baseURL: apiControlOrigin,
      extraHTTPHeaders: apiControlHeaders(),
    })
    try {
      const missingToken = await loginApi.post('/api/admin/v1/auth/attempt', {
        data: { flowToken: await identifyFlow(loginApi), method: 'password', password: administrator.password },
      })
      expect(missingToken.status()).toBe(403)

      const rejected = await loginApi.post('/api/admin/v1/auth/attempt', {
        data: {
          flowToken: await identifyFlow(loginApi),
          method: 'password',
          password: administrator.password,
          captchaToken: 'reject',
        },
      })
      expect(rejected.status()).toBe(403)

      const accepted = await loginApi.post('/api/admin/v1/auth/attempt', {
        data: {
          flowToken: await identifyFlow(loginApi),
          method: 'password',
          password: administrator.password,
          captchaToken: 'pass',
        },
      })
      expect(accepted.status()).toBe(202)
      expect((await accepted.json() as { factor?: string }).factor).toBe('totp')

      // The stub answers /siteverify with a non-JSON gateway error, so the API
      // treats the provider as unreachable and applies fail-closed.
      const blockedCode = await loginApi.post('/api/admin/v1/auth/attempt', {
        data: {
          flowToken: await identifyFlow(loginApi),
          method: 'emailCode',
          captchaToken: 'offline',
        },
      })
      expect(blockedCode.status()).toBe(403)
      expect(await blockedCode.text()).toContain('password')

      const allowedPassword = await loginApi.post('/api/admin/v1/auth/attempt', {
        data: {
          flowToken: await identifyFlow(loginApi),
          method: 'password',
          password: administrator.password,
          captchaToken: 'offline',
        },
      })
      expect(allowedPassword.status()).toBe(202)
    } finally {
      await loginApi.dispose()
    }
  })
})
