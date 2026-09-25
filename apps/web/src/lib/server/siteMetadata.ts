import { createPublicApiClient } from '@/lib/publicApiClient'
import { parseSiteBootstrap } from './publicProjectionParsing'

interface SiteIcon {
  url: string
  mediaType: string
  width: number
  height: number
}

interface SiteMetadata {
  brandName: string
  siteIcon?: SiteIcon
}

interface SiteMetadataOptions {
  apiBaseUrl?: string
  fetchImpl?: typeof fetch
}

const FALLBACK_ICON = '/site-icon-placeholder.svg'

function internalApiBaseUrl(): string {
  return process.env.PUBLIC_API_INTERNAL_URL || 'http://localhost:8080/api/public/v1'
}

async function loadSiteMetadata(options: SiteMetadataOptions = {}): Promise<SiteMetadata> {
  const fetchImpl: typeof fetch = options.fetchImpl ?? ((input, init) => fetch(input, {
    ...init,
    signal: AbortSignal.timeout(3_000),
  }))
  try {
    const client = createPublicApiClient({
      baseUrl: options.apiBaseUrl ?? internalApiBaseUrl(),
      fetchImpl,
    })
    const site = parseSiteBootstrap(await client.getSiteBootstrap('en'))
    return { brandName: site.brandName, siteIcon: site.siteIcon }
  } catch {
    return { brandName: 'AIRTEKPOWER' }
  }
}

export async function renderSiteIcon(options: SiteMetadataOptions = {}): Promise<Response> {
  const metadata = await loadSiteMetadata(options)
  return new Response(null, {
    status: 307,
    headers: {
      Location: metadata.siteIcon?.url ?? FALLBACK_ICON,
      'Cache-Control': 'public, max-age=300',
    },
  })
}

export async function renderWebManifest(options: SiteMetadataOptions = {}): Promise<Response> {
  const metadata = await loadSiteMetadata(options)
  const icon = metadata.siteIcon
    ? {
        src: '/site-icon',
        sizes: `${metadata.siteIcon.width}x${metadata.siteIcon.height}`,
        type: metadata.siteIcon.mediaType,
        purpose: 'any',
      }
    : { src: FALLBACK_ICON, sizes: 'any', type: 'image/svg+xml', purpose: 'any' }
  return new Response(JSON.stringify({
    name: metadata.brandName,
    short_name: metadata.brandName,
    start_url: '/en',
    display: 'standalone',
    background_color: '#ffffff',
    theme_color: '#0c7497',
    description: 'Industrial airflow product discovery and engineering resources.',
    icons: [icon],
  }), {
    headers: {
      'Content-Type': 'application/manifest+json; charset=utf-8',
      'Cache-Control': 'public, max-age=0, s-maxage=300',
    },
  })
}
