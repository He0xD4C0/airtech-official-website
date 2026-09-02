import { describe, expect, it, vi } from 'vitest'
import { loadPublicPageData } from './publicPageData'

function json(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), { status, headers: { 'Content-Type': 'application/json' } })
}

function content(overrides: Record<string, unknown> = {}) {
  return {
    id: '792406a9-2f19-425e-8508-78a205c0c764', kind: 'home', slug: 'home', locale: 'en',
    title: 'Title from content projection', summary: 'Summary from content projection.',
    body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
    seo: { title: 'SEO from content projection', description: 'SEO description from projection.', canonicalPath: '/en', indexable: true },
    status: 'published', isPlaceholder: false, currentRevision: 3, publishedRevision: 2, scheduledFor: null,
    updatedAt: '2026-09-01T08:00:00Z',
    ...overrides,
  }
}

function bootstrap(overrides: Record<string, unknown> = {}) {
  return {
    generalInformation: {
      id: 'f4abdf14-c27b-4d71-a22b-45cc3f7aa64d', locale: 'en', status: 'published',
      payload: {
        brandName: 'Database Brand', brandLine: 'Database brand line', homePath: '/en',
        footerStatement: 'Database footer statement', copyrightText: 'Copyright {year}',
        defaultSeo: { title: 'Database default title', description: 'Database default description.' },
        organization: { name: 'Database Organization', url: 'https://www.example.test/en' },
        navigationCta: { label: 'Database CTA', href: '/en/request-a-quote' },
      },
      currentRevision: 2, publishedRevision: 1, isPlaceholder: false, updatedAt: '2026-09-01T08:00:00Z',
    },
    navigation: content({
      kind: 'navigation', slug: 'primary-navigation', title: 'Navigation',
      body: { schemaVersion: 1, doc: { type: 'doc', attrs: { items: [{ label: 'Database Products', href: '/en/products' }] }, content: [] } },
    }),
    footer: content({
      kind: 'footer', slug: 'global-footer', title: 'Footer',
      body: { schemaVersion: 1, doc: { type: 'doc', attrs: {
        columns: [{ title: 'Database column', links: [{ label: 'Database link', href: '/en/company/about' }] }],
        legalLinks: [{ label: 'Database privacy', href: '/en/privacy' }],
      }, content: [] } },
    }),
    productFamilies: [{ code: 'axial', slug: 'axial', name: 'Database axial family', description: 'Database family description.', sortOrder: 1 }],
    motorTechnologies: ['EC'],
    generatedAt: '2026-09-01T08:00:00Z',
    ...overrides,
  }
}

function route(page: unknown, overrides: Record<string, unknown> = {}) {
  return {
    path: '/en', templateKey: 'home', entityType: 'content', entityId: '792406a9-2f19-425e-8508-78a205c0c764',
    locale: 'en', publishedRevision: 2, indexable: true, dataClass: 'editorial', page,
    ...overrides,
  }
}

