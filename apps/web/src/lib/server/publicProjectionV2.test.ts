import { describe, expect, it } from 'vitest'
import type { RouteProjectionResponse, SiteBootstrapResponse } from '@/lib/publicApiTypes'
import { routeProjectionResponse } from '@/lib/publicApiDecoders'
import { pageFromRoute } from './publicRouteModel'
import { parseSiteBootstrap } from './publicProjectionParsing'
import { articleMetadataFrom } from './publicProjectionV2'
import type { PublicContentProjection } from '@/types/projection'

const projectionId = '11111111-1111-4111-8111-111111111111'
const relationId = '22222222-2222-4222-8222-222222222222'

function projection(overrides: Partial<PublicContentProjection> = {}): PublicContentProjection {
  return {
    schemaVersion: 2,
    id: projectionId,
    kind: 'article',
    templateKey: 'articleDetail',
    locale: 'en',
    title: 'Article title',
    summary: 'Summary copy',
    slug: 'article-title',
    seo: { title: 'SEO title', description: 'SEO description', indexable: true, socialImage: null },
    body: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Body copy' }] }] },
    composition: {
      blocks: [
        {
          type: 'hero', id: 'hero', eyebrow: 'Newsroom', heading: 'Hero title', lead: 'Hero lead',
          media: null, actions: [], variant: 'standard',
        },
        { type: 'body', id: 'body', width: 'standard' },
        {
          type: 'featureGrid', id: 'grid', heading: 'Capabilities',
          items: [{ id: 'item-1', title: 'Airflow', description: 'Feature copy', icon: null }],
        },
        {
          type: 'cta', id: 'cta', eyebrow: null, heading: 'Contact us', body: 'CTA body',
          action: { label: 'Talk to us', target: { targetType: 'route', path: '/en/company/contact' } },
          variant: 'standard',
        },
        { type: 'relationCollection', id: 'relations', heading: 'Related', relationIds: [relationId], presentation: 'cards' },
      ],
    },
    typeFields: {
      type: 'article', category: 'Company', authorDisplayName: 'AIRTEKPOWER',
      publicationAt: '2026-09-01T00:00:00Z', cover: null, featured: false,
    },
    isPlaceholder: false,
    publishedRevision: 4,
    updatedAt: '2026-09-10T00:00:00Z',
    resolvedRelations: [{
      relationId,
      entityType: 'content',
      title: 'Related article',
      summary: 'Related summary',
      href: '/en/resources/articles/related',
      eyebrow: 'Article',
      tags: ['airflow'],
    }],
    resolvedLinks: [],
    resolvedMedia: [],
    ...overrides,
  }
}

function route(page: PublicContentProjection): RouteProjectionResponse {
  return {
    path: '/en/resources/articles/article-title',
    templateKey: page.templateKey,
    entityType: 'content',
    entityId: page.id,
    locale: 'en',
    publishedRevision: page.publishedRevision,
    indexable: true,
    dataClass: 'editorial',
    page,
  }
}

const site = {
  brandName: 'AIRTEKPOWER',
  homePath: '/en',
  defaultSeo: {},
  organization: { name: 'AIRTEKPOWER' },
  navigation: [],
  footerColumns: [],
  legalLinks: [],
  productFamilies: [],
  motorTechnologies: [],
  generatedAt: '2026-09-10T00:00:00Z',
  publishedRevision: 1,
  isPlaceholder: false,
}

