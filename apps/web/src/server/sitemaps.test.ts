import { describe, expect, it, vi } from 'vitest'
import { renderRobots, renderSitemapIndex, renderUrlSitemap } from '@/server/sitemaps'

const origin = 'https://www.example.test'

function discoveryResponse(entries: unknown[] = []): Response {
  return new Response(JSON.stringify({ generatedAt: '2026-09-01T00:00:00Z', entries }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  })
}

describe('published sitemap rendering', () => {
  it('keeps the sitemap index on the configured public origin', async () => {
    const response = await renderSitemapIndex(origin, {
      fetchImpl: vi.fn<typeof fetch>().mockResolvedValue(discoveryResponse()),
    })
    const xml = await response.text()
    expect(xml).toContain(`${origin}/sitemap-pages.xml`)
    expect(xml).not.toContain('admin.')
  })

  it('includes only valid published discovery paths', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({
      generatedAt: '2026-09-01T00:00:00Z',
      entries: [
        { entityType: 'content', entityId: crypto.randomUUID(), path: '/en/resources/articles/approved', locale: 'en', updatedAt: '2026-09-01T00:00:00Z' },
        { entityType: 'content', entityId: crypto.randomUUID(), path: '/en/search', locale: 'en', updatedAt: '2026-09-01T00:00:00Z' },
        { entityType: 'content', entityId: crypto.randomUUID(), path: '/admin/users', locale: 'en', updatedAt: '2026-09-01T00:00:00Z' },
        { entityType: 'content', entityId: crypto.randomUUID(), path: '/en/private/record', locale: 'en', updatedAt: '2026-09-01T00:00:00Z' },
        { entityType: 'content', entityId: crypto.randomUUID(), path: '/en/resources/articles/invalid-time', locale: 'en', updatedAt: 'not-a-date' },
        { entityType: 'content', entityId: crypto.randomUUID(), path: '/en/products/axial/wrong-entity-type', locale: 'en', updatedAt: '2026-09-01T00:00:00Z' },
        { entityType: 'content', entityId: crypto.randomUUID(), path: '/en/resources/articles/other-locale', locale: 'zh', updatedAt: '2026-09-01T00:00:00Z' },
      ],
    }), { status: 200 }))
    const response = await renderUrlSitemap('sitemap-resources.xml', {
      apiBaseUrl: 'http://api:8080/api/public/v1',
      canonicalOrigin: origin,
      fetchImpl,
    })
    const xml = await response.text()
    expect(xml).toContain(`${origin}/en/resources/articles/approved`)
    expect(xml).not.toContain('/en/search')
    expect(xml).not.toContain('/admin/users')
    expect(xml).not.toContain('/en/private/record')
    expect(xml).not.toContain('invalid-time')
    expect(xml).not.toContain('wrong-entity-type')
    expect(xml).not.toContain('other-locale')
  })

  it('fails closed when the discovery API is unavailable', async () => {
    const response = await renderUrlSitemap('sitemap-solutions.xml', {
      canonicalOrigin: origin,
      fetchImpl: vi.fn<typeof fetch>().mockRejectedValue(new Error('offline')),
    })
    expect(response.status).toBe(503)
    expect(response.headers.get('cache-control')).toContain('no-store')
    expect(response.headers.get('retry-after')).toBe('60')
    expect(response.headers.get('x-robots-tag')).toContain('noindex')
  })

  it('withholds the sitemap index when discovery is unavailable', async () => {
    const response = await renderSitemapIndex(origin, {
      fetchImpl: vi.fn<typeof fetch>().mockResolvedValue(new Response(null, { status: 503 })),
    })
    expect(response.status).toBe(503)
    expect(await response.text()).not.toContain('<sitemapindex')
  })

  it('emits only product and tool URLs explicitly present in discovery', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({
      generatedAt: '2026-09-01T00:00:00Z',
      entries: [
        { entityType: 'product', entityId: crypto.randomUUID(), path: '/en/products/axial/verified-model', locale: 'en', updatedAt: '2026-09-01T00:00:00Z' },
        { entityType: 'content', entityId: crypto.randomUUID(), path: '/en/products/selector', locale: 'en', updatedAt: '2026-09-01T00:00:00Z' },
        { entityType: 'product', entityId: crypto.randomUUID(), path: '/en/products/not-a-family/unsafe', locale: 'en', updatedAt: '2026-09-01T00:00:00Z' },
      ],
    }), { status: 200 }))
    const xml = await (await renderUrlSitemap('sitemap-products.xml', {
      canonicalOrigin: origin,
      fetchImpl,
    })).text()
    expect(xml).toContain(`${origin}/en/products/axial/verified-model`)
    expect(xml).toContain(`${origin}/en/products/selector`)
    expect(xml).not.toContain(`${origin}/en/products/axial</loc>`)
    expect(xml).not.toContain(`${origin}/en/products</loc>`)
    expect(xml).not.toContain('not-a-family')
  })

  it('does not add selector or RFQ routes when discovery is empty', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({
      generatedAt: '2026-09-01T00:00:00Z', entries: [],
    }), { status: 200 }))
    expect(await (await renderUrlSitemap('sitemap-products.xml', { canonicalOrigin: origin, fetchImpl })).text()).not.toContain('/en/products/selector')
    expect(await (await renderUrlSitemap('sitemap-pages.xml', { canonicalOrigin: origin, fetchImpl })).text()).not.toContain('/en/request-a-quote')
  })

  it('generates robots from the same canonical origin', async () => {
    const response = await renderRobots(origin, {
      fetchImpl: vi.fn<typeof fetch>().mockResolvedValue(discoveryResponse()),
    })
    expect(await response.text()).toContain(`Sitemap: ${origin}/sitemap.xml`)
  })

  it('disallows crawling and omits sitemap advertising while discovery is unavailable', async () => {
    const response = await renderRobots(origin, {
      fetchImpl: vi.fn<typeof fetch>().mockRejectedValue(new Error('offline')),
    })
    const body = await response.text()
    expect(body).toContain('Disallow: /')
    expect(body).not.toContain('Sitemap:')
  })
})
