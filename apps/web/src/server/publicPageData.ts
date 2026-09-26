import { ApiError as PublicApiError } from '@airtek/contracts'
import { createPublicApiClient } from '@/shared/lib/publicApiClient'
import { enrichPage } from '@/server/publicPageEnrichment'
import { PublicPageDataError } from '@/server/publicPageDataTypes'
import type { LoadedPublicPageData, PublicPageDataOptions } from '@/server/publicPageDataTypes'
import { parseSiteBootstrap } from '@/server/publicProjectionParsing'
import { pageFromRoute } from '@/server/publicRouteModel'

export * from '@/server/publicPageDataTypes'

function internalApiBaseUrl(): string {
  return process.env.PUBLIC_API_INTERNAL_URL || 'http://localhost:8080/api/public/v1'
}

function normalizedBaseUrl(value: string): string {
  const url = new URL(value)
  if (!['http:', 'https:'].includes(url.protocol)) throw new PublicPageDataError('The public API origin is invalid.', 503)
  return url.toString().replace(/\/$/u, '')
}

export async function loadPublicPageData(
  path: string,
  options: PublicPageDataOptions = {},
): Promise<LoadedPublicPageData> {
  const baseUrl = normalizedBaseUrl(options.baseUrl ?? internalApiBaseUrl())
  const client = createPublicApiClient({ baseUrl, fetchImpl: options.fetchImpl })
  const [bootstrapResult, routeResult] = await Promise.allSettled([
    client.getSiteBootstrap('en'),
    client.resolveRoute(path, 'en'),
  ])
  if (bootstrapResult.status === 'rejected') {
    throw new PublicPageDataError('The public site bootstrap is temporarily unavailable.', 503)
  }
  if (routeResult.status === 'rejected') {
    if (routeResult.reason instanceof PublicApiError && routeResult.reason.status === 404) {
      throw new PublicPageDataError('The requested public page does not exist.', 404)
    }
    throw new PublicPageDataError('The public route projection is temporarily unavailable.', 503)
  }
  if (routeResult.value.path !== path || routeResult.value.locale !== 'en') {
    throw new PublicPageDataError('The public route projection did not match the request.', 503)
  }
  const site = parseSiteBootstrap(bootstrapResult.value)
  const page = pageFromRoute(routeResult.value, site)
  return { page: await enrichPage(page, routeResult.value, client, options), site }
}