describe('native V2 public projection', () => {
  it('builds the page model from V2 composition blocks', () => {
    const page = pageFromRoute(route(projection()), site)
    expect(page.kind).toBe('detail')
    expect(page.title).toBe('Hero title')
    expect(page.description).toBe('Hero lead')
    expect(page.eyebrow).toBe('Newsroom')
    expect(page.blocks?.length).toBe(5)
    expect(page.sections?.[0]).toMatchObject({ eyebrow: 'Capabilities', title: 'Airflow' })
    expect(page.primaryCta).toMatchObject({ title: 'Contact us', href: '/en/company/contact' })
    expect(page.entries?.[0]).toMatchObject({ title: 'Related article', href: '/en/resources/articles/related' })
    expect(page.indexable).toBe(true)
    expect(page.projection?.publishedRevision).toBe(4)
  })

  it('marks V2 placeholders as noindex placeholders', () => {
    const page = pageFromRoute(route(projection({ isPlaceholder: true, seo: { title: null, description: null, indexable: false, socialImage: null } })), site)
    expect(page.indexable).toBe(false)
    expect(page.dataState).toBe('placeholder')
  })

  it('rejects legacy-shaped pages at the public API boundary', () => {
    const legacyPage = {
      id: projectionId,
      kind: 'article',
      slug: 'legacy-article',
      locale: 'en',
      title: 'Legacy article',
      summary: 'Legacy summary',
      body: { schemaVersion: 1, doc: { type: 'doc', attrs: { pageSlots: { hero: { title: 'Legacy hero' } } }, content: [] } },
      seo: { title: null, description: null, canonicalPath: '/en/resources/articles/legacy-article', indexable: true },
      status: 'published',
      currentRevision: 1,
      publishedRevision: 1,
      scheduledFor: null,
      isPlaceholder: false,
      updatedAt: '2026-09-01T00:00:00Z',
    }
    expect(() => routeProjectionResponse({
      ...route(projection()),
      page: legacyPage,
    })).toThrow(/CMS V2 public projection/i)
  })

  it('rejects legacy lifecycle fields even when the rest of the page is V2', () => {
    const native = projection()
    expect(() => routeProjectionResponse({
      ...route(native),
      page: { ...native, status: 'published', currentRevision: native.publishedRevision },
    })).toThrow(/CMS V2 public projection/i)
  })

  it('requires a slug only when the route template needs one', () => {
    const detail = projection({ slug: null })
    expect(() => pageFromRoute(route(detail), site)).toThrow(/no matching CMS V2 slug/i)

    const index = projection({
      kind: 'page',
      templateKey: 'articleIndex',
      slug: null,
      typeFields: { type: 'page' },
    })
    const indexPage = pageFromRoute({
      ...route(index),
      path: '/en/resources/articles',
    }, site)
    expect(indexPage.slug).toBeUndefined()
  })

  it('derives article outlines directly from the native Tiptap body', () => {
    const metadata = articleMetadataFrom(projection({
      body: {
        type: 'doc',
        content: [
          { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Evidence' }] },
          { type: 'heading', attrs: { level: 3 }, content: [{ type: 'text', text: 'Evidence' }] },
        ],
      },
    }))
    expect(metadata.outline).toEqual([
      { id: 'evidence', title: 'Evidence', level: 2 },
      { id: 'evidence-2', title: 'Evidence', level: 3 },
    ])
  })

  it('parses the V2 site bootstrap from singleton projections', () => {
    const information = projection({
      kind: 'generalInformation',
      templateKey: 'generalInformation',
      slug: null,
      typeFields: {
        type: 'generalInformation',
        organizationName: 'AIRTEKPOWER',
        brandLine: 'Smart airflow',
        homePath: '/en',
        footerStatement: 'Footer statement',
        copyrightTemplate: '© AIRTEKPOWER',
        contact: { email: null, phone: null, addressLines: [], locality: null, region: null, postalCode: null, countryCode: null },
        socialLinks: [{ service: 'linkedin', url: 'https://example.com/airtek' }],
        defaultSeo: { title: 'Default title', description: 'Default description', indexable: false, socialImage: null },
        productCategories: [],
        navigationCta: { label: 'Request a quote', target: { targetType: 'route', path: '/en/request-a-quote' } },
      },
    })
    const navigation = projection({
      kind: 'navigation',
      templateKey: 'navigation',
      slug: null,
      typeFields: {
        type: 'navigation',
        items: [{ id: 'nav-1', label: 'Products', target: { targetType: 'route', path: '/en/products' }, children: [] }],
      },
    })
    const footer = projection({
      kind: 'footer',
      templateKey: 'footer',
      slug: null,
      typeFields: {
        type: 'footer',
        columns: [{ id: 'column-1', title: 'Explore', links: [{ id: 'link-1', label: 'Products', target: { targetType: 'route', path: '/en/products' }, children: [] }] }],
        legalLinks: [{ id: 'legal-1', label: 'Privacy', target: { targetType: 'route', path: '/en/privacy' }, children: [] }],
      },
    })
    const bootstrap = parseSiteBootstrap({
      generalInformation: information,
      navigation,
      footer,
      productFamilies: [],
      motorTechnologies: [],
      generatedAt: '2026-09-10T00:00:00Z',
    } satisfies SiteBootstrapResponse)
    expect(bootstrap.brandName).toBe('AIRTEKPOWER')
    expect(bootstrap.navigation).toEqual([{ label: 'Products', href: '/en/products' }])
    expect(bootstrap.footerColumns).toEqual([{ title: 'Explore', links: [{ label: 'Products', href: '/en/products' }] }])
    expect(bootstrap.legalLinks).toEqual([{ label: 'Privacy', href: '/en/privacy' }])
    expect(bootstrap.navigationCta).toEqual({ label: 'Request a quote', href: '/en/request-a-quote' })
  })
})
