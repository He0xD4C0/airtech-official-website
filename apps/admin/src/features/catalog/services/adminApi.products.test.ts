import { adminProductApi } from '@/features/catalog/services/adminProductApi'
import { describe, expect, it, vi } from 'vitest'
import { type BackendTemporaryOverride } from '@/shared/services/adminApiTypes'
import { waitForOperation } from '@/shared/services/operationPolling'
import { productImportResult } from '@/features/catalog/services/productImportResult'
import { product, response, setupAdminApiTestEnvironment } from '@/shared/services/adminApi.testSupport'

setupAdminApiTestEnvironment()

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

    await expect(adminProductApi.getProduct(record.id)).resolves.toMatchObject({
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
          indexable: false, sortOrder: 0, relatedContentIds: [], mediaGallery: [], revision: 5, publishedRevision: null,
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
      mediaGallery: [],
      reason: 'Update reviewed website presentation',
    }

    await expect(adminProductApi.updateProductPresentation(record.id, 4, payload)).resolves.toMatchObject({
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
        ? response(productPage([product(targetId)], null))
        : response(productPage([product('10000000-0000-4000-8000-000000000001')], 'after-first'))
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminProductApi.findProduct(targetId)).resolves.toMatchObject({ id: targetId })
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

    await expect(adminProductApi.listAllTemporaryOverrides('10000000-0000-4000-8000-000000000001'))
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

    await expect(adminProductApi.publishProduct(record.id, 4, 'product-publish-test-key')).resolves.toMatchObject({
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

    await expect(adminProductApi.importProductMaster('model,voltage\nA,230')).resolves.toMatchObject({
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

    await expect(adminProductApi.getProductPrivatePricing(privateRecord.productId)).resolves.toEqual(privateRecord)
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain(`/products/${privateRecord.productId}/private-pricing`)
    expect(init?.cache).toBe('no-store')
  })

  it('queries admin-only supplier and brand archive metadata', async () => {
    const record = {
      id: '70000000-0000-4000-8000-000000000001',
      kind: 'supplier',
      label: 'Supplier A',
      sourceTable: 'tblhKpZRLfUIlGYo',
      sourceRecordId: 'rec-supplier-1',
      archiveSha256: 'a'.repeat(64),
      attributes: { 权重: 'P1' },
      rawFields: { 供应商名称: 'Supplier A', 权重: 'P1' },
      capturedAt: '2026-10-01T00:00:00Z',
      importedAt: '2026-10-01T00:00:00Z',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      void input
      return response({ items: [record], nextCursor: null })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminProductApi.listSourceMetadata({
      kind: 'supplier',
      q: 'Supplier A',
      limit: 25,
    })).resolves.toMatchObject({ items: [record], nextCursor: null })
    const url = new URL(String(fetchMock.mock.calls[0]?.[0]))
    expect(url.pathname).toContain('/source-metadata')
    expect(url.searchParams.get('kind')).toBe('supplier')
    expect(url.searchParams.get('q')).toBe('Supplier A')
    expect(url.searchParams.get('limit')).toBe('25')
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

function productPage(items: ReturnType<typeof product>[], nextCursor: string | null) {
  return {
    items,
    nextCursor,
    total: items.length,
    familyCounts: [],
    statusCounts: [],
    dataStateCounts: [],
  }
}
