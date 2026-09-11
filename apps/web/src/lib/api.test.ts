import { describe, expect, it, vi } from 'vitest'
import { createPublicApiClient, PublicApiError } from './api'

function jsonResponse(value: unknown, status = 200) {
  return new Response(JSON.stringify(value), { status, headers: { 'Content-Type': 'application/json' } })
}

function projection(overrides: Record<string, unknown> = {}) {
  return {
    schemaVersion: 2,
    id: '792406a9-2f19-425e-8508-78a205c0c764',
    kind: 'page',
    templateKey: 'newsIndex',
    locale: 'en',
    title: 'Database News',
    summary: 'Published from PostgreSQL.',
    slug: null,
    body: null,
    composition: { blocks: [] },
    typeFields: { type: 'page' },
    seo: { title: 'Database News', description: 'Published from PostgreSQL.', indexable: true, socialImage: null },
    isPlaceholder: false,
    publishedRevision: 1,
    updatedAt: '2026-09-01T08:00:00Z',
    resolvedRelations: [],
    resolvedLinks: [],
    ...overrides,
  }
}

describe('public API client', () => {
  it('posts to the Rust endpoint and accepts the real acknowledgement shape', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({
      id: '77935cef-4111-4c4c-bdb8-17679a8b42fe', reference: 'CONTACT-20260901-77935CEF', acceptedAt: '2026-09-01T08:00:00Z',
    })) as unknown as typeof fetch
    const client = createPublicApiClient({ baseUrl: 'http://api:8080/api/public/v1/', fetchImpl })
    const payload = {
      contact: { name: 'Buyer', email: 'buyer@example.com' }, topic: 'general',
      message: 'A sufficiently detailed question.', sourcePath: '/en/company/contact', locale: 'en', consent: true,
    }
    const receipt = await client.submitContact(payload, 'contact-test-0001')

    expect(receipt.reference).toBe('CONTACT-20260901-77935CEF')
    const [url, init] = (fetchImpl as unknown as ReturnType<typeof vi.fn>).mock.calls[0]
    expect(url).toBe('http://api:8080/api/public/v1/contact')
    expect(init.method).toBe('POST')
    expect(init.credentials).toBe('omit')
    expect(JSON.parse(String(init.body))).toEqual(payload)
    expect(new Headers(init.headers).get('Idempotency-Key')).toBe('contact-test-0001')
  })

  it('rejects legacy or malformed receipt bodies instead of inventing a reference', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({ submissionId: 'legacy', receivedAt: 'now', status: 'received' })) as unknown as typeof fetch
    const client = createPublicApiClient({ baseUrl: 'http://api:8080/api/public/v1', fetchImpl })
    await expect(client.submitRfq({
      journey: 'selection', contact: { name: 'Buyer', email: 'buyer@example.com' },
      sourcePath: '/en/request-a-quote/selection', locale: 'en', consent: true,
      context: {
        application: 'Industrial cooling',
        dutyPoint: { airflow: 1200, airflowUnit: 'm3/h', pressure: 320, pressureUnit: 'Pa' },
      },
    }, 'rfq-test-0001')).rejects.toThrow(/invalid submission acknowledgement/i)
  })

  it('preserves the noValidatedCandidates selector outcome', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({
      outcome: 'noValidatedCandidates', candidates: [], explanations: ['No published validated record is available.'],
    })) as unknown as typeof fetch
    const result = await createPublicApiClient({ baseUrl: 'http://api:8080/api/public/v1', fetchImpl }).selectProducts({
      airflow: 1200, airflowUnit: 'm3/h', pressure: 320, pressureUnit: 'Pa', requiredCertifications: [],
    })
    expect(result).toEqual({
      outcome: 'noValidatedCandidates', candidates: [], explanations: ['No published validated record is available.'],
    })
  })

  it('posts consented first-party analytics without credentials', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({ accepted: true, eventId: 'c45a487f-848b-4144-b9f2-c00f1a7969d8' }, 202)) as unknown as typeof fetch
    const client = createPublicApiClient({ baseUrl: 'http://api:8080/api/public/v1', fetchImpl })
    const receipt = await client.submitAnalyticsEvent({
      eventName: 'pageView', anonymousSessionId: '059adab1-18af-434e-a93b-4e7d24d5744e',
      sourcePath: '/en', locale: 'en', consentGranted: true, policyVersion: 'analytics-v1',
      consentReceipt: 'f4abdf14-c27b-4d71-a22b-45cc3f7aa64d', properties: {},
    })
    expect(receipt.accepted).toBe(true)
    const [url, init] = (fetchImpl as unknown as ReturnType<typeof vi.fn>).mock.calls[0]
    expect(url).toBe('http://api:8080/api/public/v1/analytics/events')
    expect(init.credentials).toBe('omit')
  })

  it('records consent before analytics and accepts only a bound receipt shape', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({
      consentReceipt: 'f4abdf14-c27b-4d71-a22b-45cc3f7aa64d',
      anonymousSessionId: '059adab1-18af-434e-a93b-4e7d24d5744e',
      policyVersion: 'analytics-v1',
      analyticsAllowed: true,
      grantedAt: '2026-09-01T08:00:00Z',
      expiresAt: '2027-02-28T08:00:00Z',
    }, 201)) as unknown as typeof fetch
    const client = createPublicApiClient({ baseUrl: 'http://api:8080/api/public/v1', fetchImpl })
    const receipt = await client.submitAnalyticsConsent({
      anonymousSessionId: '059adab1-18af-434e-a93b-4e7d24d5744e',
      policyVersion: 'analytics-v1',
      analyticsAllowed: true,
    })
    expect(receipt.analyticsAllowed).toBe(true)
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://api:8080/api/public/v1/analytics/consents',
      expect.objectContaining({ credentials: 'omit' }),
    )
  })

  it('loads only a valid published product for the comparison workspace', async () => {
    const product = {
      id: '77935cef-4111-4c4c-bdb8-17679a8b42fe', stableId: 'AT-P-001', model: 'Validated model', slug: 'validated-model',
      locale: 'en', family: 'axial', title: 'Published product', specifications: [], performanceCurves: [],
      sourceSnapshotId: 'c44656ad-fc7a-41c0-909e-930466096b37', sourceRevision: '8', currentRevision: 8,
      publishedRevision: 7, status: 'published', indexable: true, updatedAt: '2026-09-01T08:00:00Z',
    }
    const fetchImpl = vi.fn(async () => jsonResponse(product)) as unknown as typeof fetch
    const result = await createPublicApiClient({ baseUrl: 'http://api:8080/api/public/v1', fetchImpl }).getProduct('validated-model', 'axial')
    expect(result.stableId).toBe('AT-P-001')
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://api:8080/api/public/v1/products/validated-model?family=axial',
      expect.objectContaining({ credentials: 'omit' }),
    )
  })

  it('loads an opaque cursor page with bounded public filters', async () => {
    const product = {
      id: '77935cef-4111-4c4c-bdb8-17679a8b42fe', stableId: 'AT-P-001', model: 'Validated model', slug: 'validated-model',
      locale: 'en', family: 'axial', title: 'Published product', specifications: [], performanceCurves: [],
      sourceSnapshotId: 'c44656ad-fc7a-41c0-909e-930466096b37', sourceRevision: '8', currentRevision: 8,
      publishedRevision: 7, status: 'published', indexable: true, updatedAt: '2026-09-01T08:00:00Z',
    }
    const fetchImpl = vi.fn(async () => jsonResponse({ items: [product], nextCursor: 'eyJ2IjoxfQ' })) as unknown as typeof fetch
    const page = await createPublicApiClient({ baseUrl: 'http://api:8080/api/public/v1', fetchImpl }).listProducts({
      family: 'axial', motorTechnology: 'EC', cursor: 'cGFnZS0y', limit: 24,
    })
    expect(page.items[0]?.stableId).toBe('AT-P-001')
    expect(page.nextCursor).toBe('eyJ2IjoxfQ')
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://api:8080/api/public/v1/products?limit=24&family=axial&motorTechnology=EC&cursor=cGFnZS0y',
      expect.objectContaining({ credentials: 'omit' }),
    )
  })

  it('rejects malformed product cursors and page bodies', async () => {
    const client = createPublicApiClient({
      baseUrl: 'http://api:8080/api/public/v1',
      fetchImpl: vi.fn(async () => jsonResponse({ items: [], nextCursor: 42 })) as unknown as typeof fetch,
    })
    await expect(client.listProducts({ cursor: '../admin' })).rejects.toThrow(/cursor is invalid/i)
    await expect(client.listProducts()).rejects.toThrow(/invalid published product page/i)
  })

  it('loads the bootstrap, route and News projections through their canonical public endpoints', async () => {
    const newsIndex = projection()
    const publishedNews = projection({
      id: crypto.randomUUID(),
      kind: 'news',
      templateKey: 'newsDetail',
      slug: 'database-news',
      typeFields: {
        type: 'news', category: 'Company', authorDisplayName: 'Editorial',
        cover: null, publicationAt: '2026-09-01T08:00:00Z', featured: true,
      },
    })
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/site-bootstrap?')) return jsonResponse({
        generalInformation: projection({
          id: crypto.randomUUID(), kind: 'generalInformation', templateKey: 'generalInformation',
          typeFields: {
            type: 'generalInformation', organizationName: 'AIRTEKPOWER', brandLine: null,
            homePath: '/en', footerStatement: null, copyrightTemplate: null,
            contact: { email: null, phone: null, addressLines: [], locality: null, region: null, postalCode: null, countryCode: null },
            socialLinks: [], defaultSeo: { title: null, description: null, indexable: false, socialImage: null },
            productCategories: [], navigationCta: null,
          },
        }),
        navigation: projection({
          id: crypto.randomUUID(), kind: 'navigation', templateKey: 'navigation',
          typeFields: { type: 'navigation', items: [] },
        }),
        footer: projection({
          id: crypto.randomUUID(), kind: 'footer', templateKey: 'footer',
          typeFields: { type: 'footer', columns: [], legalLinks: [] },
        }),
        productFamilies: [{ code: 'axial', slug: 'axial', name: 'Axial', description: 'Published family.', sortOrder: 1 }],
        motorTechnologies: ['EC'],
        generatedAt: '2026-09-01T08:00:00Z',
      })
      if (url.includes('/routes/resolve?')) return jsonResponse({
        path: '/en/resources/news', templateKey: 'newsIndex', entityType: 'content', entityId: newsIndex.id,
        locale: 'en', publishedRevision: 1, indexable: true, dataClass: 'editorial', page: newsIndex,
      })
      return jsonResponse({
        items: [{ content: publishedNews, category: 'Company', authorDisplayName: 'Editorial', coverMediaId: null, publishedAt: '2026-09-01T08:00:00Z', featured: true, dataClass: 'editorial' }],
        nextCursor: null,
      })
    }) as unknown as typeof fetch
    const client = createPublicApiClient({ baseUrl: 'http://api:8080/api/public/v1', fetchImpl })

    const bootstrap = await client.getSiteBootstrap()
    expect(bootstrap.productFamilies[0]?.name).toBe('Axial')
    expect(bootstrap.motorTechnologies).toEqual(['EC'])
    expect((await client.resolveRoute('/en/resources/news')).templateKey).toBe('newsIndex')
    expect((await client.listNews()).items[0]?.content.title).toBe('Database News')
  })

  it('records a consent-bound Guest visit without using the legacy analytics path', async () => {
    const response = {
      id: crypto.randomUUID(), anonymousSessionId: crypto.randomUUID(), landingPath: '/en',
      referrerDomain: 'example.test', source: 'search', medium: null, campaign: null,
      firstSeenAt: '2026-09-01T08:00:00Z', lastSeenAt: '2026-09-01T08:00:00Z', retentionUntil: '2027-09-01T08:00:00Z',
    }
    const fetchImpl = vi.fn(async () => jsonResponse(response, 201)) as unknown as typeof fetch
    const client = createPublicApiClient({ baseUrl: 'http://api:8080/api/public/v1', fetchImpl })
    await client.recordGuestVisit({
      anonymousSessionId: response.anonymousSessionId,
      consentReceipt: crypto.randomUUID(),
      policyVersion: 'analytics-v1',
      landingPath: '/en',
      referrerDomain: 'example.test',
      source: 'search',
    })
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://api:8080/api/public/v1/guest-visits',
      expect.objectContaining({ method: 'POST', credentials: 'omit' }),
    )
  })

  it.each([
    { status: 404, operation: 'news' as const, detail: 'Published News was not found.' },
    { status: 409, operation: 'product' as const, detail: 'The product slug is shared by more than one family.' },
    { status: 503, operation: 'discovery' as const, detail: 'The public site shell projection is incomplete.' },
  ])('preserves Problem Details and HTTP status for $status contract failures', async ({ status, operation, detail }) => {
    const problem = {
      type: `https://api.example.test/problems/${status}`,
      title: status === 404 ? 'Not found' : status === 409 ? 'Conflict' : 'Service unavailable',
      status,
      detail,
      instance: '/api/public/v1/test',
      requestId: '792406a9-2f19-425e-8508-78a205c0c764',
      errors: { projection: ['Published contract failure.'] },
    }
    const fetchImpl = vi.fn(async () => jsonResponse(problem, status)) as unknown as typeof fetch
    const client = createPublicApiClient({ baseUrl: 'http://api:8080/api/public/v1', fetchImpl })
    const request = operation === 'news'
      ? client.getNews('missing-news')
      : operation === 'product'
        ? client.getProduct('shared-slug')
        : client.getDiscovery()

    const error = await request.catch((cause: unknown) => cause)
    expect(error).toBeInstanceOf(PublicApiError)
    expect(error).toMatchObject({
      type: problem.type,
      title: problem.title,
      status,
      detail,
      instance: problem.instance,
      requestId: problem.requestId,
      errors: problem.errors,
      problem,
    })
    expect((error as PublicApiError).response?.status).toBe(status)
  })
})
