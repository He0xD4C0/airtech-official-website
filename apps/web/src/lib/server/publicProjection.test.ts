import { describe, expect, it, vi } from 'vitest'
import { resolvePublicRoute } from '@/content/routes'
import { loadPublishedProjection } from './publicProjection'

function jsonResponse(value: unknown, status = 200) {
  return new Response(JSON.stringify(value), { status, headers: { 'Content-Type': 'application/json' } })
}

const contentProjection = {
  id: '77935cef-4111-4c4c-bdb8-17679a8b42fe', kind: 'technology', slug: 'control', locale: 'en',
  title: 'Published control content', summary: 'Content from the published projection.',
  body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
  seo: { title: 'Published control | AIRTEKPOWER', description: 'Published metadata.', canonicalPath: '/en/technology/control', indexable: true },
  status: 'published', isPlaceholder: false, currentRevision: 4, publishedRevision: 3, updatedAt: '2026-09-01T08:00:00Z',
}

const productProjection = {
  id: '77935cef-4111-4c4c-bdb8-17679a8b42fe', stableId: 'AT-P-001', model: 'Verified model', slug: 'verified-product',
  locale: 'en', family: 'axial', title: 'Published axial product', summary: 'Published product summary.',
  subtype: null, motorTechnology: null,
  specifications: [], performanceCurves: [], sourceSnapshotId: 'c44656ad-fc7a-41c0-909e-930466096b37',
  sourceRevision: 'source-8', currentRevision: 8, publishedRevision: 7, status: 'published', indexable: true,
  updatedAt: '2026-09-01T08:00:00Z',
}

