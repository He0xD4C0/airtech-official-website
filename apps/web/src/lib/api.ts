import type {
  AcceptedResponse,
  AnalyticsConsentReceipt,
  AnalyticsEventReceipt,
  ContentEntry,
  ContractResponse,
  CreateAnalyticsConsent,
  CreateGuestVisit,
  CreateContactRequest,
  CreateAnalyticsEvent,
  CreateRfqRequest,
  DiscoveryDocument,
  DiscoveryEntry,
  GeneralInformation,
  GuestVisit,
  NewsEntry,
  NewsPage,
  Product,
  ProductFamily,
  ProductFamilyPresentation,
  ProductPage,
  ProblemDetails,
  RouteResolution,
  SelectorRequest,
  SelectorResponse,
  SiteBootstrap,
  operations,
} from '@airtek/contracts'
import { ApiError as PublicApiError, createContractClient } from '@airtek/contracts'
import { PUBLIC_PRODUCT_PAGE_SIZE } from './productPagination'

export { PublicApiError }

export interface PublicApiClientOptions {
  baseUrl: string
  fetchImpl?: typeof fetch
}

export type PublishedProductListQuery = NonNullable<
  operations['listPublishedProducts']['parameters']['query']
>
export type PublishedNewsListQuery = NonNullable<
  operations['listPublishedNews']['parameters']['query']
>
export type ProductFamilyProjectionResponse = ProductFamilyPresentation
export type GeneralInformationResponse = GeneralInformation
export type SiteBootstrapResponse = SiteBootstrap
export type RouteProjectionResponse = RouteResolution
export type NewsEntryResponse = NewsEntry
export type NewsPageResponse = NewsPage
export type PublicDiscoveryEntryResponse = DiscoveryEntry
export type PublicDiscoveryResponse = DiscoveryDocument
export type GuestVisitRequest = CreateGuestVisit
export type GuestVisitResponse = GuestVisit

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

