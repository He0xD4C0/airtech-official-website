import { randomUUID } from 'node:crypto'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { request, type FullConfig } from '@playwright/test'
import {
  administrator,
  apiControlHeaders,
  apiControlOrigin,
  adminOrigin,
  adminSecondaryStorageStatePath,
  adminStorageStatePath,
  adminTotpSecretPath,
  apiOrigin,
  browserCookiesForLocalGateway,
  isolatedStack,
  publicOrigin,
  runAdminWorkflows,
} from './support/environment'
import { totp } from './support/totp'
import { cookieRequestHeader, mergeCookies, secureHostOnlyCookies } from './support/secure-cookie'
import { upsertAndPublish } from './support/content-fixtures'

async function seedAnalyticsProjection(): Promise<void> {
  const publicApi = await request.newContext({
    baseURL: apiControlOrigin,
    extraHTTPHeaders: {
      Origin: publicOrigin,
      ...(new URL(apiControlOrigin).host === new URL(apiOrigin).host
        ? {}
        : { Host: new URL(apiOrigin).host }),
    },
  })
  const landingPath = '/en/e2e-admin-analytics'
  const pageViewsPerVisit = [2, 2, 2, 2, 1, 1, 1]
  try {
    for (const [visitIndex, pageViewCount] of pageViewsPerVisit.entries()) {
      const anonymousSessionId = randomUUID()
      const consent = await publicApi.post('/api/public/v1/analytics/consents', {
        data: {
          anonymousSessionId,
          policyVersion: 'analytics-v1',
          analyticsAllowed: true,
        },
      })
      const consentBody = await expectJson(consent, 'Unable to create analytics consent fixture')
      const consentReceipt = String(consentBody.consentReceipt)
      const visit = await publicApi.post('/api/public/v1/guest-visits', {
        data: {
          anonymousSessionId,
          consentReceipt,
          policyVersion: 'analytics-v1',
          landingPath,
          referrerDomain: 'e2e.example.test',
          source: 'e2e-source',
          medium: 'integration-test',
          campaign: 'admin-acceptance',
        },
      })
      await expectJson(visit, 'Unable to create guest visit fixture')

      const eventNames = [
        ...Array.from({ length: pageViewCount }, () => 'pageView'),
        ...(visitIndex < 3 ? ['rfqStarted'] : []),
        ...(visitIndex < 2 ? ['rfqSubmitted'] : []),
      ]
      for (const eventName of eventNames) {
        const event = await publicApi.post('/api/public/v1/analytics/events', {
          headers: { 'Idempotency-Key': randomUUID() },
          data: {
            eventName,
            anonymousSessionId,
            sourcePath: landingPath,
            locale: 'en',
            consentGranted: true,
            policyVersion: 'analytics-v1',
            consentReceipt,
            properties: {},
          },
        })
        await expectJson(event, `Unable to create ${eventName} analytics fixture`)
      }
    }
  } finally {
    await publicApi.dispose()
  }
}

type ApiContext = Awaited<ReturnType<typeof request.newContext>>

async function expectJson(response: Awaited<ReturnType<ApiContext['post']>>, action: string): Promise<Record<string, unknown>> {
  if (!response.ok()) throw new Error(`${action} (${response.status()}): ${await response.text()}`)
  return await response.json() as Record<string, unknown>
}

async function seedPublicProjection(api: ApiContext, csrf: string): Promise<void> {
  const fixturePath = path.resolve('services/platform/fixtures/development-public-site.json')
  const fixtures = JSON.parse(await readFile(fixturePath, 'utf8')) as Array<Record<string, unknown>>
  for (const fixture of fixtures) await upsertAndPublish(api, csrf, fixture)
}

