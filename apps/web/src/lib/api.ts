import type {
  AcceptedResponse,
  AnalyticsConsentReceipt,
  AnalyticsEventReceipt,
  CreateAnalyticsConsent,
  CreateContactRequest,
  CreateAnalyticsEvent,
  CreateRfqRequest,
  ProblemDetails,
  Product,
  ProductFamily,
  ProductPage,
  SelectorRequest,
  SelectorResponse,
} from '@airtek/contracts'
import { PUBLIC_PRODUCT_PAGE_SIZE } from './productPagination'

export interface PublicApiClientOptions {
  baseUrl: string
  fetchImpl?: typeof fetch
}

export interface PublishedProductListQuery {
  cursor?: string
  family?: ProductFamily
  limit?: number
  motorTechnology?: string
}

function normalizeBaseUrl(value: string) {
  return value.replace(/\/$/, '')
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function acceptedResponse(value: unknown): AcceptedResponse {
  if (!isRecord(value)
    || typeof value.id !== 'string'
    || typeof value.reference !== 'string'
    || typeof value.acceptedAt !== 'string') {
    throw new Error('The server returned an invalid submission acknowledgement.')
  }
  return { id: value.id, reference: value.reference, acceptedAt: value.acceptedAt }
}

function selectorResponse(value: unknown): SelectorResponse {
  const validCandidate = (candidate: unknown) => isRecord(candidate)
    && typeof candidate.productId === 'string'
    && typeof candidate.productRevision === 'number'
    && typeof candidate.title === 'string'
    && typeof candidate.rank === 'number'
    && Array.isArray(candidate.matchedConstraints)
    && candidate.matchedConstraints.every((item) => typeof item === 'string')
    && Array.isArray(candidate.warnings)
    && candidate.warnings.every((item) => typeof item === 'string')
  if (!isRecord(value)
    || !['matched', 'noValidatedCandidates', 'engineeringReviewRequired'].includes(String(value.outcome))
    || !Array.isArray(value.candidates)
    || !value.candidates.every(validCandidate)
    || !Array.isArray(value.explanations)
    || !value.explanations.every((item) => typeof item === 'string')) {
    throw new Error('The selector returned an invalid response.')
  }
  return value as unknown as SelectorResponse
}

function analyticsEventReceipt(value: unknown): AnalyticsEventReceipt {
  if (!isRecord(value)
    || typeof value.accepted !== 'boolean'
    || (value.eventId !== undefined && value.eventId !== null && typeof value.eventId !== 'string')) {
    throw new Error('The server returned an invalid analytics acknowledgement.')
  }
  return { accepted: value.accepted, eventId: typeof value.eventId === 'string' ? value.eventId : null }
}

function analyticsConsentReceipt(value: unknown): AnalyticsConsentReceipt {
  if (!isRecord(value)
    || typeof value.consentReceipt !== 'string'
    || typeof value.anonymousSessionId !== 'string'
    || value.policyVersion !== 'analytics-v1'
    || typeof value.analyticsAllowed !== 'boolean'
    || typeof value.grantedAt !== 'string'
    || typeof value.expiresAt !== 'string') {
    throw new Error('The server returned an invalid analytics consent receipt.')
  }
  return value as unknown as AnalyticsConsentReceipt
}

function productResponse(value: unknown): Product {
  const validSpec = (spec: unknown) => isRecord(spec)
    && typeof spec.key === 'string'
    && typeof spec.label === 'string'
    && typeof spec.state === 'string'
  const validCurve = (curve: unknown) => isRecord(curve)
    && typeof curve.state === 'string'
    && Array.isArray(curve.points)
  if (!isRecord(value)
    || typeof value.id !== 'string'
    || typeof value.stableId !== 'string'
    || typeof value.slug !== 'string'
    || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(value.slug)
    || typeof value.title !== 'string'
    || !['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'].includes(String(value.family))
    || value.locale !== 'en'
    || value.status !== 'published'
    || typeof value.indexable !== 'boolean'
    || !Number.isInteger(value.publishedRevision)
    || !Array.isArray(value.specifications)
    || !value.specifications.every(validSpec)
    || !Array.isArray(value.performanceCurves)
    || !value.performanceCurves.every(validCurve)) {
    throw new Error('The server returned an invalid published product.')
  }
  return value as unknown as Product
}

function productPageResponse(value: unknown): ProductPage {
  if (!isRecord(value)
    || !Array.isArray(value.items)
    || (value.nextCursor !== null && typeof value.nextCursor !== 'string')) {
    throw new Error('The server returned an invalid published product page.')
  }
  return {
    items: value.items.map(productResponse),
    nextCursor: value.nextCursor,
  }
}

function productListPath(query: PublishedProductListQuery): string {
  const limit = query.limit ?? PUBLIC_PRODUCT_PAGE_SIZE
  if (!Number.isInteger(limit) || limit < 1 || limit > 100) throw new Error('The product page size is invalid.')
  if (query.cursor && !/^[A-Za-z0-9_-]{1,2048}$/u.test(query.cursor)) throw new Error('The product cursor is invalid.')
  if (query.motorTechnology && (query.motorTechnology.length > 120 || /[\u0000-\u001f\u007f]/u.test(query.motorTechnology))) {
    throw new Error('The motor technology filter is invalid.')
  }
  if (query.family && !['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'].includes(query.family)) {
    throw new Error('The product family filter is invalid.')
  }
  const params = new URLSearchParams({ limit: String(limit) })
  if (query.family) params.set('family', query.family)
  if (query.motorTechnology?.trim()) params.set('motorTechnology', query.motorTechnology.trim())
  if (query.cursor) params.set('cursor', query.cursor)
  return `/products?${params.toString()}`
}

export function createPublicApiClient(options: PublicApiClientOptions) {
  const fetchImpl = options.fetchImpl ?? fetch
  const baseUrl = normalizeBaseUrl(options.baseUrl)

  async function post(path: string, payload: unknown, idempotencyKey?: string): Promise<unknown> {
    const headers = new Headers({
      'Content-Type': 'application/json',
      Accept: 'application/json',
    })
    if (idempotencyKey) headers.set('Idempotency-Key', idempotencyKey)

    const response = await fetchImpl(`${baseUrl}${path}`, {
      method: 'POST',
      credentials: 'omit',
      headers,
      body: JSON.stringify(payload),
    })

    const body = await response.json().catch(() => undefined) as unknown
    if (!response.ok) {
      const problem = isRecord(body) ? body as unknown as ProblemDetails : undefined
      throw new Error(problem?.detail || problem?.title || 'The request could not be submitted.')
    }
    return body
  }

  async function get(path: string): Promise<unknown> {
    const response = await fetchImpl(`${baseUrl}${path}`, {
      credentials: 'omit',
      headers: { Accept: 'application/json' },
    })
    const body = await response.json().catch(() => undefined) as unknown
    if (!response.ok) {
      const problem = isRecord(body) ? body as unknown as ProblemDetails : undefined
      throw new Error(problem?.detail || problem?.title || 'The requested public record is unavailable.')
    }
    return body
  }

  return {
    async submitContact(payload: CreateContactRequest, idempotencyKey: string) {
      return acceptedResponse(await post('/contact', payload, idempotencyKey))
    },
    async submitRfq(payload: CreateRfqRequest, idempotencyKey: string) {
      return acceptedResponse(await post('/rfqs', payload, idempotencyKey))
    },
    async selectProducts(payload: SelectorRequest) {
      return selectorResponse(await post('/selector', payload))
    },
    async submitAnalyticsEvent(payload: CreateAnalyticsEvent) {
      return analyticsEventReceipt(await post('/analytics/events', payload))
    },
    async submitAnalyticsConsent(payload: CreateAnalyticsConsent) {
      return analyticsConsentReceipt(await post('/analytics/consents', payload))
    },
    async getProduct(slug: string) {
      if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(slug)) throw new Error('The product slug is invalid.')
      return productResponse(await get(`/products/${encodeURIComponent(slug)}`))
    },
    async listProducts(query: PublishedProductListQuery = {}) {
      return productPageResponse(await get(productListPath(query)))
    },
  }
}

function browserApiBaseUrl() {
  return import.meta.env.VITE_PUBLIC_API_BASE_URL || 'http://localhost:8080/api/public/v1'
}

export function submitContact(payload: CreateContactRequest, idempotencyKey: string) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).submitContact(payload, idempotencyKey)
}

export function submitRfq(payload: CreateRfqRequest, idempotencyKey: string) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).submitRfq(payload, idempotencyKey)
}

export function selectProducts(payload: SelectorRequest) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).selectProducts(payload)
}

export function submitAnalyticsEvent(payload: CreateAnalyticsEvent) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).submitAnalyticsEvent(payload)
}

export function submitAnalyticsConsent(payload: CreateAnalyticsConsent) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).submitAnalyticsConsent(payload)
}

export function getPublishedProduct(slug: string) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).getProduct(slug)
}

export function getPublishedProducts(query: PublishedProductListQuery = {}) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).listProducts(query)
}

export function newIdempotencyKey() {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return crypto.randomUUID()
  return `request-${Date.now()}-${Math.random().toString(16).slice(2)}`
}
