import type { PublicDiscoveryDocument, PublicDiscoveryEntry } from '@airtek/contracts'
import { isCanonicalDiscoveryPath } from '@/lib/publicPaths'
import { normalizePublicOrigin } from '@/lib/publicOrigin'

export type SitemapName = 'sitemap-pages.xml' | 'sitemap-products.xml' | 'sitemap-solutions.xml' | 'sitemap-resources.xml'

interface SitemapUrl {
  path: string
  updatedAt?: string
}

interface SitemapOptions {
  apiBaseUrl?: string
  canonicalOrigin?: string
  fetchImpl?: typeof fetch
}

const sitemapNames: SitemapName[] = [
  'sitemap-pages.xml',
  'sitemap-products.xml',
  'sitemap-solutions.xml',
  'sitemap-resources.xml',
]

// These routes are release-controlled application pages rather than CMS
// records. Everything else must arrive through the published discovery feed.
const releaseControlled: Record<SitemapName, SitemapUrl[]> = {
  'sitemap-pages.xml': [{ path: '/en/request-a-quote' }],
  'sitemap-products.xml': [{ path: '/en/products/selector' }],
  'sitemap-solutions.xml': [],
  'sitemap-resources.xml': [],
}

const excludedPaths = new Set([
  '/en/search',
  '/en/products/compare',
  '/en/request-a-quote/product',
  '/en/request-a-quote/selection',
  '/en/request-a-quote/project',
  '/en/request-a-quote/replacement',
])

function internalApiBaseUrl(): string {
  return process.env.PUBLIC_API_INTERNAL_URL || 'http://localhost:8080/api/public/v1'
}

function publicOrigin(): string {
  return process.env.PUBLIC_ORIGIN || import.meta.env.VITE_PUBLIC_ORIGIN || 'http://localhost:3000'
}

function escapeXml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;')
}

function validDiscoveryEntry(value: unknown): value is PublicDiscoveryEntry {
  if (!value || typeof value !== 'object') return false
  const entry = value as Partial<PublicDiscoveryEntry>
  if ((entry.entityType !== 'content' && entry.entityType !== 'product') || typeof entry.entityId !== 'string') return false
  if (entry.locale !== 'en' || typeof entry.path !== 'string' || typeof entry.updatedAt !== 'string') return false
  if (excludedPaths.has(entry.path) || !Number.isFinite(Date.parse(entry.updatedAt))) return false
  return isCanonicalDiscoveryPath(entry.entityType, entry.path)
}

function sitemapFor(path: string): SitemapName {
  if (path === '/en/products' || path.startsWith('/en/products/')) return 'sitemap-products.xml'
  if (path === '/en/solutions' || path.startsWith('/en/solutions/')) return 'sitemap-solutions.xml'
  if (path === '/en/resources' || path.startsWith('/en/resources/')) return 'sitemap-resources.xml'
  return 'sitemap-pages.xml'
}

function upsertLatest(urls: Map<string, SitemapUrl>, candidate: SitemapUrl): void {
  const existing = urls.get(candidate.path)
  const candidateTime = candidate.updatedAt ? Date.parse(candidate.updatedAt) : Number.NaN
  const existingTime = existing?.updatedAt ? Date.parse(existing.updatedAt) : Number.NaN
  if (!existing || (Number.isFinite(candidateTime) && (!Number.isFinite(existingTime) || candidateTime > existingTime))) {
    urls.set(candidate.path, candidate)
  }
}

async function discovery(options: SitemapOptions): Promise<PublicDiscoveryEntry[]> {
  const fetchImpl = options.fetchImpl ?? fetch
  const apiBaseUrl = (options.apiBaseUrl ?? internalApiBaseUrl()).replace(/\/$/, '')
  try {
    const response = await fetchImpl(`${apiBaseUrl}/discovery`, {
      headers: { Accept: 'application/json' },
      signal: AbortSignal.timeout(3_000),
    })
    if (!response.ok) return []
    const body = await response.json() as Partial<PublicDiscoveryDocument>
    return Array.isArray(body.entries) ? body.entries.filter(validDiscoveryEntry) : []
  } catch {
    return []
  }
}

function xmlResponse(body: string): Response {
  return new Response(body, {
    status: 200,
    headers: {
      'Content-Type': 'application/xml; charset=utf-8',
      'Cache-Control': 'public, max-age=0, s-maxage=300, stale-while-revalidate=60',
    },
  })
}

export function renderSitemapIndex(originValue = publicOrigin()): Response {
  const origin = normalizePublicOrigin(originValue)
  const items = sitemapNames.map((name) => `  <sitemap><loc>${escapeXml(`${origin}/${name}`)}</loc></sitemap>`).join('\n')
  return xmlResponse(`<?xml version="1.0" encoding="UTF-8"?>\n<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${items}\n</sitemapindex>\n`)
}

export async function renderUrlSitemap(name: SitemapName, options: SitemapOptions = {}): Promise<Response> {
  const origin = normalizePublicOrigin(options.canonicalOrigin ?? publicOrigin())
  const discovered = await discovery(options)
  const urls = new Map<string, SitemapUrl>()
  for (const entry of releaseControlled[name]) upsertLatest(urls, entry)
  for (const entry of discovered) {
    if (sitemapFor(entry.path) !== name) continue
    upsertLatest(urls, { path: entry.path, updatedAt: entry.updatedAt })
    if (name === 'sitemap-products.xml' && entry.entityType === 'product') {
      const familyPath = entry.path.split('/').slice(0, 4).join('/')
      upsertLatest(urls, { path: '/en/products', updatedAt: entry.updatedAt })
      upsertLatest(urls, { path: familyPath, updatedAt: entry.updatedAt })
    }
  }
  const items = [...urls.values()]
    .sort((left, right) => left.path.localeCompare(right.path))
    .map((entry) => {
      const lastmod = entry.updatedAt && !Number.isNaN(Date.parse(entry.updatedAt))
        ? `<lastmod>${escapeXml(new Date(entry.updatedAt).toISOString())}</lastmod>`
        : ''
      return `  <url><loc>${escapeXml(`${origin}${entry.path}`)}</loc>${lastmod}</url>`
    })
    .join('\n')
  return xmlResponse(`<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${items}\n</urlset>\n`)
}

export function renderRobots(originValue = publicOrigin()): Response {
  const origin = normalizePublicOrigin(originValue)
  return new Response([
    'User-agent: *',
    'Allow: /',
    'Disallow: /en/search',
    'Disallow: /en/products/compare',
    'Disallow: /en/request-a-quote/product',
    'Disallow: /en/request-a-quote/selection',
    'Disallow: /en/request-a-quote/project',
    'Disallow: /en/request-a-quote/replacement',
    '',
    `Sitemap: ${origin}/sitemap.xml`,
    '',
  ].join('\n'), {
    headers: {
      'Content-Type': 'text/plain; charset=utf-8',
      'Cache-Control': 'public, max-age=0, s-maxage=300, stale-while-revalidate=60',
    },
  })
}

export function isSitemapName(value: string): value is SitemapName {
  return sitemapNames.includes(value as SitemapName)
}