function validProblemErrors(value: unknown): value is Record<string, string[]> {
  return isRecord(value)
    && Object.values(value).every((messages) => Array.isArray(messages)
      && messages.every((message) => typeof message === 'string'))
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function nullableString(value: unknown): value is string | null {
  return value === null || typeof value === 'string'
}

function publishedContentResponse(value: unknown): ContentEntry {
  if (!isRecord(value)
    || typeof value.id !== 'string'
    || typeof value.kind !== 'string'
    || typeof value.slug !== 'string'
    || value.locale !== 'en'
    || typeof value.title !== 'string'
    || !isRecord(value.body)
    || value.body.schemaVersion !== 1
    || !isRecord(value.body.doc)
    || !isRecord(value.seo)
    || value.status !== 'published'
    || typeof value.isPlaceholder !== 'boolean'
    || !Number.isInteger(value.currentRevision)
    || !Number.isInteger(value.publishedRevision)
    || Number(value.publishedRevision) <= 0
    || typeof value.updatedAt !== 'string'
    || !Number.isFinite(Date.parse(value.updatedAt))) {
    throw new Error('The server returned an invalid published content record.')
  }
  return value as unknown as ContentEntry
}

function optionalPublishedContent(value: unknown): ContentEntry | null {
  return value === null ? null : publishedContentResponse(value)
}

function productFamilyProjectionResponse(value: unknown): ProductFamilyProjectionResponse {
  const codes = ['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'] as const
  if (!isRecord(value)
    || !codes.includes(value.code as typeof codes[number])
    || typeof value.slug !== 'string'
    || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(value.slug)
    || typeof value.name !== 'string'
    || typeof value.description !== 'string'
    || !Number.isInteger(value.sortOrder)) {
    throw new Error('The server returned an invalid product-family projection.')
  }
  return value as unknown as ProductFamilyProjectionResponse
}

function generalInformationResponse(value: unknown): GeneralInformationResponse {
  if (!isRecord(value)
    || typeof value.id !== 'string'
    || value.locale !== 'en'
    || !isRecord(value.payload)
    || value.status !== 'published'
    || !Number.isInteger(value.currentRevision)
    || !Number.isInteger(value.publishedRevision)
    || Number(value.publishedRevision) <= 0
    || typeof value.isPlaceholder !== 'boolean'
    || typeof value.updatedAt !== 'string'
    || !Number.isFinite(Date.parse(value.updatedAt))) {
    throw new Error('The server returned invalid General Information.')
  }
  return value as unknown as GeneralInformationResponse
}

function siteBootstrapResponse(value: unknown): SiteBootstrapResponse {
  if (!isRecord(value)
    || !Array.isArray(value.productFamilies)
    || !Array.isArray(value.motorTechnologies)
    || !value.motorTechnologies.every((item) => typeof item === 'string' && item.trim() && item.length <= 120 && !/[\u0000-\u001f\u007f]/u.test(item))
    || typeof value.generatedAt !== 'string'
    || !Number.isFinite(Date.parse(value.generatedAt))) {
    throw new Error('The server returned an invalid public-site bootstrap.')
  }
  return {
    generalInformation: value.generalInformation === null ? null : generalInformationResponse(value.generalInformation),
    navigation: optionalPublishedContent(value.navigation),
    footer: optionalPublishedContent(value.footer),
    productFamilies: value.productFamilies.map(productFamilyProjectionResponse),
    motorTechnologies: [...new Set(value.motorTechnologies.map((item) => item.trim()))],
    generatedAt: value.generatedAt,
  }
}

function routeProjectionResponse(value: unknown): RouteProjectionResponse {
  if (!isRecord(value)
    || typeof value.path !== 'string'
    || typeof value.templateKey !== 'string'
    || !/^[A-Za-z][A-Za-z0-9-]{0,79}$/u.test(value.templateKey)
    || typeof value.entityType !== 'string'
    || (value.entityId !== null && typeof value.entityId !== 'string')
    || value.locale !== 'en'
    || (value.publishedRevision !== null && (!Number.isInteger(value.publishedRevision) || Number(value.publishedRevision) <= 0))
    || typeof value.indexable !== 'boolean'
    || !['editorial', 'feishu', 'verifiedCsv', 'developmentFixture'].includes(String(value.dataClass))) {
    throw new Error('The server returned an invalid public-route projection.')
  }
  return {
    path: value.path,
    templateKey: value.templateKey,
    entityType: value.entityType,
    entityId: value.entityId,
    locale: value.locale,
    publishedRevision: value.publishedRevision === null ? null : Number(value.publishedRevision),
    indexable: value.indexable,
    dataClass: value.dataClass as RouteProjectionResponse['dataClass'],
    page: optionalPublishedContent(value.page),
  }
}

function publicDiscoveryResponse(value: unknown): PublicDiscoveryResponse {
  if (!isRecord(value)
    || typeof value.generatedAt !== 'string'
    || !Number.isFinite(Date.parse(value.generatedAt))
    || !Array.isArray(value.entries)) {
    throw new Error('The server returned an invalid public discovery projection.')
  }
  const entries = value.entries.map((entry): PublicDiscoveryEntryResponse => {
    if (!isRecord(entry)
      || !['content', 'product'].includes(String(entry.entityType))
      || typeof entry.entityId !== 'string'
      || typeof entry.path !== 'string'
      || entry.locale !== 'en'
      || (entry.title !== undefined && typeof entry.title !== 'string')
      || (entry.summary !== undefined && !nullableString(entry.summary))
      || typeof entry.updatedAt !== 'string'
      || !Number.isFinite(Date.parse(entry.updatedAt))) {
      throw new Error('The server returned an invalid public discovery entry.')
    }
    return entry as unknown as PublicDiscoveryEntryResponse
  })
  return { generatedAt: value.generatedAt, entries }
}

function newsEntryResponse(value: unknown): NewsEntryResponse {
  if (!isRecord(value)
    || typeof value.category !== 'string'
    || !value.category.trim()
    || !nullableString(value.authorDisplayName)
    || !nullableString(value.coverMediaId)
    || !nullableString(value.publishedAt)
    || (typeof value.publishedAt === 'string' && !Number.isFinite(Date.parse(value.publishedAt)))
    || typeof value.featured !== 'boolean'
    || !['editorial', 'feishu', 'verifiedCsv', 'developmentFixture'].includes(String(value.dataClass))) {
    throw new Error('The server returned an invalid News projection.')
  }
  return {
    content: publishedContentResponse(value.content),
    category: value.category,
    authorDisplayName: value.authorDisplayName,
    coverMediaId: value.coverMediaId,
    publishedAt: value.publishedAt,
    featured: value.featured,
    dataClass: value.dataClass as NewsEntryResponse['dataClass'],
  }
}

function newsPageResponse(value: unknown): NewsPageResponse {
  if (!isRecord(value)
    || !Array.isArray(value.items)
    || (value.nextCursor !== null && typeof value.nextCursor !== 'string')) {
    throw new Error('The server returned an invalid News page.')
  }
  return { items: value.items.map(newsEntryResponse), nextCursor: value.nextCursor }
}

function guestVisitResponse(value: unknown): GuestVisitResponse {
  if (!isRecord(value)
    || typeof value.id !== 'string'
    || typeof value.anonymousSessionId !== 'string'
    || typeof value.landingPath !== 'string'
    || !nullableString(value.referrerDomain)
    || typeof value.source !== 'string'
    || !nullableString(value.medium)
    || !nullableString(value.campaign)
    || typeof value.firstSeenAt !== 'string'
    || typeof value.lastSeenAt !== 'string'
    || typeof value.retentionUntil !== 'string') {
    throw new Error('The server returned an invalid Guest visit record.')
  }
  return value as unknown as GuestVisitResponse
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

function productListQuery(query: PublishedProductListQuery): PublishedProductListQuery {
  const limit = query.limit ?? PUBLIC_PRODUCT_PAGE_SIZE
  if (!Number.isInteger(limit) || limit < 1 || limit > 100) throw new Error('The product page size is invalid.')
  if (query.cursor && !/^[A-Za-z0-9_-]{1,2048}$/u.test(query.cursor)) throw new Error('The product cursor is invalid.')
  if (query.motorTechnology && (query.motorTechnology.length > 120 || /[\u0000-\u001f\u007f]/u.test(query.motorTechnology))) {
    throw new Error('The motor technology filter is invalid.')
  }
  if (query.family && !['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'].includes(query.family)) {
    throw new Error('The product family filter is invalid.')
  }
  return {
    limit,
    ...(query.family ? { family: query.family } : {}),
    ...(query.motorTechnology?.trim() ? { motorTechnology: query.motorTechnology.trim() } : {}),
    ...(query.cursor ? { cursor: query.cursor } : {}),
  }
}

function newsListQuery(query: PublishedNewsListQuery): PublishedNewsListQuery {
  const limit = query.limit ?? 24
  if (!Number.isInteger(limit) || limit < 1 || limit > 100) throw new Error('The News page size is invalid.')
  if (query.cursor && !/^[A-Za-z0-9_-]{1,2048}$/u.test(query.cursor)) throw new Error('The News cursor is invalid.')
  if (query.category && !/^[A-Za-z0-9][A-Za-z0-9 _.-]{0,119}$/u.test(query.category)) {
    throw new Error('The News category is invalid.')
  }
  return {
    locale: query.locale ?? 'en',
    limit,
    ...(query.category ? { category: query.category } : {}),
    ...(query.cursor ? { cursor: query.cursor } : {}),
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
    async submitAnalyticsEvent(payload: CreateAnalyticsEvent) {
      const result = await publicContractRequest(client.post('/api/public/v1/analytics/events', {
        body: payload,
        credentials: 'omit',
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
    async listProducts(query: PublishedProductListQuery = {}) {
      const result = await publicContractRequest(client.get('/api/public/v1/products', {
        credentials: 'omit',
        parameters: { query: productListQuery(query) },
      }))
      return productPageResponse(result.data)
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

export function getPublishedProduct(slug: string, family?: ProductFamily) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).getProduct(slug, family)
}

export function getPublishedProducts(query: PublishedProductListQuery = {}) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).listProducts(query)
}

export function getPublishedNews(query: PublishedNewsListQuery = {}) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).listNews(query)
}

export function recordGuestVisit(payload: GuestVisitRequest) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).recordGuestVisit(payload)
}

export function newIdempotencyKey() {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return crypto.randomUUID()
  return `request-${Date.now()}-${Math.random().toString(16).slice(2)}`
}
