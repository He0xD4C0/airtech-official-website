import type {
  CreateAnalyticsConsent,
  CreateAnalyticsEvent,
  CreateContactRequest,
  CreateRfqRequest,
  ProductFamily,
  SelectorRequest,
} from '@airtek/contracts'
import { createPublicApiClient } from '@/shared/lib/publicApiClient'
import { publicApiBaseUrl } from '@/shared/lib/runtimeConfig'
import type {
  GuestVisitRequest,
  PublishedNewsListQuery,
  PublishedProductListQuery,
  PublishedSearchQuery,
} from '@/shared/lib/publicApiTypes'

export { ApiError as PublicApiError } from '@airtek/contracts'
export { createPublicApiClient } from '@/shared/lib/publicApiClient'
export * from '@/shared/lib/publicApiTypes'

export function submitContact(payload: CreateContactRequest, idempotencyKey: string) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).submitContact(payload, idempotencyKey)
}

export function submitRfq(payload: CreateRfqRequest, idempotencyKey: string) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).submitRfq(payload, idempotencyKey)
}

export function selectProducts(payload: SelectorRequest) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).selectProducts(payload)
}

export function submitAnalyticsEvent(payload: CreateAnalyticsEvent, idempotencyKey: string) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).submitAnalyticsEvent(payload, idempotencyKey)
}

export function submitAnalyticsConsent(payload: CreateAnalyticsConsent) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).submitAnalyticsConsent(payload)
}

export function getPublishedProduct(slug: string, family?: ProductFamily) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).getProduct(slug, family)
}

export function getPublishedProductAssets(slug: string, family?: ProductFamily) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).getProductAssets(slug, family)
}

export function getPublishedProducts(query: PublishedProductListQuery = {}) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).listProducts(query)
}

export function searchPublishedSite(query: PublishedSearchQuery = {}) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).search(query)
}

export function getPublishedNews(query: PublishedNewsListQuery = {}) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).listNews(query)
}

export function recordGuestVisit(payload: GuestVisitRequest) {
  return createPublicApiClient({ baseUrl: publicApiBaseUrl() }).recordGuestVisit(payload)
}

export function newIdempotencyKey() {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return crypto.randomUUID()
  return `request-${Date.now()}-${Math.random().toString(16).slice(2)}`
}
