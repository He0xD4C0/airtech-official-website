import { randomUUID } from 'node:crypto'
import { mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { request, type FullConfig } from '@playwright/test'
import {
  administrator,
  apiControlHeaders,
  apiControlOrigin,
  adminOrigin,
  adminStorageStatePath,
  apiOrigin,
  browserCookiesForLocalGateway,
  isolatedStack,
  publicOrigin,
  runAdminWorkflows,
} from './support/environment'
import { totp } from './support/totp'
import { cookieRequestHeader, mergeCookies, secureHostOnlyCookies } from './support/secure-cookie'

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

async function createAndPublishContent(
  api: ApiContext,
  csrf: string,
  payload: Record<string, unknown>,
): Promise<void> {
  const listed = await api.get('/api/admin/v1/content?limit=100')
  if (!listed.ok()) throw new Error(`Unable to inspect content fixtures (${listed.status()}): ${await listed.text()}`)
  const listing = await listed.json() as { items: Array<Record<string, unknown>> }
  let entry = listing.items.find((item) => item.kind === payload.kind && item.slug === payload.slug && item.locale === 'en')
  if (!entry) {
    const created = await api.post('/api/admin/v1/content', {
      headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
      data: payload,
    })
    entry = await expectJson(created, `Unable to create ${String(payload.kind)} fixture`)
  }
  if (entry.status === 'published' && entry.publishedRevision === entry.currentRevision) return
  const id = String(entry.id)
  const revision = Number(entry.currentRevision)
  const published = await api.post(`/api/admin/v1/content/${id}/publish`, {
    headers: {
      'X-CSRF-Token': csrf,
      'Idempotency-Key': randomUUID(),
      'If-Match': `"revision-${revision}"`,
    },
  })
  await expectJson(published, `Unable to publish ${String(payload.kind)} fixture`)
}

async function seedPublicProjection(api: ApiContext, csrf: string): Promise<void> {
  const placeholderParagraph = {
    type: 'paragraph',
    content: [{
      type: 'text',
      text: 'This development fixture verifies database-backed rendering. Replace it with reviewed editorial content before launch.',
    }],
  }
  const content = (kind: string, slug: string, title: string, canonicalPath: string | null, pageSlots: Record<string, unknown>) => ({
    kind,
    slug,
    locale: 'en',
    title,
    summary: 'Database-backed public website development preview.',
    body: {
      schemaVersion: 1,
      doc: { type: 'doc', attrs: { pageSlots }, content: [placeholderParagraph] },
    },
    seo: {
      title: `${title} | Development Preview`,
      description: 'A database-backed development preview for the AIRTEKPOWER public website.',
      canonicalPath,
      indexable: false,
    },
    isPlaceholder: true,
  })

  await createAndPublishContent(api, csrf, content('home', 'index', 'AIRTEKPOWER Development Preview', '/en', {
    templateKey: 'home',
    eyebrow: 'Development fixture',
    hero: {
      eyebrow: 'Development fixture',
      title: 'AIRTEKPOWER Development Preview',
      description: 'This placeholder verifies the public SSR content pipeline without publishing unreviewed product or business claims.',
    },
    primaryCta: {
      eyebrow: 'Development fixture',
      title: 'Prepare a structured inquiry',
      description: 'Use the RFQ flow to test the development experience.',
      label: 'Request a Quote',
      href: '/en/request-a-quote',
    },
    sections: [{
      id: 'database-backed',
      eyebrow: 'Development fixture',
      title: 'Database-backed content',
      description: 'This section is loaded from PostgreSQL and is intentionally non-indexable.',
    }],
  }))
  await createAndPublishContent(api, csrf, content('navigation', 'primary', 'Primary navigation', null, {
    items: [
      { label: 'Home', href: '/en' },
      { label: 'Products', href: '/en/products' },
      { label: 'Solutions', href: '/en/solutions' },
      { label: 'Technology', href: '/en/technology' },
      { label: 'Resources', href: '/en/resources/articles' },
      { label: 'Company', href: '/en/company/about' },
      { label: 'News', href: '/en/resources/news' },
    ],
  }))
  await createAndPublishContent(api, csrf, content('footer', 'primary', 'Primary footer', null, {
    columns: [
      { title: 'Explore', links: [{ label: 'Products', href: '/en/products' }, { label: 'Solutions', href: '/en/solutions' }] },
      { title: 'Resources', links: [{ label: 'Articles', href: '/en/resources/articles' }, { label: 'News', href: '/en/resources/news' }] },
      { title: 'Company', links: [{ label: 'About', href: '/en/company/about' }, { label: 'Contact', href: '/en/company/contact' }] },
    ],
    legalLinks: [{ label: 'Privacy', href: '/en/privacy' }, { label: 'Terms', href: '/en/terms' }],
  }))

  const existingInformation = await api.get('/api/admin/v1/general-information?locale=en')
  let informationEntry: Record<string, unknown>
  if (existingInformation.status() === 404) {
    const information = await api.post('/api/admin/v1/general-information', {
      headers: { 'X-CSRF-Token': csrf, 'Idempotency-Key': randomUUID() },
      data: {
      locale: 'en',
      isPlaceholder: true,
      payload: {
        brandName: 'AIRTEKPOWER',
        brandLine: 'Redefining Airflow with Smart, Green Technology',
        homePath: '/en',
        footerStatement: 'Development fixture content. Replace all placeholders with reviewed, source-backed publication data before launch.',
        copyrightText: '© {year} AIRTEKPOWER. Development fixture.',
        defaultSeo: {
          title: 'AIRTEKPOWER | Development Preview',
          description: 'A database-backed development preview for the AIRTEKPOWER public website.',
        },
        organization: { name: 'AIRTEKPOWER' },
        navigationCta: { label: 'Request a Quote', href: '/en/request-a-quote' },
      },
      },
    })
    informationEntry = await expectJson(information, 'Unable to create General Information fixture')
  } else {
    if (!existingInformation.ok()) {
      throw new Error(`Unable to inspect General Information fixture (${existingInformation.status()}): ${await existingInformation.text()}`)
    }
    informationEntry = await existingInformation.json() as Record<string, unknown>
  }
  if (informationEntry.status === 'published' && informationEntry.publishedRevision === informationEntry.currentRevision) return
  const published = await api.post(`/api/admin/v1/general-information/${String(informationEntry.id)}/publish`, {
    headers: {
      'X-CSRF-Token': csrf,
      'Idempotency-Key': randomUUID(),
      'If-Match': `"revision-${Number(informationEntry.currentRevision)}"`,
    },
  })
  await expectJson(published, 'Unable to publish General Information fixture')
}

export default async function globalSetup(_config: FullConfig): Promise<void> {
  if (!runAdminWorkflows) return
  if (!isolatedStack) {
    throw new Error('Admin workflow E2E mutates durable records and therefore requires E2E_ISOLATED_STACK=true.')
  }

  await mkdir(path.dirname(adminStorageStatePath), { recursive: true })
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
    await seedPublicProjection(api, refreshedCsrf)
    // The production image must keep Secure cookies. The disposable gateway is
    // intentionally plain HTTP, so only the browser fixture copy relaxes the
    // transport bit; domain, path, SameSite and HttpOnly remain unchanged.
    const storageState = { cookies: productionCookies, origins: [] }
    const browserStorageState = apiOrigin.startsWith('http://')
      ? { ...storageState, cookies: browserCookiesForLocalGateway(storageState.cookies) }
      : storageState
    await writeFile(adminStorageStatePath, `${JSON.stringify(browserStorageState, null, 2)}\n`, 'utf8')
  } finally {
    await api?.dispose()
    await setupApi.dispose()
  }

  await seedAnalyticsProjection()
}