describe('server-only published projection loader', () => {
  it('loads CMS content from the internal API before SSR', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse(contentProjection)) as unknown as typeof fetch
    const page = await loadPublishedProjection(resolvePublicRoute('/en/technology/control'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://api:8080/api/public/v1/content/technology/control?locale=en',
      expect.objectContaining({ headers: { Accept: 'application/json' } }),
    )
    expect(page.title).toBe('Published control content')
    expect(page.indexable).toBe(true)
    expect(page.dataState).toBe('published')
  })

  it('fails closed to an explicit noindex placeholder', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({ title: 'Unavailable' }, 503)) as unknown as typeof fetch
    const page = await loadPublishedProjection(resolvePublicRoute('/en/technology/control'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(page.indexable).toBe(false)
    expect(page.dataState).toBe('placeholder')
    expect(page.placeholderReason).toMatch(/no product values|published content projection/i)
    expect(page.publishedContent).toBeUndefined()
  })

  it('uses a published product to create immutable detail and Product RFQ context', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse(productProjection)) as unknown as typeof fetch
    const detail = await loadPublishedProjection(resolvePublicRoute('/en/products/axial/verified-product'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(detail.title).toBe('Published axial product')
    expect(detail.productContext).toEqual({
      productId: productProjection.id, stableId: 'AT-P-001', model: 'Verified model', publishedRevision: 7,
    })

    const rfq = await loadPublishedProjection(resolvePublicRoute('/en/request-a-quote/product'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl, productSlug: 'verified-product',
    })
    expect(rfq.productContext).toEqual(detail.productContext)
  })

  it('loads the catalog and family from the published product list projection', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({ items: [productProjection], nextCursor: null })) as unknown as typeof fetch
    const catalog = await loadPublishedProjection(resolvePublicRoute('/en/products'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://api:8080/api/public/v1/products?limit=24',
      expect.objectContaining({ headers: { Accept: 'application/json' } }),
    )
    expect(catalog.publishedProducts).toEqual([productProjection])
    expect(catalog.dataState).toBe('published')

    const family = await loadPublishedProjection(resolvePublicRoute('/en/products/axial'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(fetchImpl).toHaveBeenLastCalledWith(
      'http://api:8080/api/public/v1/products?limit=24&family=axial',
      expect.any(Object),
    )
    expect(family.publishedProducts).toEqual([productProjection])
  })

  it('carries the opaque next cursor from SSR into the hydrated catalog', async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({ items: [productProjection], nextCursor: 'opaque-page-2' })) as unknown as typeof fetch
    const page = await loadPublishedProjection(resolvePublicRoute('/en/products'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(page.productNextCursor).toBe('opaque-page-2')
  })

  it('fails the catalog closed when the product list is not a published projection', async () => {
    const draft = { ...productProjection, status: 'draft' }
    const page = await loadPublishedProjection(resolvePublicRoute('/en/products'), {
      baseUrl: 'http://api:8080/api/public/v1',
      fetchImpl: vi.fn(async () => jsonResponse({ items: [draft] })) as unknown as typeof fetch,
    })
    expect(page.publishedProducts).toBeUndefined()
    expect(page.indexable).toBe(false)
    expect(page.placeholderReason).toMatch(/catalog is unavailable/i)
  })

  it('keeps an empty catalog out of the index and reports the honest empty state', async () => {
    const page = await loadPublishedProjection(resolvePublicRoute('/en/products/axial'), {
      baseUrl: 'http://api:8080/api/public/v1',
      fetchImpl: vi.fn(async () => jsonResponse({ items: [], nextCursor: null })) as unknown as typeof fetch,
    })
    expect(page.indexable).toBe(false)
    expect(page.dataState).toBe('placeholder')
    expect(page.publishedProducts).toEqual([])
    expect(page.placeholderReason).toMatch(/no published Product Master records/i)
  })

  it('rejects malformed product records at the SSR trust boundary', async () => {
    for (const malformed of [
      { ...productProjection, slug: '../admin' },
      { ...productProjection, model: 42 },
      { ...productProjection, indexable: 'yes' },
      { ...productProjection, performanceCurves: [{ state: 'verified', points: [{ airflow: Number.NaN, pressure: 20 }] }] },
    ]) {
      const page = await loadPublishedProjection(resolvePublicRoute('/en/products/axial/verified-product'), {
        baseUrl: 'http://api:8080/api/public/v1',
        fetchImpl: vi.fn(async () => jsonResponse(malformed)) as unknown as typeof fetch,
      })
      expect(page.indexable).toBe(false)
      expect(page.publishedProduct).toBeUndefined()
    }
  })

  it('does not invent Product RFQ context for a generic entry', async () => {
    const page = await loadPublishedProjection(resolvePublicRoute('/en/request-a-quote/product'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl: vi.fn() as unknown as typeof fetch,
    })
    expect(page.productContext).toBeUndefined()
    expect(page.indexable).toBe(false)
    expect(page.placeholderReason).toMatch(/requires a published product route/i)
  })

  it('promotes a valid dynamic route only with its matching published projection', async () => {
    const route = resolvePublicRoute('/en/technology/new-verified-topic')
    const projection = {
      ...contentProjection,
      slug: 'new-verified-topic',
      title: 'New verified topic',
      seo: { ...contentProjection.seo, canonicalPath: '/en/technology/new-verified-topic' },
      body: { schemaVersion: 1, doc: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Published body.' }] }] } },
    }
    const promoted = await loadPublishedProjection(route, {
      baseUrl: 'http://api:8080/api/public/v1',
      fetchImpl: vi.fn(async () => jsonResponse(projection)) as unknown as typeof fetch,
    })
    expect(promoted.dataState).toBe('published')
    expect(promoted.indexable).toBe(true)
    expect(promoted.publishedContent?.body).toEqual(projection.body)

    const mismatched = await loadPublishedProjection(route, {
      baseUrl: 'http://api:8080/api/public/v1',
      fetchImpl: vi.fn(async () => jsonResponse({ ...projection, slug: 'other-topic' })) as unknown as typeof fetch,
    })
    expect(mismatched.dataState).toBe('placeholder')
    expect(mismatched.indexable).toBe(false)
    expect(mismatched.publishedContent).toBeUndefined()
  })

  it('builds collection entries only from canonical published discovery records', async () => {
    const indexProjection = {
      ...contentProjection,
      kind: 'solution',
      slug: 'index',
      title: 'Published solutions',
      seo: { ...contentProjection.seo, canonicalPath: '/en/solutions' },
    }
    const discovery = {
      generatedAt: '2026-09-01T08:05:00Z',
      entries: [
        { entityType: 'content', entityId: '4ecdc1c2-f30d-4f7d-82e8-29009c6b285b', path: '/en/solutions/verified-application', locale: 'en', title: 'Verified application', summary: 'Published summary.', updatedAt: '2026-09-01T08:00:00Z' },
        { entityType: 'content', entityId: '87b65911-42ac-4f6e-abbb-fce875827d96', path: '/en/technology/control', locale: 'en', title: 'Wrong collection', summary: null, updatedAt: '2026-09-01T08:00:00Z' },
        { entityType: 'content', entityId: 'cf8858bd-1bfe-4ec5-b76a-4546bd74a2fc', path: '/en/solutions/nested/record', locale: 'en', title: 'Nested route', summary: null, updatedAt: '2026-09-01T08:00:00Z' },
        { entityType: 'product', entityId: productProjection.id, path: '/en/solutions/not-content', locale: 'en', title: 'Product', summary: null, updatedAt: '2026-09-01T08:00:00Z' },
        { entityType: 'content', entityId: 'c64fc0f5-a3a7-474a-be0a-4be61950ebee', path: '/en/solutions/no-title', locale: 'en', updatedAt: '2026-09-01T08:00:00Z' },
      ],
    }
    const fetchImpl = vi.fn(async (input: string | URL | Request) => (
      String(input).endsWith('/discovery') ? jsonResponse(discovery) : jsonResponse(indexProjection)
    )) as unknown as typeof fetch
    const page = await loadPublishedProjection(resolvePublicRoute('/en/solutions'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(page.entries).toEqual([expect.objectContaining({
      slug: 'verified-application',
      title: 'Verified application',
      summary: 'Published summary.',
      href: '/en/solutions/verified-application',
    })])
  })

  it('does not fall back to demo collection entries when discovery is unavailable', async () => {
    const indexProjection = {
      ...contentProjection,
      kind: 'article',
      slug: 'index',
      title: 'Published articles',
      seo: { ...contentProjection.seo, canonicalPath: '/en/resources/articles' },
    }
    const fetchImpl = vi.fn(async (input: string | URL | Request) => (
      String(input).endsWith('/discovery') ? jsonResponse({}, 503) : jsonResponse(indexProjection)
    )) as unknown as typeof fetch
    const page = await loadPublishedProjection(resolvePublicRoute('/en/resources/articles'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(page.entries).toEqual([])
    expect(page.title).toBe('Published articles')
  })

  it('enriches the Downloads list only with safe descriptive fields from each published record', async () => {
    const indexProjection = {
      ...contentProjection,
      kind: 'download', slug: 'index', title: 'Published downloads',
      seo: { ...contentProjection.seo, canonicalPath: '/en/resources/downloads' },
    }
    const recordProjection = {
      ...contentProjection,
      kind: 'download', slug: 'approved-record', title: 'Approved record',
      body: { schemaVersion: 1, doc: { type: 'doc', attrs: {
        version: 'V4', applicableModels: ['MODEL-A'], resourceType: 'Datasheet',
        fileDescription: 'Published description.', downloadUrl: '/media/public/approved.pdf',
        fileStatus: { scan: 'clean', access: 'public' },
      }, content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Published body.' }] }] } },
      seo: { ...contentProjection.seo, canonicalPath: '/en/resources/downloads/approved-record' },
    }
    const discovery = {
      generatedAt: '2026-09-01T08:05:00Z',
      entries: [{
        entityType: 'content', entityId: recordProjection.id, path: '/en/resources/downloads/approved-record',
        locale: 'en', title: 'Approved record', summary: 'Published summary.', updatedAt: '2026-09-01T08:00:00Z',
      }],
    }
    const fetchImpl = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      if (url.endsWith('/discovery')) return jsonResponse(discovery)
      if (url.includes('/content/downloads/approved-record?')) return jsonResponse(recordProjection)
      return jsonResponse(indexProjection)
    }) as unknown as typeof fetch
    const page = await loadPublishedProjection(resolvePublicRoute('/en/resources/downloads'), {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(page.entries).toEqual([expect.objectContaining({
      slug: 'approved-record',
      download: {
        version: 'V4', applicableModels: ['MODEL-A'], resourceType: 'Datasheet',
        fileDescription: 'Published description.',
      },
    })])
    expect(JSON.stringify(page.entries)).not.toContain('/media/public/approved.pdf')
    expect(JSON.stringify(page.entries)).not.toContain('fileStatus')
  })

  it('renders Search from published discovery instead of local demo entries', async () => {
    const discovery = {
      generatedAt: '2026-09-01T08:05:00Z',
      entries: [
        { entityType: 'product', entityId: productProjection.id, path: '/en/products/axial/verified-product', locale: 'en', title: 'Published axial product', summary: 'Published product summary.', updatedAt: '2026-09-01T08:00:00Z' },
        { entityType: 'content', entityId: contentProjection.id, path: '/en/technology/control', locale: 'en', title: 'Published control content', summary: 'Published metadata.', updatedAt: '2026-09-01T08:00:00Z' },
        { entityType: 'content', entityId: crypto.randomUUID(), path: '/en/private/record', locale: 'en', title: 'Private record', summary: 'Must not leak into search.', updatedAt: '2026-09-01T08:00:00Z' },
        { entityType: 'content', entityId: crypto.randomUUID(), path: '/en/products/compare', locale: 'en', title: 'Compare', summary: null, updatedAt: '2026-09-01T08:00:00Z' },
      ],
    }
    const page = await loadPublishedProjection(resolvePublicRoute('/en/search'), {
      baseUrl: 'http://api:8080/api/public/v1',
      fetchImpl: vi.fn(async () => jsonResponse(discovery)) as unknown as typeof fetch,
    })
    expect(page.entries).toEqual([
      expect.objectContaining({ title: 'Published axial product', eyebrow: 'Product' }),
      expect.objectContaining({ title: 'Published control content', eyebrow: 'Technology' }),
    ])
    expect(JSON.stringify(page.entries)).not.toContain('resourceSearchEntries')
    expect(JSON.stringify(page.entries)).not.toContain('Private record')
    expect(JSON.stringify(page.entries)).not.toContain('Compare')
  })
})
