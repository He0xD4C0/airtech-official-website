import type {
  ContractResponse,
  CreateAnalyticsConsent,
  CreateAnalyticsEvent,
  CreateContactRequest,
  CreateRfqRequest,
  ProblemDetails,
  ProductFamily,
  SelectorRequest,
} from '@airtek/contracts'
import { ApiError as PublicApiError, createContractClient } from '@airtek/contracts'
import {
  acceptedResponse,
  analyticsConsentReceipt,
  analyticsEventReceipt,
  guestVisitResponse,
  isRecord,
  newsEntryResponse,
  newsListQuery,
  newsPageResponse,
  productListQuery,
  productPageResponse,
  productResponse,
  productSourceAssetDocumentResponse,
  publicDiscoveryResponse,
  routeProjectionResponse,
  selectorResponse,
  searchPageResponse,
  searchQuery,
  siteBootstrapResponse,
  validProblemErrors,
} from './publicApiDecoders'
import type {
  GuestVisitRequest,
  PublicApiClientOptions,
  PublishedNewsListQuery,
  PublishedProductListQuery,
  PublishedSearchQuery,
} from './publicApiTypes'

async function publicContractRequest<Data>(
  request: Promise<ContractResponse<Data>>,
): Promise<ContractResponse<Data>> {
  try {
    return await request
  } catch (cause) {
    if (!(cause instanceof PublicApiError) || !cause.response) throw cause
    const problem: Record<string, unknown> = isRecord(cause.problem) ? cause.problem : {}
    const responseStatus = cause.response.status
    const normalized: ProblemDetails = {
      type: typeof problem.type === 'string' ? problem.type : 'about:blank',
      title: typeof problem.title === 'string' && problem.title
        ? problem.title
        : cause.response.statusText || 'Request failed',
      status: responseStatus,
      detail: typeof problem.detail === 'string' && problem.detail
        ? problem.detail
        : cause.response.statusText || 'Request failed',
      requestId: typeof problem.requestId === 'string'
        ? problem.requestId
        : cause.response.headers.get('X-Request-ID') || '00000000-0000-0000-0000-000000000000',
      ...(problem.instance === null || typeof problem.instance === 'string'
        ? { instance: problem.instance }
        : {}),
      ...(validProblemErrors(problem.errors) ? { errors: problem.errors } : {}),
    }
    throw new PublicApiError(normalized, cause.response)
  }
}

function contractBaseUrl(publicApiBaseUrl: string): string {
  const normalized = publicApiBaseUrl.replace(/\/$/, '')
  const publicPrefix = '/api/public/v1'
  if (!normalized.endsWith(publicPrefix)) {
    throw new Error(`The public API base URL must end with ${publicPrefix}.`)
  }
  return normalized.slice(0, -publicPrefix.length)
}

