import { randomUUID } from 'node:crypto'
import { mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { request, type FullConfig } from '@playwright/test'
import {
  administrator,
  apiControlHeaders,
  apiControlOrigin,
  adminOrigin,
  adminSecondaryStorageStatePath,
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

function record(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
}

function legacyTarget(href: unknown): Record<string, unknown> {
  const value = typeof href === 'string' ? href : ''
  return value.startsWith('/')
    ? { targetType: 'route', path: value }
    : { targetType: 'external', url: value }
}

function legacyLinks(value: unknown): Array<Record<string, unknown>> {
  if (!Array.isArray(value)) return []
  return value.flatMap((entry) => {
    const link = record(entry)
    const label = typeof link.label === 'string' ? link.label : ''
    if (!label) return []
    return [{
      id: randomUUID(),
      label,
      target: legacyTarget(link.href),
      children: legacyLinks(link.children),
    }]
  })
}

function placeholderBody(): Record<string, unknown> {
  return {
    type: 'doc',
    content: [{
      type: 'paragraph',
      content: [{
        type: 'text',
        text: 'This development fixture verifies database-backed rendering. Replace it with reviewed editorial content before launch.',
      }],
    }],
  }
}

/** Converts the small legacy-shaped seed helper input into the canonical V2 draft. */
function v2DraftFromLegacy(payload: Record<string, unknown>): Record<string, unknown> {
  const kind = String(payload.kind)
  const slots = record(record(record(payload.body).doc).attrs).pageSlots
  const slotValues = record(slots)
  const seo = record(payload.seo)
  const draft: Record<string, unknown> = {
    schemaVersion: 2,
    kind,
    locale: 'en',
    templateKey: String(slotValues.templateKey ?? kind),
    title: String(payload.title ?? ''),
    summary: payload.summary ?? null,
    isPlaceholder: payload.isPlaceholder ?? true,
    body: null,
    composition: { blocks: [] },
    seo: {
      title: seo.title ?? null,
      description: seo.description ?? null,
      indexable: false,
      socialImage: null,
    },
    relations: [],
    draftVersion: 1,
  }
  if (kind === 'home') {
    const hero = record(slotValues.hero)
    const cta = record(slotValues.primaryCta)
    draft.slug = payload.slug ?? 'index'
    draft.typeFields = { type: 'home' }
    draft.body = placeholderBody()
    draft.composition = {
      blocks: [
        {
          type: 'hero',
          id: randomUUID(),
          eyebrow: hero.eyebrow ?? null,
          heading: hero.title ?? draft.title,
          lead: hero.description ?? null,
          media: null,
          actions: typeof cta.label === 'string' && cta.label
            ? [{ label: cta.label, target: legacyTarget(cta.href) }]
            : [],
          variant: 'standard',
        },
        { type: 'body', id: randomUUID(), width: 'standard' },
      ],
    }
  } else if (kind === 'navigation') {
    draft.slug = null
    draft.typeFields = { type: 'navigation', items: legacyLinks(slotValues.items) }
  } else if (kind === 'footer') {
    draft.slug = null
    draft.typeFields = {
      type: 'footer',
      columns: Array.isArray(slotValues.columns)
        ? slotValues.columns.map((column) => {
            const value = record(column)
            return { id: randomUUID(), title: String(value.title ?? ''), links: legacyLinks(value.links) }
          })
        : [],
      legalLinks: legacyLinks(slotValues.legalLinks),
    }
  } else {
    throw new Error(`Unsupported legacy fixture kind: ${kind}`)
  }
  return draft
}

async function upsertAndPublish(
  api: ApiContext,
  csrf: string,
  draft: Record<string, unknown>,
): Promise<void> {
  const listed = await api.get('/api/admin/v1/content?limit=100')
  if (!listed.ok()) throw new Error(`Unable to inspect content fixtures (${listed.status()}): ${await listed.text()}`)
  const listing = await listed.json() as { items: Array<Record<string, unknown>> }
  let entry = listing.items.find((item) => {
    const document = record(item.draft)
    return document.kind === draft.kind
      && (document.slug ?? null) === (draft.slug ?? null)
      && document.locale === 'en'
  })
  if (!entry) {
    const created = await api.post('/api/admin/v1/content', {
      headers: {
        'X-CSRF-Token': csrf,
        'Idempotency-Key': randomUUID(),
        'If-Match': '"draft-0"',
      },
      data: draft,
    })
    entry = await expectJson(created, `Unable to create ${String(draft.kind)} fixture`)
  }
  if (entry.status === 'published' && entry.publishedRevision != null) return
  const document = record(entry.draft)
  const published = await api.post(`/api/admin/v1/content/${String(entry.id)}/snapshots`, {
    headers: {
      'X-CSRF-Token': csrf,
      'Idempotency-Key': randomUUID(),
      'If-Match': `"draft-${Number(document.draftVersion)}"`,
    },
    data: { intent: 'publish', reason: 'Seed development fixture for the isolated E2E stack' },
  })
  await expectJson(published, `Unable to publish ${String(draft.kind)} fixture`)
}

async function createAndPublishContent(
  api: ApiContext,
  csrf: string,
  payload: Record<string, unknown>,
): Promise<void> {
  await upsertAndPublish(api, csrf, v2DraftFromLegacy(payload))
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
  await upsertAndPublish(api, csrf, {
    schemaVersion: 2,
    kind: 'page',
    locale: 'en',
    templateKey: 'selector',
    title: 'Fan Selector Development Preview',
    slug: 'fan-selector',
    summary: 'Select only from validated published product records.',
    isPlaceholder: true,
    typeFields: { type: 'page' },
    body: null,
    composition: {
      blocks: [{
        type: 'hero',
        id: randomUUID(),
        eyebrow: 'Product discovery',
        heading: 'Fan Selector Development Preview',
        lead: 'No product candidate is inferred when validated published data is unavailable.',
        media: null,
        actions: [],
        variant: 'standard',
      }],
    },
    seo: {
      title: 'Fan Selector | Development Preview',
      description: 'Development fixture for the published-data selector workflow.',
      indexable: false,
      socialImage: null,
    },
    relations: [],
    draftVersion: 1,
  })
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

  await upsertAndPublish(api, csrf, {
    schemaVersion: 2,
    kind: 'generalInformation',
    locale: 'en',
    templateKey: 'generalInformation',
    title: 'General Information',
    slug: null,
    summary: null,
    isPlaceholder: true,
    typeFields: {
      type: 'generalInformation',
      organizationName: 'AIRTEKPOWER',
      brandLine: 'Redefining Airflow with Smart, Green Technology',
      homePath: '/en',
      footerStatement: 'Development fixture content. Replace all placeholders with reviewed, source-backed publication data before launch.',
      copyrightTemplate: '© {year} AIRTEKPOWER. Development fixture.',
      contact: {
        email: null,
        phone: null,
        addressLines: [],
        locality: null,
        region: null,
        postalCode: null,
        countryCode: null,
      },
      socialLinks: [],
      defaultSeo: {
        title: 'AIRTEKPOWER | Development Preview',
        description: 'A database-backed development preview for the AIRTEKPOWER public website.',
        indexable: false,
        socialImage: null,
      },
      productCategories: [],
      navigationCta: {
        label: 'Request a Quote',
        target: { targetType: 'route', path: '/en/request-a-quote' },
      },
    },
    body: null,
    composition: { blocks: [] },
    seo: { title: null, description: null, indexable: false, socialImage: null },
    relations: [],
    draftVersion: 1,
  })
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
