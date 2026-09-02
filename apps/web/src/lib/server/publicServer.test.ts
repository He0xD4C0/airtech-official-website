import { describe, expect, it } from 'vitest'
import publicServer, { withPublicSecurityHeaders } from '../../../+server'

describe('public server infrastructure routes', () => {
  it('uses a permanent 308 locale redirect with public security headers', async () => {
    const response = await publicServer.fetch(new Request('http://localhost:3000/'))
    expect(response.status).toBe(308)
    expect(response.headers.get('location')).toBe('/en')
    expect(response.headers.get('x-content-type-options')).toBe('nosniff')
    expect(response.headers.get('content-security-policy')).toContain("frame-ancestors 'none'")
  })

  it('does not proxy the isolated Admin namespace', async () => {
    for (const path of ['/admin', '/admin/', '/admin/users']) {
      const response = await publicServer.fetch(new Request(`http://localhost:3000${path}`))
      expect(response.status, path).toBe(404)
    }
  })

  it('owns public robots and sitemap responses', async () => {
    const robots = await publicServer.fetch(new Request('http://localhost:3000/robots.txt'))
    expect(robots.status).toBe(200)
    expect(robots.headers.get('content-type')).toContain('text/plain')
    expect(await robots.text()).toContain('Sitemap: http://localhost:3000/sitemap.xml')

    const sitemap = await publicServer.fetch(new Request('http://localhost:3000/sitemap.xml'))
    expect(sitemap.status).toBe(200)
    expect(sitemap.headers.get('content-type')).toContain('application/xml')
    expect(await sitemap.text()).not.toContain('admin.')
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
