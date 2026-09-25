import { describe, expect, it, vi } from 'vitest'
import { loadPublicPageData } from './publicPageData'

function json(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), { status, headers: { 'Content-Type': 'application/json' } })
}

function content(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    schemaVersion: 2,
    id: '792406a9-2f19-425e-8508-78a205c0c764', kind: 'home', templateKey: 'home', slug: null, locale: 'en',
    title: 'Title from content projection', summary: 'Summary from content projection.',
    body: { type: 'doc', content: [] },
    composition: { blocks: [
      { type: 'hero', id: 'hero', eyebrow: null, heading: 'Title from content projection', lead: 'Summary from content projection.', media: null, actions: [], variant: 'standard' },
    ] },
    typeFields: { type: 'home' },
    seo: { title: 'SEO from content projection', description: 'SEO description from projection.', indexable: true, socialImage: null },
    isPlaceholder: false, publishedRevision: 2,
    updatedAt: '2026-09-01T08:00:00Z',
    resolvedRelations: [], resolvedLinks: [], resolvedMedia: [],
    ...overrides,
  }
}

function bootstrap(overrides: Record<string, unknown> = {}) {
  return {
    generalInformation: content({
      id: 'f4abdf14-c27b-4d71-a22b-45cc3f7aa64d', kind: 'generalInformation',
      templateKey: 'generalInformation', slug: null, title: 'Database Brand', publishedRevision: 1,
      typeFields: {
        type: 'generalInformation', organizationName: 'Database Brand', brandLine: 'Database brand line',
        homePath: '/en', footerStatement: 'Database footer statement', copyrightTemplate: 'Copyright {year}',
        contact: { email: null, phone: null, addressLines: [], locality: null, region: null, postalCode: null, countryCode: null },
        socialLinks: [],
        defaultSeo: { title: 'Database default title', description: 'Database default description.', indexable: false, socialImage: null },
        productCategories: [],
        navigationCta: { label: 'Database CTA', target: { targetType: 'route', path: '/en/request-a-quote' } },
      },
    }),
    navigation: content({
      id: '22222222-2222-4222-8222-222222222222', kind: 'navigation', templateKey: 'navigation', slug: null, title: 'Navigation',
      typeFields: { type: 'navigation', items: [{ id: 'nav-1', label: 'Database Products', target: { targetType: 'route', path: '/en/products' }, children: [] }] },
    }),
    footer: content({
      id: '33333333-3333-4333-8333-333333333333', kind: 'footer', templateKey: 'footer', slug: null, title: 'Footer',
      typeFields: {
        type: 'footer',
        columns: [{ id: 'column-1', title: 'Database column', links: [{ id: 'footer-link', label: 'Database link', target: { targetType: 'route', path: '/en/company/about' }, children: [] }] }],
        legalLinks: [{ id: 'privacy-link', label: 'Database privacy', target: { targetType: 'route', path: '/en/privacy' }, children: [] }],
      },
    }),
    productFamilies: [{ code: 'axial', slug: 'axial', name: 'Database axial family', description: 'Database family description.', sortOrder: 1 }],
    motorTechnologies: ['EC'],
    generatedAt: '2026-09-01T08:00:00Z',
    ...overrides,
  }
}

function route(page: Record<string, unknown> | null, overrides: Record<string, unknown> = {}) {
  const entityId = typeof page?.id === 'string' ? page.id : '792406a9-2f19-425e-8508-78a205c0c764'
  const templateKey = typeof page?.templateKey === 'string' ? page.templateKey : 'productDetail'
  const publishedRevision = typeof page?.publishedRevision === 'number' ? page.publishedRevision : 2
  return {
    path: '/en', templateKey, entityType: page ? 'content' : 'product', entityId,
    locale: 'en', publishedRevision, indexable: true, dataClass: 'editorial', page,
    ...overrides,
  }
}

