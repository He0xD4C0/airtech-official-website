import type {
  AcceptedResponse,
  AnalyticsConsentReceipt,
  AnalyticsEventReceipt,
  ContentEntry,
  Product,
  ProductPage,
  SelectorResponse,
} from '@airtek/contracts'
import { PUBLIC_PRODUCT_PAGE_SIZE } from './productPagination'
import type {
  GeneralInformationResponse,
  GuestVisitResponse,
  NewsEntryResponse,
  NewsPageResponse,
  ProductFamilyProjectionResponse,
  PublicDiscoveryEntryResponse,
  PublicDiscoveryResponse,
  PublishedNewsListQuery,
  PublishedProductListQuery,
  RouteProjectionResponse,
  SiteBootstrapResponse,
} from './publicApiTypes'

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function validProblemErrors(value: unknown): value is Record<string, string[]> {
  return isRecord(value)
    && Object.values(value).every((messages) => Array.isArray(messages)
      && messages.every((message) => typeof message === 'string'))
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

export function siteBootstrapResponse(value: unknown): SiteBootstrapResponse {
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

export function routeProjectionResponse(value: unknown): RouteProjectionResponse {
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

export function publicDiscoveryResponse(value: unknown): PublicDiscoveryResponse {
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

export function newsEntryResponse(value: unknown): NewsEntryResponse {
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

export function newsPageResponse(value: unknown): NewsPageResponse {
  if (!isRecord(value)
    || !Array.isArray(value.items)
    || (value.nextCursor !== null && typeof value.nextCursor !== 'string')) {
    throw new Error('The server returned an invalid News page.')
  }
  return { items: value.items.map(newsEntryResponse), nextCursor: value.nextCursor }
}

export function guestVisitResponse(value: unknown): GuestVisitResponse {
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

export function acceptedResponse(value: unknown): AcceptedResponse {
  if (!isRecord(value)
    || typeof value.id !== 'string'
    || typeof value.reference !== 'string'
    || typeof value.acceptedAt !== 'string') {
    throw new Error('The server returned an invalid submission acknowledgement.')
  }
  return { id: value.id, reference: value.reference, acceptedAt: value.acceptedAt }
}

export function selectorResponse(value: unknown): SelectorResponse {
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

export function analyticsEventReceipt(value: unknown): AnalyticsEventReceipt {
  if (!isRecord(value)
    || typeof value.accepted !== 'boolean'
    || (value.eventId !== undefined && value.eventId !== null && typeof value.eventId !== 'string')) {
    throw new Error('The server returned an invalid analytics acknowledgement.')
  }
  return { accepted: value.accepted, eventId: typeof value.eventId === 'string' ? value.eventId : null }
}

export function analyticsConsentReceipt(value: unknown): AnalyticsConsentReceipt {
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

export function productResponse(value: unknown): Product {
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

export function productPageResponse(value: unknown): ProductPage {
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

export function productListQuery(query: PublishedProductListQuery): PublishedProductListQuery {
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

export function newsListQuery(query: PublishedNewsListQuery): PublishedNewsListQuery {
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
