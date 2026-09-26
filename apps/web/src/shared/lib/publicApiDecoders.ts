import type {
  AcceptedResponse,
  AnalyticsConsentReceipt,
  AnalyticsEventReceipt,
  DataClass,
  SelectorResponse,
} from '@airtek/contracts'
import { parseOpenApiSchema } from '@airtek/contracts'
import type { PublicContentProjection } from '@/shared/types/projection'
import type {
  GuestVisitResponse,
  NewsEntryResponse,
  NewsPageResponse,
  ProductFamilyProjectionResponse,
  PublicDiscoveryEntryResponse,
  PublicDiscoveryResponse,
  PublishedNewsListQuery,
  RouteProjectionResponse,
  SiteBootstrapResponse,
} from '@/shared/lib/publicApiTypes'

export {
  productListQuery,
  productPageResponse,
  productResponse,
  productSourceAssetDocumentResponse,
  searchPageResponse,
  searchQuery,
} from '@/shared/lib/publicApiProductDecoders'

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

function nullableInteger(value: unknown): value is number | null {
  return value === null || Number.isInteger(value)
}

const contentKinds = new Set([
  'home', 'page', 'solution', 'technology', 'article', 'news', 'faq', 'caseStudy',
  'download', 'company', 'legal', 'generalInformation', 'navigation', 'footer',
])
const templateKeys = new Set([
  'home', 'productIndex', 'productFamily', 'selector', 'compare', 'solutionIndex',
  'solutionDetail', 'technologyIndex', 'technologyDetail', 'articleIndex',
  'articleDetail', 'newsIndex', 'newsDetail', 'faqIndex', 'faqDetail',
  'caseStudyIndex', 'caseStudyDetail', 'downloadIndex', 'downloadDetail', 'about',
  'contact', 'rfqRouter', 'rfqForm', 'search', 'legal', 'navigation', 'footer',
  'generalInformation',
])
const routeTemplateKeys = new Set([...templateKeys, 'productDetail'])
const contentBlockKinds = new Set([
  'hero', 'body', 'media', 'featureGrid', 'evidence', 'cta', 'relationCollection',
  'faqCollection', 'downloadAsset', 'contactBlock',
])
const dataClasses = new Set<string>([
  'editorial', 'feishu', 'verifiedCsv', 'developmentFixture',
])

function validDataClass(value: unknown): value is DataClass {
  return typeof value === 'string' && dataClasses.has(value)
}

function validRouteEntityType(value: unknown): value is 'content' | 'product' {
  return value === 'content' || value === 'product'
}

function validV2Block(value: unknown): boolean {
  return isRecord(value)
    && typeof value.id === 'string'
    && typeof value.type === 'string'
    && contentBlockKinds.has(value.type)
}

function validResolvedRelation(value: unknown): boolean {
  return isRecord(value)
    && typeof value.relationId === 'string'
    && ['content', 'product'].includes(String(value.entityType))
    && typeof value.title === 'string'
    && nullableString(value.summary)
    && typeof value.href === 'string'
    && nullableString(value.eyebrow)
    && Array.isArray(value.tags)
    && value.tags.every((tag) => typeof tag === 'string')
}

function validResolvedLink(value: unknown): boolean {
  return isRecord(value)
    && typeof value.contentId === 'string'
    && typeof value.href === 'string'
}

function validResolvedMedia(value: unknown): boolean {
  return isRecord(value)
    && typeof value.assetId === 'string'
    && typeof value.publicUrl === 'string'
    && nullableString(value.previewUrl)
    && typeof value.downloadUrl === 'string'
    && typeof value.originalName === 'string'
    && typeof value.mediaType === 'string'
    && Number.isInteger(value.byteSize)
    && Number(value.byteSize) >= 0
    && nullableInteger(value.originalWidth)
    && nullableInteger(value.originalHeight)
    && nullableInteger(value.previewWidth)
    && nullableInteger(value.previewHeight)
    && nullableInteger(value.previewByteSize)
}