describe('database-driven public SSR page loading', () => {
  it('builds shell, SEO, taxonomy, sections and CTA only from published projections', async () => {
    const home = content({
      body: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Database body.' }] }] },
      composition: { blocks: [
        { type: 'hero', id: 'hero', eyebrow: 'Database eyebrow', heading: 'Database hero', lead: 'Database hero description.', media: null, actions: [], variant: 'standard' },
        { type: 'body', id: 'body', width: 'standard' },
        { type: 'featureGrid', id: 'database-section', heading: 'Database section', items: [{ id: 'feature', title: 'Database section', description: 'Database section description.', icon: null }] },
        { type: 'cta', id: 'cta', eyebrow: 'Database CTA eyebrow', heading: 'Database CTA title', body: 'Database CTA description.', action: { label: 'Database CTA label', target: { targetType: 'route', path: '/en/request-a-quote' } }, variant: 'standard' },
        { type: 'relationCollection', id: 'relations', heading: 'Related', relationIds: ['relation-1'], presentation: 'cards' },
      ] },
      resolvedRelations: [{
        relationId: 'relation-1', entityType: 'content', title: 'Database related page',
        summary: 'Database relationship.', href: '/en/company/about', eyebrow: null, tags: [],
      }],
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

    const simultaneousFailure = vi.fn(async (input: RequestInfo | URL) => String(input).includes('/site-bootstrap?')
      ? json({ title: 'Unavailable' }, 503)
      : json({ title: 'Not Found' }, 404)) as unknown as typeof fetch
    await expect(loadPublicPageData('/en/missing', {
      baseUrl: 'http://api:8080/api/public/v1',
      fetchImpl: simultaneousFailure,
    })).rejects.toMatchObject({ status: 503 })
  })

  it('SSR-renders a complete development bootstrap but forces its pages noindex', async () => {
    const base = bootstrap()
    const fixtureBootstrap = {
      ...base,
      generalInformation: { ...base.generalInformation, isPlaceholder: true },
      navigation: { ...base.navigation, isPlaceholder: true },
      footer: { ...base.footer, isPlaceholder: true },
    }
    const fixtureHome = content({ isPlaceholder: true, seo: { title: 'Fixture', description: 'Fixture.', indexable: false, socialImage: null } })
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => String(input).includes('/site-bootstrap?')
      ? json(fixtureBootstrap)
      : json(route(fixtureHome, { indexable: false, dataClass: 'developmentFixture' }))) as unknown as typeof fetch
    const result = await loadPublicPageData('/en', { baseUrl: 'http://api:8080/api/public/v1', fetchImpl })
    expect(result.site.isPlaceholder).toBe(true)
    expect(result.page.dataState).toBe('placeholder')
    expect(result.page.indexable).toBe(false)
  })

  it('shows published development News fixtures while keeping the fixture page noindex', async () => {
    const newsIndex = content({
      kind: 'page', templateKey: 'newsIndex', slug: null, typeFields: { type: 'page' },
      title: 'News', isPlaceholder: true,
      seo: { title: 'News', description: 'News projection.', indexable: false, socialImage: null },
    })
    const publishedNews = content({
      kind: 'news', templateKey: 'newsDetail', slug: 'database-update', title: 'Database update',
      summary: 'News from PostgreSQL.',
      typeFields: { type: 'news', category: 'Company', authorDisplayName: 'Editorial', cover: null, publicationAt: null, featured: true },
    })
    const placeholderNews = content({
      id: crypto.randomUUID(), kind: 'news', templateKey: 'newsDetail', slug: 'placeholder-update',
      title: 'Placeholder update', isPlaceholder: true,
      typeFields: { type: 'news', category: 'Development', authorDisplayName: null, cover: null, publicationAt: null, featured: false },
    })
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/site-bootstrap?')) return json(bootstrap())
      if (url.includes('/routes/resolve?')) return json(route(newsIndex, { path: '/en/resources/news', dataClass: 'developmentFixture', indexable: false }))
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

  it('loads a News detail only from its matching native V2 projection', async () => {
    const publishedNews = content({
      kind: 'news', templateKey: 'newsDetail', slug: 'database-update', title: 'Database update',
      summary: 'News from the native public projection.',
      composition: { blocks: [
        { type: 'hero', id: 'hero', eyebrow: 'Company', heading: 'Database update', lead: 'Native V2 News.', media: null, actions: [], variant: 'standard' },
        { type: 'body', id: 'body', width: 'standard' },
      ] },
      typeFields: {
        type: 'news', category: 'Company', authorDisplayName: 'Editorial', cover: null,
        publicationAt: '2026-09-01T08:00:00Z', featured: true,
      },
    })
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/site-bootstrap?')) return json(bootstrap())
      if (url.includes('/routes/resolve?')) {
        return json(route(publishedNews, { path: '/en/resources/news/database-update' }))
      }
      return json({
        content: publishedNews, category: 'Company', authorDisplayName: 'Editorial',
        coverMediaId: null, publishedAt: '2026-09-01T08:00:00Z', featured: true,
        dataClass: 'editorial',
      })
    }) as unknown as typeof fetch

    const result = await loadPublicPageData('/en/resources/news/database-update', {
      baseUrl: 'http://api:8080/api/public/v1', fetchImpl,
    })
    expect(result.page.kind).toBe('news-detail')
    expect(result.page.projection?.schemaVersion).toBe(2)
    expect(result.page.blocks?.map((block) => block.type)).toEqual(['hero', 'body'])
    expect(result.page.projection?.schemaVersion).toBe(2)
  })

  it('does not render a missing product-detail fallback', async () => {
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/site-bootstrap?')) return json(bootstrap())
      if (url.includes('/routes/resolve?')) return json(route(null, { path: '/en/products/axial/missing-product' }))
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
        path: '/en/products/axial/database-fan',
      }))
      if (url.includes('/products/database-fan/assets?')) return json({
        productId: '21dab5e7-b0d7-48ce-ab97-ddf36f80dff4',
        productRevision: 2,
        items: [],
      })
      return json({
        id: '21dab5e7-b0d7-48ce-ab97-ddf36f80dff4', stableId: 'DATABASE-FAN', model: 'DATABASE-FAN',
        slug: 'database-fan', locale: 'en', family: 'axial', subtype: null, motorTechnology: 'EC',
        title: 'Database fan', summary: 'Database product summary.',
        seo: { title: 'Published product SEO', description: 'Published product SEO description.', canonicalPath: '/en/products/axial/database-fan', indexable: true },
        sortOrder: 10, relatedContentIds: [], mediaGallery: [], specifications: [], performanceCurves: [],
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
    expect(result.page.productAssets).toEqual([])
    const calls = (fetchImpl as unknown as ReturnType<typeof vi.fn>).mock.calls
    expect(calls.some(([input]) => String(input).endsWith('/products/database-fan?family=axial'))).toBe(true)
    expect(calls.some(([input]) => String(input).endsWith('/products/database-fan/assets?family=axial'))).toBe(true)
  })

  it('serves the native V2 projection shape end to end', async () => {
    const v2Projection = {
      schemaVersion: 2,
      id: '3a1d7e5a-6f4b-4f0e-9d0a-2c1b3e4f5a6b',
      kind: 'home',
      templateKey: 'home',
      locale: 'en',
      title: 'V2 title',
      summary: 'V2 summary',
      slug: 'home',
      seo: { title: 'V2 SEO', description: 'V2 SEO description', indexable: true, socialImage: null },
      body: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'V2 body' }] }] },
      composition: { blocks: [
        { type: 'hero', id: 'hero', eyebrow: 'V2 eyebrow', heading: 'V2 hero', lead: 'V2 lead', media: null, actions: [], variant: 'standard' },
        { type: 'relationCollection', id: 'relations', heading: 'Related', relationIds: ['rel-1'], presentation: 'cards' },
      ] },
      typeFields: { type: 'home' },
      isPlaceholder: false,
      publishedRevision: 5,
      updatedAt: '2026-09-10T00:00:00Z',
      resolvedRelations: [{
        relationId: 'rel-1', entityType: 'content', title: 'V2 related', summary: 'Related summary',
        href: '/en/company/about', eyebrow: 'Company', tags: [],
      }],
      resolvedLinks: [],
      resolvedMedia: [],
    }
    const generalInformation = {
      ...v2Projection,
      id: '4b2e8f6b-7a5c-4a1f-8e2d-3c4d5e6f7a8b',
      kind: 'generalInformation',
      templateKey: 'generalInformation',
      slug: null,
      typeFields: {
        type: 'generalInformation', organizationName: 'V2 Brand', brandLine: null, homePath: '/en',
        footerStatement: null, copyrightTemplate: null,
        contact: { email: null, phone: null, addressLines: [], locality: null, region: null, postalCode: null, countryCode: null },
        socialLinks: [], defaultSeo: { title: null, description: null, indexable: false, socialImage: null },
        productCategories: [], navigationCta: null,
      },
    }
    const navigation = {
      ...v2Projection,
      id: '5c3f9a7c-8b6d-4b2a-9f3e-4d5e6f7a8b9c',
      kind: 'navigation',
      templateKey: 'navigation',
      slug: null,
      typeFields: {
        type: 'navigation',
        items: [{ id: 'n1', label: 'V2 products', target: { targetType: 'route', path: '/en/products' }, children: [] }],
      },
    }
    const footer = {
      ...v2Projection,
      id: '6d4a0b8d-9c7e-4c3b-8a4f-5e6f7a8b9c0d',
      kind: 'footer',
      templateKey: 'footer',
      slug: null,
      typeFields: { type: 'footer', columns: [], legalLinks: [] },
    }
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => String(input).includes('/site-bootstrap?')
      ? json({ generalInformation, navigation, footer, productFamilies: [], motorTechnologies: [], generatedAt: '2026-09-10T00:00:00Z' })
      : json(route(v2Projection))) as unknown as typeof fetch

    const result = await loadPublicPageData('/en', { baseUrl: 'http://api:8080/api/public/v1', fetchImpl })
    expect(result.site.brandName).toBe('V2 Brand')
    expect(result.site.navigation).toEqual([{ label: 'V2 products', href: '/en/products' }])
    expect(result.page.title).toBe('V2 hero')
    expect(result.page.blocks?.length).toBe(2)
    expect(result.page.entries?.[0]?.title).toBe('V2 related')
    expect(result.page.projection?.publishedRevision).toBe(5)
  })
})