export function createPublicApiClient(options: PublicApiClientOptions) {
  const client = createContractClient({
    baseUrl: contractBaseUrl(options.baseUrl),
    fetchImpl: options.fetchImpl,
  })

  return {
    async submitContact(payload: CreateContactRequest, idempotencyKey: string) {
      const result = await publicContractRequest(client.post('/api/public/v1/contact', {
        body: payload,
        credentials: 'omit',
        parameters: { header: { 'Idempotency-Key': idempotencyKey } },
      }))
      return acceptedResponse(result.data)
    },

    async submitRfq(payload: CreateRfqRequest, idempotencyKey: string) {
      const result = await publicContractRequest(client.post('/api/public/v1/rfqs', {
        body: payload,
        credentials: 'omit',
        parameters: { header: { 'Idempotency-Key': idempotencyKey } },
      }))
      return acceptedResponse(result.data)
    },

    async selectProducts(payload: SelectorRequest) {
      const result = await publicContractRequest(client.post('/api/public/v1/selector', {
        body: payload,
        credentials: 'omit',
      }))
      return selectorResponse(result.data)
    },

    async submitAnalyticsEvent(payload: CreateAnalyticsEvent, idempotencyKey: string) {
      const result = await publicContractRequest(client.post('/api/public/v1/analytics/events', {
        body: payload,
        credentials: 'omit',
        parameters: { header: { 'Idempotency-Key': idempotencyKey } },
      }))
      return analyticsEventReceipt(result.data)
    },

    async submitAnalyticsConsent(payload: CreateAnalyticsConsent) {
      const result = await publicContractRequest(client.post('/api/public/v1/analytics/consents', {
        body: payload,
        credentials: 'omit',
      }))
      return analyticsConsentReceipt(result.data)
    },

    async getProduct(slug: string, family?: ProductFamily) {
      if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(slug)) throw new Error('The product slug is invalid.')
      if (family && !['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'].includes(family)) {
        throw new Error('The product family is invalid.')
      }
      const result = await publicContractRequest(client.get('/api/public/v1/products/{slug}', {
        credentials: 'omit',
        parameters: {
          path: { slug },
          ...(family ? { query: { family } } : {}),
        },
      }))
      return productResponse(result.data)
    },

    async getProductAssets(slug: string, family?: ProductFamily) {
      if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(slug)) throw new Error('The product slug is invalid.')
      if (family && !['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'].includes(family)) {
        throw new Error('The product family is invalid.')
      }
      const result = await publicContractRequest(client.get('/api/public/v1/products/{slug}/assets', {
        credentials: 'omit',
        parameters: {
          path: { slug },
          ...(family ? { query: { family } } : {}),
        },
      }))
      return productSourceAssetDocumentResponse(result.data)
    },

    async listProducts(query: PublishedProductListQuery = {}) {
      const result = await publicContractRequest(client.get('/api/public/v1/products', {
        credentials: 'omit',
        parameters: { query: productListQuery(query) },
      }))
      return productPageResponse(result.data)
    },

    async search(query: PublishedSearchQuery = {}) {
      const result = await publicContractRequest(client.get('/api/public/v1/search', {
        credentials: 'omit',
        parameters: { query: searchQuery(query) },
      }))
      return searchPageResponse(result.data)
    },

    async getDiscovery() {
      const result = await publicContractRequest(client.get('/api/public/v1/discovery', { credentials: 'omit' }))
      return publicDiscoveryResponse(result.data)
    },

    async getSiteBootstrap(locale = 'en') {
      if (locale !== 'en') throw new Error('The public locale is invalid.')
      const result = await publicContractRequest(client.get('/api/public/v1/site-bootstrap', {
        credentials: 'omit',
        parameters: { query: { locale } },
      }))
      return siteBootstrapResponse(result.data)
    },

    async resolveRoute(path: string, locale = 'en') {
      if (locale !== 'en'
        || (path !== '/en' && !path.startsWith('/en/'))
        || path.includes('?')
        || path.includes('#')) {
        throw new Error('The public route lookup is invalid.')
      }
      const result = await publicContractRequest(client.get('/api/public/v1/routes/resolve', {
        credentials: 'omit',
        parameters: { query: { path, locale } },
      }))
      return routeProjectionResponse(result.data)
    },

    async listNews(query: PublishedNewsListQuery = {}) {
      const result = await publicContractRequest(client.get('/api/public/v1/news', {
        credentials: 'omit',
        parameters: { query: newsListQuery(query) },
      }))
      return newsPageResponse(result.data)
    },

    async getNews(slug: string, locale: 'en' = 'en') {
      if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(slug)) throw new Error('The News slug is invalid.')
      const result = await publicContractRequest(client.get('/api/public/v1/news/{slug}', {
        credentials: 'omit',
        parameters: { path: { slug }, query: { locale } },
      }))
      return newsEntryResponse(result.data)
    },

    async recordGuestVisit(payload: GuestVisitRequest) {
      const result = await publicContractRequest(client.post('/api/public/v1/guest-visits', {
        body: payload,
        credentials: 'omit',
      }))
      return guestVisitResponse(result.data)
    },
  }
}
