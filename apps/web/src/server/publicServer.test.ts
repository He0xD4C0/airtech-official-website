import { afterEach, describe, expect, it, vi } from 'vitest'
import publicServer, { withPublicSecurityHeaders } from '../../+server'

afterEach(() => vi.unstubAllGlobals())

describe('public server infrastructure routes', () => {
  it('keeps process liveness independent from missing CMS and API', async () => {
    const fetch = vi.fn().mockRejectedValue(new Error('offline'))
    vi.stubGlobal('fetch', fetch)
    const response = await publicServer.fetch(new Request('http://localhost:3000/healthz'))
    expect(response.status).toBe(200)
    expect(fetch).not.toHaveBeenCalled()
  })
  it('uses a permanent 308 locale redirect with public security headers', async () => {
    const response = await publicServer.fetch(new Request('http://localhost:3000/'))
    expect(response.status).toBe(308)
    expect(response.headers.get('location')).toBe('/en')
    expect(response.headers.get('x-content-type-options')).toBe('nosniff')
    expect(response.headers.get('content-security-policy')).toContain("frame-ancestors 'none'")
    expect(response.headers.get('content-security-policy')).toContain('http://media.localhost:19000')
  })

  it('does not proxy the isolated Admin namespace', async () => {
    for (const path of ['/admin', '/admin/', '/admin/users']) {
      const response = await publicServer.fetch(new Request(`http://localhost:3000${path}`))
      expect(response.status, path).toBe(404)
    }
  })

  it('owns public robots and sitemap responses', async () => {
    vi.stubGlobal('fetch', vi.fn<typeof fetch>().mockImplementation(async () => discoveryResponse()))
    const robots = await publicServer.fetch(new Request('http://localhost:3000/robots.txt'))
    expect(robots.status).toBe(200)
    expect(robots.headers.get('content-type')).toContain('text/plain')
    expect(await robots.text()).toContain('Sitemap: http://localhost:3000/sitemap.xml')

    const sitemap = await publicServer.fetch(new Request('http://localhost:3000/sitemap.xml'))
    expect(sitemap.status).toBe(200)
    expect(sitemap.headers.get('content-type')).toContain('application/xml')
    expect(await sitemap.text()).not.toContain('admin.')
  })

  it('fails closed when public discovery is unavailable', async () => {
    vi.stubGlobal('fetch', vi.fn<typeof fetch>().mockRejectedValue(new Error('offline')))
    const robots = await publicServer.fetch(new Request('http://localhost:3000/robots.txt'))
    expect(await robots.text()).toBe('User-agent: *\nDisallow: /\n')

    const sitemap = await publicServer.fetch(new Request('http://localhost:3000/sitemap.xml'))
    expect(sitemap.status).toBe(503)
  })

  it('forces every preview response and error to stay private and unindexable', () => {
    for (const status of [200, 404, 410]) {
      const response = withPublicSecurityHeaders(new Response('preview', { status }), '/en/preview')
      expect(response.headers.get('cache-control')).toBe('private, no-store, max-age=0')
      expect(response.headers.get('pragma')).toBe('no-cache')
      expect(response.headers.get('x-robots-tag')).toBe('noindex, nofollow, noarchive')
      expect(response.headers.get('referrer-policy')).toBe('no-referrer')
    }
  })

  it('marks unavailable SSR projections as retryable, uncacheable and unindexable', () => {
    const response = withPublicSecurityHeaders(new Response('unavailable', { status: 503 }), '/en')
    expect(response.headers.get('cache-control')).toBe('no-store, max-age=0')
    expect(response.headers.get('retry-after')).toBe('60')
    expect(response.headers.get('x-robots-tag')).toBe('noindex, nofollow, noarchive')
  })

})

function discoveryResponse(): Response {
  return new Response(JSON.stringify({
    generatedAt: '2026-09-01T00:00:00Z',
    entries: [],
  }), { status: 200, headers: { 'Content-Type': 'application/json' } })
}