export function publicContentProjectionResponse(value: unknown): PublicContentProjection {
  if (!isRecord(value)
    || value.schemaVersion !== 2
    || 'payload' in value
    || 'status' in value
    || 'currentRevision' in value
    || typeof value.id !== 'string'
    || !contentKinds.has(String(value.kind))
    || value.locale !== 'en'
    || !templateKeys.has(String(value.templateKey))
    || typeof value.title !== 'string'
    || !nullableString(value.slug)
    || (typeof value.slug === 'string' && !/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(value.slug))
    || !nullableString(value.summary)
    || typeof value.isPlaceholder !== 'boolean'
    || (value.body !== null && (!isRecord(value.body)
      || value.body.type !== 'doc'
      || !Array.isArray(value.body.content)))
    || !isRecord(value.composition)
    || !Array.isArray(value.composition.blocks)
    || !value.composition.blocks.every(validV2Block)
    || !isRecord(value.typeFields)
    || typeof value.typeFields.type !== 'string'
    || !contentKinds.has(value.typeFields.type)
    || value.typeFields.type !== value.kind
    || !isRecord(value.seo)
    || typeof value.seo.indexable !== 'boolean'
    || !Number.isInteger(value.publishedRevision)
    || Number(value.publishedRevision) <= 0
    || typeof value.updatedAt !== 'string'
    || !Number.isFinite(Date.parse(value.updatedAt))
    || !Array.isArray(value.resolvedRelations)
    || !value.resolvedRelations.every(validResolvedRelation)
    || !Array.isArray(value.resolvedLinks)
    || !value.resolvedLinks.every(validResolvedLink)
    || !Array.isArray(value.resolvedMedia)
    || !value.resolvedMedia.every(validResolvedMedia)) {
    throw new Error('The server returned an invalid CMS V2 public projection.')
  }
  return parseOpenApiSchema<PublicContentProjection>('PublicContentProjection', value)
}

function optionalPublicContentProjection(value: unknown): PublicContentProjection | null {
  return value === null ? null : publicContentProjectionResponse(value)
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
  return parseOpenApiSchema<ProductFamilyProjectionResponse>('ProductFamilyPresentation', value)
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
    generalInformation: optionalPublicContentProjection(value.generalInformation),
    navigation: optionalPublicContentProjection(value.navigation),
    footer: optionalPublicContentProjection(value.footer),
    productFamilies: value.productFamilies.map(productFamilyProjectionResponse),
    motorTechnologies: [...new Set(value.motorTechnologies.map((item) => item.trim()))],
    generatedAt: value.generatedAt,
  }
}

export function routeProjectionResponse(value: unknown): RouteProjectionResponse {
  if (!isRecord(value)
    || typeof value.path !== 'string'
    || typeof value.templateKey !== 'string'
    || !routeTemplateKeys.has(value.templateKey)
    || !validRouteEntityType(value.entityType)
    || (value.entityId !== null && typeof value.entityId !== 'string')
    || value.locale !== 'en'
    || (value.publishedRevision !== null && (!Number.isInteger(value.publishedRevision) || Number(value.publishedRevision) <= 0))
    || typeof value.indexable !== 'boolean'
    || !validDataClass(value.dataClass)) {
    throw new Error('The server returned an invalid public-route projection.')
  }
  const page = optionalPublicContentProjection(value.page)
  if (value.entityType === 'content'
    && (!page
      || value.entityId !== page.id
      || value.publishedRevision !== page.publishedRevision
      || value.templateKey !== page.templateKey
      || value.locale !== page.locale
      || (value.indexable && (page.isPlaceholder || !page.seo.indexable)))) {
    throw new Error('The server returned an inconsistent CMS V2 route projection.')
  }
  if (value.entityType === 'product'
    && (value.templateKey !== 'productDetail'
      || page !== null
      || value.entityId === null
      || value.publishedRevision === null)) {
    throw new Error('The server returned an invalid product route projection.')
  }
  return {
    path: value.path,
    templateKey: value.templateKey,
    entityType: value.entityType,
    entityId: value.entityId,
    locale: value.locale,
    publishedRevision: value.publishedRevision === null ? null : Number(value.publishedRevision),
    indexable: value.indexable,
    dataClass: value.dataClass,
    page,
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
    return parseOpenApiSchema<PublicDiscoveryEntryResponse>('DiscoveryEntry', entry)
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
    || !validDataClass(value.dataClass)) {
    throw new Error('The server returned an invalid News projection.')
  }
  const content = publicContentProjectionResponse(value.content)
  if (content.kind !== 'news'
    || content.templateKey !== 'newsDetail'
    || !content.slug) {
    throw new Error('The server returned a non-routable News projection.')
  }
  return {
    content,
    category: value.category,
    authorDisplayName: value.authorDisplayName,
    coverMediaId: value.coverMediaId,
    publishedAt: value.publishedAt,
    featured: value.featured,
    dataClass: value.dataClass,
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
  return parseOpenApiSchema<GuestVisitResponse>('GuestVisit', value)
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
    && typeof candidate.slug === 'string'
    && ['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'].includes(String(candidate.family))
    && typeof candidate.canonicalPath === 'string'
    && candidate.canonicalPath.startsWith('/en/products/')
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
  return parseOpenApiSchema<SelectorResponse>('SelectorResponse', value)
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
  return parseOpenApiSchema<AnalyticsConsentReceipt>('AnalyticsConsentReceipt', value)
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
