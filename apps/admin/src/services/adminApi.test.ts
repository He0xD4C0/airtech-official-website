import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { adminApi, type BackendProduct, type BackendTemporaryOverride } from './adminApi'

vi.mock('virtual:devtools-routes', () => ({ devtoolsPermissions: [] }))

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