describe('database-driven public SSR page loading', () => {
  it('builds shell, SEO, taxonomy, sections and CTA only from published projections', async () => {
    const home = content({
      body: { schemaVersion: 1, doc: { type: 'doc', attrs: { pageSlots: {
        hero: { eyebrow: 'Database eyebrow', title: 'Database hero', description: 'Database hero description.' },
        primaryCta: { eyebrow: 'Database CTA eyebrow', title: 'Database CTA title', description: 'Database CTA description.', label: 'Database CTA label', href: '/en/request-a-quote' },
        sections: [{ id: 'database-section', title: 'Database section', description: 'Database section description.' }],
        relationships: [{ entityType: 'content', slug: 'about', title: 'Database related page', summary: 'Database relationship.', href: '/en/company/about' }],
      } }, content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Database body.' }] }] } },
    })
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => String(input).includes('/site-bootstrap?')
      ? json(bootstrap())
      : json(route(home))) as unknown as typeof fetch

    const result = await loadPublicPageData('/en', { baseUrl: 'http://api:8080/api/public/v1', fetchImpl })
    expect(result.site.brandName).toBe('Database Brand')
    expect(result.site.navigation).toEqual([{ label: 'Database Products', href: '/en/products' }])
    expect(result.page.title).toBe('Database hero')
    expect(result.page.metaTitle).toBe('SEO from content projection')
    expect(result.page.primaryCta?.label).toBe('Database CTA label')
    expect(result.page.sections?.[0]?.title).toBe('Database section')
    expect(result.page.entries?.[0]?.title).toBe('Database related page')
    expect(result.page.productFamilies?.[0]?.name).toBe('Database axial family')
    expect(result.page.motorTechnologies).toEqual(['EC'])
  })

  it('returns 404 for an unknown route and 503 for a missing core bootstrap', async () => {
    const unknownFetch = vi.fn(async (input: RequestInfo | URL) => String(input).includes('/site-bootstrap?')
      ? json(bootstrap())
      : json({ title: 'Not Found' }, 404)) as unknown as typeof fetch
    await expect(loadPublicPageData('/en/unknown', { baseUrl: 'http://api:8080/api/public/v1', fetchImpl: unknownFetch }))
      .rejects.toMatchObject({ status: 404 })

    const unavailableFetch = vi.fn(async (input: RequestInfo | URL) => String(input).includes('/site-bootstrap?')
      ? json(bootstrap({ generalInformation: null }))
      : json(route(content()))) as unknown as typeof fetch
    await expect(loadPublicPageData('/en', { baseUrl: 'http://api:8080/api/public/v1', fetchImpl: unavailableFetch }))
      .rejects.toMatchObject({ status: 503 })
  })

  it('SSR-renders a complete development bootstrap but forces its pages noindex', async () => {
    const base = bootstrap()
    const fixtureBootstrap = {
      ...base,
      generalInformation: { ...base.generalInformation, isPlaceholder: true },
      navigation: { ...base.navigation, isPlaceholder: true },
      footer: { ...base.footer, isPlaceholder: true },
    }
    const fixtureHome = content({ isPlaceholder: true, seo: { title: 'Fixture', description: 'Fixture.', canonicalPath: '/en', indexable: false } })
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => String(input).includes('/site-bootstrap?')
      ? json(fixtureBootstrap)
      : json(route(fixtureHome, { indexable: false, dataClass: 'developmentFixture' }))) as unknown as typeof fetch
    const result = await loadPublicPageData('/en', { baseUrl: 'http://api:8080/api/public/v1', fetchImpl })
    expect(result.site.isPlaceholder).toBe(true)
    expect(result.page.dataState).toBe('placeholder')
    expect(result.page.indexable).toBe(false)
  })

  it('shows published development News fixtures while keeping the fixture page noindex', async () => {
    const newsIndex = content({ kind: 'article', slug: 'index', title: 'News', isPlaceholder: true, seo: { title: 'News', description: 'News projection.', canonicalPath: '/en/resources/news', indexable: false } })
    const publishedNews = content({ kind: 'article', slug: 'database-update', title: 'Database update', summary: 'News from PostgreSQL.' })
    const placeholderNews = content({ id: crypto.randomUUID(), kind: 'article', slug: 'placeholder-update', title: 'Placeholder update', isPlaceholder: true })
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/site-bootstrap?')) return json(bootstrap())
      if (url.includes('/routes/resolve?')) return json(route(newsIndex, { path: '/en/resources/news', templateKey: 'news-index', dataClass: 'developmentFixture', indexable: false }))
      return json({ items: [
        { content: publishedNews, category: 'Company', authorDisplayName: 'Editorial', coverMediaId: null, publishedAt: '2026-09-01T08:00:00Z', featured: true, dataClass: 'editorial' },
        { content: placeholderNews, category: 'Development', authorDisplayName: null, coverMediaId: null, publishedAt: null, featured: false, dataClass: 'developmentFixture' },
      ], nextCursor: null })
    }) as unknown as typeof fetch

    const result = await loadPublicPageData('/en/resources/news', { baseUrl: 'http://api:8080/api/public/v1', fetchImpl })
    expect(result.page.kind).toBe('news')
    expect(result.page.entries).toHaveLength(2)
    expect(result.page.indexable).toBe(false)
    expect(result.page.entries?.[0]?.href).toBe('/en/resources/news/database-update')
  })

  it('does not render a missing product-detail fallback', async () => {
    const productPage = content({ kind: 'company', slug: 'product-detail', title: 'Product detail', seo: { title: 'Product detail', description: 'Product detail projection.', canonicalPath: '/en/products/axial/missing-product', indexable: true } })
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/site-bootstrap?')) return json(bootstrap())
      if (url.includes('/routes/resolve?')) return json(route(productPage, { path: '/en/products/axial/missing-product', templateKey: 'product-detail', entityType: 'product' }))
      return json({ title: 'Not Found' }, 404)
    }) as unknown as typeof fetch
    await expect(loadPublicPageData('/en/products/axial/missing-product', { baseUrl: 'http://api:8080/api/public/v1', fetchImpl }))
      .rejects.toMatchObject({ status: 404 })
  })

  it('uses the exact published product localization for product SEO', async () => {
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/site-bootstrap?')) return json(bootstrap())
      if (url.includes('/routes/resolve?')) return json(route(null, {
        path: '/en/products/axial/database-fan', templateKey: 'product-detail', entityType: 'product',
      }))
      return json({
        id: '21dab5e7-b0d7-48ce-ab97-ddf36f80dff4', stableId: 'DATABASE-FAN', model: 'DATABASE-FAN',
        slug: 'database-fan', locale: 'en', family: 'axial', subtype: null, motorTechnology: 'EC',
        title: 'Database fan', summary: 'Database product summary.',
        seo: { title: 'Published product SEO', description: 'Published product SEO description.', canonicalPath: '/en/products/axial/database-fan', indexable: true },
        sortOrder: 10, relatedContentIds: [], specifications: [], performanceCurves: [],
        sourceSnapshotId: '593e33f3-e334-4570-8991-9c6336108b20', sourceRevision: 'csv:1',
        currentRevision: 2, publishedRevision: 2, status: 'published', indexable: true,
        updatedAt: '2026-09-02T00:00:00Z',
      })
    }) as unknown as typeof fetch

    const result = await loadPublicPageData('/en/products/axial/database-fan', {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(result.page.metaTitle).toBe('Published product SEO')
    expect(result.page.description).toBe('Published product SEO description.')
    expect(result.page.publishedProduct?.relatedContentIds).toEqual([])
    const calls = (fetchImpl as unknown as ReturnType<typeof vi.fn>).mock.calls
    expect(calls.some(([input]) => String(input).endsWith('/products/database-fan?family=axial'))).toBe(true)
  })
})
