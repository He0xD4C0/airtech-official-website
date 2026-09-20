interface SiteAssetOptions {
  apiBaseUrl?: string
  fetchImpl?: typeof fetch
}

interface PublishedSiteIcon {
  publicUrl: string
  mediaType: string
  width: number
  height: number
}

interface SiteIdentity {
  organizationName: string
  icon: PublishedSiteIcon | null
}

const neutralIcon = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img" aria-label="Site icon placeholder"><rect width="64" height="64" rx="12" fill="#eef1f2"/><circle cx="32" cy="32" r="17" fill="none" stroke="#526169" stroke-width="5"/><circle cx="32" cy="32" r="5" fill="#526169"/></svg>`

function internalApiBaseUrl(): string {
  return process.env.PUBLIC_API_INTERNAL_URL || 'http://localhost:8080/api/public/v1'
}

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
}

function parseIdentity(value: unknown): SiteIdentity | null {
  const bootstrap = record(value)
  const information = record(bootstrap?.generalInformation)
  const typeFields = record(information?.typeFields)
  if (typeFields?.type !== 'generalInformation') return null
  const organizationName = typeof typeFields.organizationName === 'string' && typeFields.organizationName.trim()
    ? typeFields.organizationName.trim()
    : 'AIRTEKPOWER'
  const siteIcon = record(typeFields.siteIcon)
  if (!siteIcon || typeof siteIcon.assetId !== 'string') return { organizationName, icon: null }
  const resolved = Array.isArray(information?.resolvedMedia)
    ? information.resolvedMedia.map(record).find((item) => item?.assetId === siteIcon.assetId)
    : undefined
  const width = resolved?.originalWidth
  const height = resolved?.originalHeight
  if (!resolved
    || typeof resolved.publicUrl !== 'string'
    || !/^(?:https?:\/\/|\/)/u.test(resolved.publicUrl)
    || !['image/png', 'image/jpeg', 'image/webp'].includes(String(resolved.mediaType))
    || !Number.isInteger(width) || !Number.isInteger(height)
    || Number(width) < 512 || width !== height) {
    return { organizationName, icon: null }
  }
  return {
    organizationName,
    icon: {
      publicUrl: resolved.publicUrl,
      mediaType: String(resolved.mediaType),
      width: Number(width),
      height: Number(height),
    },
  }
}

async function loadIdentity(options: SiteAssetOptions): Promise<SiteIdentity | null> {
  const fetchImpl = options.fetchImpl ?? fetch
  const baseUrl = (options.apiBaseUrl ?? internalApiBaseUrl()).replace(/\/$/u, '')
  try {
    const response = await fetchImpl(`${baseUrl}/site-bootstrap?locale=en`, {
      headers: { Accept: 'application/json' },
      signal: AbortSignal.timeout(3_000),
    })
    if (!response.ok) return null
    return parseIdentity(await response.json())
  } catch {
    return null
  }
}

function placeholderIconResponse(): Response {
  return new Response(neutralIcon, {
    headers: {
      'Content-Type': 'image/svg+xml; charset=utf-8',
      'Cache-Control': 'public, max-age=0, s-maxage=60',
    },
  })
}

export async function renderSiteIcon(options: SiteAssetOptions = {}): Promise<Response> {
  const identity = await loadIdentity(options)
  if (!identity?.icon) return placeholderIconResponse()
  return new Response(null, {
    status: 308,
    headers: {
      Location: identity.icon.publicUrl,
      'Cache-Control': 'public, max-age=0, s-maxage=300',
    },
  })
}

export async function renderWebManifest(options: SiteAssetOptions = {}): Promise<Response> {
  const identity = await loadIdentity(options)
  const icon = identity?.icon
  const name = identity?.organizationName ?? 'AIRTEKPOWER'
  const body = {
    name,
    short_name: name.slice(0, 40),
    start_url: '/en',
    display: 'standalone',
    background_color: '#ffffff',
    theme_color: '#0c7497',
    description: 'Industrial airflow product discovery and engineering resources.',
    icons: [icon ? {
      src: '/site-icon', type: icon.mediaType, sizes: `${icon.width}x${icon.height}`,
    } : { src: '/site-icon', type: 'image/svg+xml', sizes: 'any' }],
  }
  return Response.json(body, {
    headers: { 'Cache-Control': 'public, max-age=0, s-maxage=60' },
  })
}