export default async function globalSetup(_config: FullConfig): Promise<void> {
  if (!runAdminWorkflows) return
  if (!isolatedStack) {
    throw new Error('Admin workflow E2E mutates durable records and therefore requires E2E_ISOLATED_STACK=true.')
  }

  await mkdir(path.dirname(adminStorageStatePath), { recursive: true })
  await mkdir(path.dirname(adminSecondaryStorageStatePath), { recursive: true })
  const setupApi = await request.newContext({
    baseURL: apiControlOrigin,
    extraHTTPHeaders: apiControlHeaders(),
  })
  let api: ApiContext | undefined
  try {
    const setup = await setupApi.post('/api/admin/v1/auth/setup', {
      data: {
        displayName: administrator.displayName,
        email: administrator.email,
        password: administrator.password,
        bootstrapToken: administrator.bootstrapToken,
      },
    })
    if (setup.status() !== 201) {
      throw new Error(`Unable to create the isolated E2E administrator (${setup.status()}): ${await setup.text()}`)
    }
    const csrf = setup.headers()['x-csrf-token']
    if (!csrf) throw new Error('Initial setup did not return the CSRF token required for TOTP enrollment.')
    const cookieHostname = new URL(apiOrigin).hostname
    let productionCookies = secureHostOnlyCookies(setup, cookieHostname)
    api = await request.newContext({
      baseURL: apiControlOrigin,
      extraHTTPHeaders: {
        ...apiControlHeaders(),
        Cookie: cookieRequestHeader(productionCookies),
      },
    })
    const enrollment = await api.post('/api/admin/v1/auth/totp/enrollment', {
      headers: { 'X-CSRF-Token': csrf },
    })
    if (!enrollment.ok()) {
      throw new Error(`Unable to start E2E TOTP enrollment (${enrollment.status()}): ${await enrollment.text()}`)
    }
    const { secret } = await enrollment.json() as { secret: string }
    await writeFile(adminTotpSecretPath, `${secret}\n`, { encoding: 'utf8', mode: 0o600 })
    const confirmation = await api.post('/api/admin/v1/auth/totp/confirm', {
      headers: { 'X-CSRF-Token': csrf },
      data: { code: totp(secret) },
    })
    if (!confirmation.ok()) {
      throw new Error(`Unable to confirm E2E TOTP enrollment (${confirmation.status()}): ${await confirmation.text()}`)
    }
    const refreshed = await api.get('/api/admin/v1/auth/session')
    if (!refreshed.ok()) {
      throw new Error(`Unable to refresh the TOTP-enabled E2E session (${refreshed.status()}): ${await refreshed.text()}`)
    }
    productionCookies = mergeCookies(productionCookies, secureHostOnlyCookies(refreshed, cookieHostname))
    const refreshedCsrf = refreshed.headers()['x-csrf-token'] ?? csrf
    const objectStorage = await api.put('/api/admin/v1/settings/object-storage', {
      headers: {
        'X-CSRF-Token': refreshedCsrf,
        'If-Match': '"revision-0"',
      },
      data: {
        endpoint: 'http://minio:9000',
        region: 'us-east-1',
        bucket: 'airtek-media',
        accessKeyId: 'airtek-media-api',
        secretAccessKey: 'local-api-media-only',
        keyPrefix: 'media',
        pathStyle: true,
        publicBaseUrl: process.env.E2E_MEDIA_PUBLIC_BASE_URL
          ?? 'http://media.localhost:19000/airtek-media',
        adoptLegacyAssets: true,
        reason: 'Configure isolated E2E object storage through the Admin API.',
      },
    })
    if (!objectStorage.ok()) {
      throw new Error(`Unable to configure E2E object storage (${objectStorage.status()}): ${await objectStorage.text()}`)
    }
    await seedPublicProjection(api, refreshedCsrf)
    // The production image must keep Secure cookies. The disposable gateway is
    // intentionally plain HTTP, so only the browser fixture copy relaxes the
    // transport bit; domain, path, SameSite and HttpOnly remain unchanged.
    const storageState = { cookies: productionCookies, origins: [] }
    const browserStorageState = apiOrigin.startsWith('http://')
      ? { ...storageState, cookies: browserCookiesForLocalGateway(storageState.cookies) }
      : storageState
    await writeFile(adminStorageStatePath, `${JSON.stringify(browserStorageState, null, 2)}\n`, 'utf8')

    // The concurrency acceptance test needs two distinct server-side sessions.
    // Reusing one cookie across browser contexts makes their CSRF rotations
    // invalidate each other before the stale ETag can reach the 409 boundary.
    const secondaryApi = await request.newContext({
      baseURL: apiControlOrigin,
      extraHTTPHeaders: apiControlHeaders(),
    })
    try {
      const secondaryLogin = await secondaryApi.post('/api/admin/v1/auth/login', {
        data: {
          email: administrator.email,
          password: administrator.password,
          otp: totp(secret),
        },
      })
      if (!secondaryLogin.ok()) {
        throw new Error(`Unable to create the second E2E admin session (${secondaryLogin.status()}): ${await secondaryLogin.text()}`)
      }
      const secondaryCookies = secureHostOnlyCookies(secondaryLogin, cookieHostname)
      const secondaryStorageState = { cookies: secondaryCookies, origins: [] }
      const secondaryBrowserStorageState = apiOrigin.startsWith('http://')
        ? { ...secondaryStorageState, cookies: browserCookiesForLocalGateway(secondaryStorageState.cookies) }
        : secondaryStorageState
      await writeFile(
        adminSecondaryStorageStatePath,
        `${JSON.stringify(secondaryBrowserStorageState, null, 2)}\n`,
        'utf8',
      )
    } finally {
      await secondaryApi.dispose()
    }
  } finally {
    await api?.dispose()
    await setupApi.dispose()
  }

  await seedAnalyticsProjection()
}
