import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { adminApi, productImportResult, waitForOperation, type BackendProduct, type BackendTemporaryOverride } from './adminApi'

function product(id: string): BackendProduct {
  return {
    id,
    stableId: `stable-${id}`,
    model: null,
    slug: `record-${id.slice(0, 8)}`,
    locale: 'en',
    family: 'axial',
    subtype: null,
    motorTechnology: null,
    title: 'API product record',
    summary: null,
    status: 'draft',
    specifications: [],
    performanceCurves: [],
    sourceSnapshotId: '20000000-0000-4000-8000-000000000001',
    sourceRevision: 'source-1',
    currentRevision: 4,
    publishedRevision: null,
    indexable: false,
    seo: { title: null, description: null, canonicalPath: null, indexable: false },
    sortOrder: 0,
    relatedContentIds: [],
    updatedAt: '2026-08-31T00:00:00Z',
  }
}

function response(value: unknown, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(value), { status: 200, headers: { 'Content-Type': 'application/json', ...headers } })
}

beforeEach(() => {
  vi.stubGlobal('document', { cookie: '' })
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('admin product API', () => {
  it('reads the flattened admin product detail from its dedicated resource', async () => {
    const record = {
      ...product('10000000-0000-4000-8000-000000000001'),
      sourceKind: 'verifiedCsv',
      missingAssets: [],
      presentation: null,
    }
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      void input
      return response(record, { ETag: '"revision-4"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.getProduct(record.id)).resolves.toMatchObject({
      id: record.id,
      sourceKind: 'verifiedCsv',
      presentation: null,
    })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain(`/products/${record.id}`)
  })

  it('updates only the product presentation contract with an exact revision precondition', async () => {
    const record = { ...product('10000000-0000-4000-8000-000000000001'), currentRevision: 9 }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response({
        ...record,
        currentRevision: 9,
        presentation: {
          locale: 'en', slug: 'portal-title', title: 'Portal title', summary: null,
          seo: { title: null, description: null, canonicalPath: '/en/products/axial/portal-title', indexable: false },
          indexable: false, sortOrder: 0, relatedContentIds: [], revision: 5, publishedRevision: null,
          updatedAt: '2026-09-02T00:00:00Z',
        },
        sourceKind: 'verifiedCsv', missingAssets: [],
      }, { ETag: '"revision-5"' })
    })
    vi.stubGlobal('fetch', fetchMock)
    const payload = {
      locale: 'en' as const,
      slug: 'portal-title',
      title: 'Portal title',
      summary: null,
      seo: { title: null, description: null, canonicalPath: '/en/products/axial/portal-title', indexable: false },
      indexable: false,
      sortOrder: 0,
      relatedContentIds: [],
      reason: 'Update reviewed website presentation',
    }

    await expect(adminApi.updateProductPresentation(record.id, 4, payload)).resolves.toMatchObject({
      product: { currentRevision: 9, presentation: { revision: 5 }, sourceKind: 'verifiedCsv' },
      etag: '"revision-5"',
    })
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain(`/products/${record.id}/presentation`)
    expect(init?.method).toBe('PATCH')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-4"')
    expect(JSON.parse(String(init?.body))).toEqual(payload)
  })

  it('finds a product outside the first cursor page', async () => {
    const targetId = '10000000-0000-4000-8000-000000000002'
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      return url.includes('cursor=after-first')
        ? response({ items: [product(targetId)], nextCursor: null })
        : response({ items: [product('10000000-0000-4000-8000-000000000001')], nextCursor: 'after-first' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.findProduct(targetId)).resolves.toMatchObject({ id: targetId })
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain('/products?limit=100')
    expect(String(fetchMock.mock.calls[1]?.[0])).toContain('cursor=after-first&limit=100')
  })

  it('collects temporary overrides across every cursor page', async () => {
    const override = (id: string): BackendTemporaryOverride => ({
      id,
      productId: '10000000-0000-4000-8000-000000000001',
      fieldPath: 'specifications.airflow',
      value: null,
      reason: 'Controlled exception',
      createdAt: '2026-08-01T00:00:00Z',
      expiresAt: '2026-09-30T00:00:00Z',
      expired: false,
    })
    const fetchMock = vi.fn(async (input: string | URL | Request) => String(input).includes('cursor=next-overrides')
      ? response({ items: [override('override-2')], nextCursor: null })
      : response({ items: [override('override-1')], nextCursor: 'next-overrides' }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.listAllTemporaryOverrides('10000000-0000-4000-8000-000000000001'))
      .resolves.toHaveLength(2)
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain('/products/10000000-0000-4000-8000-000000000001/temporary-overrides?limit=100')
  })

  it('publishes with If-Match and an idempotency key and returns the ETag', async () => {
    const record = product('10000000-0000-4000-8000-000000000001')
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response({ ...record, status: 'published', publishedRevision: 4 }, { ETag: '"revision-4"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.publishProduct(record.id, 4, 'product-publish-test-key')).resolves.toMatchObject({
      product: { status: 'published', publishedRevision: 4 },
      etag: '"revision-4"',
    })
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain(`/products/${record.id}/publish`)
    expect(init?.method).toBe('POST')
    const headers = new Headers(init?.headers)
    expect(headers.get('If-Match')).toBe('"revision-4"')
    expect(headers.get('Idempotency-Key')).toBe('product-publish-test-key')
  })
})

describe('admin content preview API', () => {
  it('issues an exact-revision short-lived preview with the concurrency precondition', async () => {
    const contentId = '30000000-0000-4000-8000-000000000001'
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return new Response(JSON.stringify({
        url: 'http://www.localhost:8088/en/preview?token=signed-token',
        contentId,
        revision: 7,
        issuedAt: '2026-09-01T00:00:00Z',
        expiresAt: '2026-09-01T00:10:00Z',
      }), { status: 201, headers: { 'Content-Type': 'application/json' } })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.createContentPreview(contentId, 7)).resolves.toMatchObject({
      contentId,
      revision: 7,
    })
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain(`/content/${contentId}/preview`)
    expect(init?.method).toBe('POST')
    const headers = new Headers(init?.headers)
    expect(headers.get('If-Match')).toBe('"revision-7"')
    expect(JSON.parse(String(init?.body))).toEqual({ revision: 7, expiresInSeconds: 600 })
  })
})

describe('platform settings API', () => {
  it('reads the settings ETag and updates only the allow-listed policy fields', async () => {
    const current = {
      rfqRetentionDays: 365,
      retentionDeletionGraceDays: 30,
      temporaryOverrideDefaultDays: 30,
      publicLocale: 'en' as const,
      revision: 6,
    }
    const fetchMock = vi.fn(async (_input: string | URL | Request, init?: RequestInit) => {
      if (init?.method === 'PATCH') {
        return response({ ...current, rfqRetentionDays: 540, revision: 7 }, { ETag: '"revision-7"' })
      }
      return response(current, { ETag: '"revision-6"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.getSettings()).resolves.toEqual({ settings: current, etag: '"revision-6"' })
    await expect(adminApi.updateSettings({
      rfqRetentionDays: 540,
      reason: 'Align retention with approved policy',
    }, 6)).resolves.toMatchObject({
      settings: { rfqRetentionDays: 540, publicLocale: 'en', revision: 7 },
      etag: '"revision-7"',
    })

    const [url, init] = fetchMock.mock.calls[1] ?? []
    expect(String(url)).toContain('/settings')
    expect(init?.method).toBe('PATCH')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-6"')
    expect(JSON.parse(String(init?.body))).toEqual({
      rfqRetentionDays: 540,
      reason: 'Align retention with approved policy',
    })
  })
})

describe('database-driven management APIs', () => {
  it('accepts an invitation through the unauthenticated contract endpoint', async () => {
    const acceptance = {
      userId: '91000000-0000-4000-8000-000000000001', email: 'invitee@example.test', displayName: 'Invitee',
      locale: 'zh-CN', roleKeys: ['content-editor'], status: 'active', acceptedAt: '2026-09-02T00:00:00Z',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return new Response(JSON.stringify(acceptance), { status: 201, headers: { 'Content-Type': 'application/json' } })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.acceptInvitation({ token: 'a'.repeat(43), password: 'secure-password-123' })).resolves.toEqual(acceptance)
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain('/api/admin/v1/auth/invitations/accept')
    expect(JSON.parse(String(init?.body))).toEqual({ token: 'a'.repeat(43), password: 'secure-password-123' })
  })

  it('lets the server apply its deployed Product Master mapping and sends idempotency', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response({
        operationId: '50000000-0000-4000-8000-000000000001',
        status: 'queued',
        operationUrl: '/api/admin/v1/operations/50000000-0000-4000-8000-000000000001',
        eventsUrl: '/api/admin/v1/operations/50000000-0000-4000-8000-000000000001/events',
      })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.importProductMaster('model,voltage\nA,230')).resolves.toMatchObject({
      status: 'queued',
      operationId: '50000000-0000-4000-8000-000000000001',
    })
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain('/products/imports')
    expect(init?.method).toBe('POST')
    expect(new Headers(init?.headers).get('Idempotency-Key')).toBeTruthy()
    expect(JSON.parse(String(init?.body))).toEqual({
      csv: 'model,voltage\nA,230',
    })
  })

  it('saves General Information with an exact revision precondition', async () => {
    const entry = {
      id: '60000000-0000-4000-8000-000000000001',
      locale: 'en' as const,
      payload: {
        brandName: 'AIRTEKPOWER', brandLine: null, homePath: '/en', footerStatement: null,
        copyrightText: null, defaultSeo: { title: null, description: null },
        organization: {
          name: 'AIRTEKPOWER', url: null, logoUrl: null, legalName: null, salesEmail: null,
          marketingEmail: null, address: null, socialLinks: [],
        },
        navigationCta: { label: 'Request a Quote', href: '/en/request-a-quote' },
      },
      status: 'draft' as const,
      currentRevision: 3,
      publishedRevision: null,
      isPlaceholder: true,
      updatedAt: '2026-09-02T00:00:00Z',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response(entry, { ETag: '"revision-3"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.saveGeneralInformation({ locale: 'en', payload: entry.payload, isPlaceholder: true }, entry.id, 2)
    const [, init] = fetchMock.mock.calls[0] ?? []
    expect(init?.method).toBe('PATCH')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-2"')
  })

  it('rolls General Information back by publishing a selected historical revision with a reason', async () => {
    const entry = {
      id: '60000000-0000-4000-8000-000000000001', locale: 'en', payload: {}, status: 'published',
      currentRevision: 5, publishedRevision: 5, isPlaceholder: true, updatedAt: '2026-09-02T00:00:00Z',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response(entry, { ETag: '"revision-5"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.rollbackGeneralInformation(entry.id, 4, 2, 'Restore the reviewed site identity')
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain(`/general-information/${entry.id}/rollback`)
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-4"')
    expect(JSON.parse(String(init?.body))).toEqual({ revision: 2, reason: 'Restore the reviewed site identity' })
  })

  it('keeps News metadata separate from the versioned content document', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response({
      content: {
        id: '70000000-0000-4000-8000-000000000001', kind: 'news', slug: 'development-news', locale: 'en',
        title: 'Development news', summary: null, body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
        seo: { title: null, description: null, canonicalPath: '/en/resources/news/development-news', indexable: false },
        status: 'draft', isPlaceholder: true, currentRevision: 1, publishedRevision: null, updatedAt: '2026-09-02T00:00:00Z',
      },
      category: 'Development', authorDisplayName: 'AIRTEKPOWER', coverMediaId: null, publishedAt: null,
      featured: false, dataClass: 'developmentFixture',
      })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.saveNews({
      content: {
        kind: 'news', slug: 'development-news', locale: 'en', title: 'Development news', summary: null,
        body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
        seo: { title: null, description: null, canonicalPath: '/en/resources/news/development-news', indexable: false },
        isPlaceholder: true,
      },
      category: 'Development', authorDisplayName: 'AIRTEKPOWER', featured: false, dataClass: 'developmentFixture',
    })
    const [, init] = fetchMock.mock.calls[0] ?? []
    const body = JSON.parse(String(init?.body))
    expect(body.content.kind).toBe('news')
    expect(body.category).toBe('Development')
    expect(body.content.body.doc).not.toHaveProperty('category')
  })

  it('sends the target revision and an auditable reason for News rollback', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response({
      content: {
        id: '70000000-0000-4000-8000-000000000001', kind: 'news', slug: 'news', locale: 'en',
        title: 'News', summary: null, body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
        seo: { title: null, description: null, canonicalPath: null, indexable: false }, status: 'published',
        isPlaceholder: true, currentRevision: 5, publishedRevision: 5, updatedAt: '2026-09-02T00:00:00Z',
      },
      category: 'Development', authorDisplayName: 'AIRTEKPOWER', coverMediaId: null,
      publishedAt: null, featured: false, dataClass: 'developmentFixture',
      })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.rollbackNews(
      '70000000-0000-4000-8000-000000000001',
      4,
      2,
      'Restore the reviewed revision',
    )
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain('/news/70000000-0000-4000-8000-000000000001/rollback')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-4"')
    expect(JSON.parse(String(init?.body))).toEqual({ revision: 2, reason: 'Restore the reviewed revision' })
  })

  it('uses the invitation resource and preserves its one-time token response', async () => {
    const invitation = {
      id: '80000000-0000-4000-8000-000000000001', email: 'editor@example.test', displayName: 'Editor',
      locale: 'zh-CN', roleKeys: ['content-editor'], status: 'pending', invitedAt: '2026-09-02T00:00:00Z',
      expiresAt: '2026-09-09T00:00:00Z', invitationToken: 'one-time-token',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response(invitation)
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.inviteUser({
      email: invitation.email,
      displayName: invitation.displayName,
      roleKeys: invitation.roleKeys,
    })).resolves.toMatchObject({ invitationToken: 'one-time-token' })
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain('/user-invitations')
    expect(String(url)).not.toContain('/users')
    expect(init?.method).toBe('POST')
  })

  it('revokes invitations through the invitation resource with an audit reason', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return new Response(null, { status: 204 })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.revokeInvitation(
      '80000000-0000-4000-8000-000000000001',
      'Invitation was sent to the wrong address',
    )
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain('/user-invitations/80000000-0000-4000-8000-000000000001/revoke')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(String(init?.body))).toEqual({ reason: 'Invitation was sent to the wrong address' })
  })

  it('updates users with the exact revision ETag', async () => {
    const user = {
      id: '90000000-0000-4000-8000-000000000001', email: 'editor@example.test', displayName: 'Editor',
      locale: 'zh-CN', status: 'active', roles: ['content-editor'], totpEnabled: true, invitedAt: null,
      lastLoginAt: null, createdAt: '2026-09-02T00:00:00Z', updatedAt: '2026-09-02T00:00:00Z', revision: 3,
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response(user)
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.updateUser(user.id, 3, { displayName: 'Updated editor', reason: 'Correct the display name' })
    const [, init] = fetchMock.mock.calls[0] ?? []
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-3"')
    expect(new Headers(init?.headers).get('Idempotency-Key')).toBeTruthy()
  })

  it('loads immutable News and General Information revision lists from their contract routes', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      return response({ items: [], nextCursor: null }, url.includes('/revisions') ? {} : {})
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.listNewsRevisions('70000000-0000-4000-8000-000000000001')
    await adminApi.listGeneralInformationRevisions('71000000-0000-4000-8000-000000000001')

    expect(String(fetchMock.mock.calls[0]?.[0])).toContain('/api/admin/v1/news/70000000-0000-4000-8000-000000000001/revisions')
    expect(String(fetchMock.mock.calls[1]?.[0])).toContain('/api/admin/v1/general-information/71000000-0000-4000-8000-000000000001/revisions')
  })

  it('reads only aggregate analytics contracts without a visitor identifier', async () => {
    const visit = { bucketDate: '2026-09-02', landingPath: '/en', locale: 'en', visits: 3, pageViews: 8, rfqStarts: 1, rfqSubmissions: 0 }
    const source = { ...visit, source: 'referral', sourceName: 'Referral', referrerDomain: 'example.test', utmSource: null, medium: null, campaign: null }
    const fetchMock = vi.fn(async (input: string | URL | Request) => response({ items: String(input).includes('/sources') ? [source] : [visit], nextCursor: null }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.listGuestVisits({ cursor: 'visit-cursor', limit: 25 })).resolves.toEqual({ items: [visit], nextCursor: null })
    await expect(adminApi.listGuestSources({ cursor: 'source-cursor', limit: 10 })).resolves.toEqual({ items: [source], nextCursor: null })
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain('/analytics/visits?cursor=visit-cursor&limit=25')
    expect(String(fetchMock.mock.calls[1]?.[0])).toContain('/analytics/sources?cursor=source-cursor&limit=10')
    expect(JSON.stringify(fetchMock.mock.calls)).not.toContain('anonymousSessionId')
  })

  it('loads private pricing only from the permissioned no-store endpoint', async () => {
    const privateRecord = {
      productId: '10000000-0000-4000-8000-000000000001',
      stableId: 'stable-record-1',
      sourceRowNumber: 2,
      pricingFields: { quotation: 'restricted-value' },
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response(privateRecord)
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.getProductPrivatePricing(privateRecord.productId)).resolves.toEqual(privateRecord)
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain(`/products/${privateRecord.productId}/private-pricing`)
    expect(init?.cache).toBe('no-store')
  })

  it('updates roles with the exact revision ETag and audit reason', async () => {
    const role = { id: '92000000-0000-4000-8000-000000000001', key: 'publisher', displayName: 'Publisher', systemRole: true, permissions: ['content.publish'], revision: 5 }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response({ ...role, revision: 6 }, { ETag: '"revision-6"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.updateRole(role.id, 5, { displayName: 'Publisher', permissions: role.permissions, reason: 'Align approved permission matrix' })
    const [, init] = fetchMock.mock.calls[0] ?? []
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-5"')
    expect(new Headers(init?.headers).get('Idempotency-Key')).toBeTruthy()
  })

  it('waits on the accepted operation resource and extracts the import report', async () => {
    const report = {
      id: 'a0000000-0000-4000-8000-000000000001', checksum: 'abc', mappingVersion: 'airtek-basic-v1',
      status: 'readyToPublish', totalRows: 375, validRows: 370, malformedRows: 5, errors: [],
      missingAssets: [], reused: false, createdAt: '2026-09-02T00:00:00Z',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response({
      id: 'b0000000-0000-4000-8000-000000000001', kind: 'productImport', status: 'completed',
      reason: 'Import verified Product Master CSV', result: { import: report },
      createdAt: '2026-09-02T00:00:00Z', updatedAt: '2026-09-02T00:00:00Z',
      })
    })
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('EventSource', undefined)

    const operation = await waitForOperation({
      operationId: 'b0000000-0000-4000-8000-000000000001',
      status: 'completed',
      operationUrl: '/api/admin/v1/operations/b0000000-0000-4000-8000-000000000001',
      eventsUrl: '/api/admin/v1/operations/b0000000-0000-4000-8000-000000000001/events',
    })
    expect(productImportResult(operation)).toEqual(report)
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain('/operations/b0000000-0000-4000-8000-000000000001')
  })
})

describe('contract transport compatibility', () => {
  it('filters development-only and unknown permissions when the virtual boundary is disabled', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => response({
      id: '92000000-0000-4000-8000-000000000001',
      displayName: 'Production Admin',
      email: 'admin@example.test',
      role: 'Developer',
      permissions: ['dashboard.read', 'devtools.shell', 'future.permission'],
      environment: 'production',
      totpEnabled: true,
    })))

    await expect(adminApi.session()).resolves.toMatchObject({
      permissions: ['dashboard.read'],
      environment: 'production',
    })
  })

  it('rotates CSRF from a safe response and sends it only with the following mutation', async () => {
    const session = {
      id: '93000000-0000-4000-8000-000000000001',
      displayName: 'Contract Admin',
      email: 'admin@example.test',
      role: 'Super Admin',
      permissions: ['settings.manage'],
      environment: 'development',
      totpEnabled: true,
    }
    const settings = {
      rfqRetentionDays: 365,
      retentionDeletionGraceDays: 30,
      temporaryOverrideDefaultDays: 30,
      publicLocale: 'en',
      revision: 2,
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void init
      const url = String(input)
      if (url.endsWith('/auth/logout')) return new Response(null, { status: 204 })
      if (url.endsWith('/auth/session')) {
        return response(session, { 'X-CSRF-Token': 'rotated-contract-token' })
      }
      return response(settings, { ETag: '"revision-2"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.logout()
    fetchMock.mockClear()
    await adminApi.session()
    await adminApi.updateSettings({ rfqRetentionDays: 365, reason: 'Verify CSRF transport' }, 1)

    const [, sessionInit] = fetchMock.mock.calls[0] ?? []
    const [, mutationInit] = fetchMock.mock.calls[1] ?? []
    expect(new Headers(sessionInit?.headers).has('X-CSRF-Token')).toBe(false)
    expect(new Headers(mutationInit?.headers).get('X-CSRF-Token')).toBe('rotated-contract-token')
    await adminApi.logout()
  })

  it('preserves direct Problem Details fields for existing Admin error consumers', async () => {
    const problem = {
      type: 'https://airtek.example/problems/validation',
      title: 'Validation failed',
      status: 422,
      detail: 'Credentials were rejected by the contract endpoint.',
      requestId: '94000000-0000-4000-8000-000000000001',
    }
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify(problem), {
      status: 422,
      headers: { 'Content-Type': 'application/problem+json' },
    })))

    await expect(adminApi.login('admin@example.test', 'invalid-password')).rejects.toMatchObject({
      status: 422,
      detail: problem.detail,
      problem,
    })
  })
})
