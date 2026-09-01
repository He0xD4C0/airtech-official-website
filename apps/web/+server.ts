import vike from 'vike/fetch'
import type { Server } from 'vike/types'
import { isSitemapName, renderRobots, renderSitemapIndex, renderUrlSitemap } from '@/lib/server/sitemaps'

// Vike's universal middleware type includes optional adapter context arguments,
// while the built-in server invokes the Fetch API shape used here.
const renderVike = vike.fetch as unknown as (request: Request) => Promise<Response>

function publicApiBrowserOrigin(): string {
  const configured = process.env.PUBLIC_API_BROWSER_ORIGIN || 'http://localhost:8080'
  try {
    const url = new URL(configured)
    if (url.protocol === 'http:' || url.protocol === 'https:') return url.origin
  } catch {
    // Fail closed below when deployment configuration is invalid.
  }
  return ''
}

export function withPublicSecurityHeaders(response: Response, pathname = ''): Response {
  const headers = new Headers(response.headers)
  headers.set('X-Content-Type-Options', 'nosniff')
  headers.set('Referrer-Policy', 'strict-origin-when-cross-origin')
  headers.set('Permissions-Policy', 'camera=(), geolocation=(), microphone=()')
  headers.set(
    'Content-Security-Policy',
    `default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; img-src 'self' data: blob: https:; font-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; connect-src 'self' ${publicApiBrowserOrigin()}; form-action 'self'`,
  )
  if (pathname === '/en/preview' || pathname === '/en/preview/') {
    headers.set('Cache-Control', 'private, no-store, max-age=0')
    headers.set('Pragma', 'no-cache')
    headers.set('X-Robots-Tag', 'noindex, nofollow, noarchive')
    headers.set('Referrer-Policy', 'no-referrer')
  }
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  })
}

export default {
  async fetch(request: Request) {
    const url = new URL(request.url)
    if (url.pathname === '/') {
      return withPublicSecurityHeaders(new Response(null, {
        status: 308,
        headers: { Location: '/en' },
      }), url.pathname)
    }
    if (url.pathname === '/admin' || url.pathname.startsWith('/admin/')) {
      return withPublicSecurityHeaders(new Response('Not Found', {
        status: 404,
        headers: { 'Content-Type': 'text/plain; charset=utf-8' },
      }), url.pathname)
    }
    if (url.pathname === '/robots.txt') {
      return withPublicSecurityHeaders(renderRobots(), url.pathname)
    }
    if (url.pathname === '/sitemap.xml') {
      return withPublicSecurityHeaders(renderSitemapIndex(), url.pathname)
    }
    const sitemapName = url.pathname.slice(1)
    if (isSitemapName(sitemapName)) {
      return withPublicSecurityHeaders(await renderUrlSitemap(sitemapName), url.pathname)
    }
    return withPublicSecurityHeaders(await renderVike(request), url.pathname)
  },
} satisfies Server
