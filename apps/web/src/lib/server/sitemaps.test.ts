import { describe, expect, it, vi } from 'vitest'
import { renderRobots, renderSitemapIndex, renderUrlSitemap } from './sitemaps'

const origin = 'https://www.example.test'

describe('published sitemap rendering', () => {
  it('keeps the sitemap index on the configured public origin', async () => {
    const response = renderSitemapIndex(origin)
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
    const xml = await response.text()
    expect(xml).toContain('<urlset')
    expect(xml).not.toContain('<url>')
  })

  it('exposes catalog parents only when a valid published product is discoverable', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify({
      generatedAt: '2026-09-01T00:00:00Z',
      entries: [
        { entityType: 'product', entityId: crypto.randomUUID(), path: '/en/products/axial/verified-model', locale: 'en', updatedAt: '2026-09-01T00:00:00Z' },
        { entityType: 'product', entityId: crypto.randomUUID(), path: '/en/products/not-a-family/unsafe', locale: 'en', updatedAt: '2026-09-01T00:00:00Z' },
      ],
    }), { status: 200 }))
    const xml = await (await renderUrlSitemap('sitemap-products.xml', {
      canonicalOrigin: origin,
      fetchImpl,
    })).text()
    expect(xml).toContain(`${origin}/en/products/axial/verified-model`)
    expect(xml).toContain(`${origin}/en/products/axial`)
    expect(xml).toContain(`${origin}/en/products</loc>`)
    expect(xml).toContain(`${origin}/en/products/selector`)
    expect(xml).not.toContain('not-a-family')
  })

  it('generates robots from the same canonical origin', async () => {
    expect(await renderRobots(origin).text()).toContain(`Sitemap: ${origin}/sitemap.xml`)
  })
})
