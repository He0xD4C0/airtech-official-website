import type {
  CreateAnalyticsConsent,
  CreateAnalyticsEvent,
  CreateContactRequest,
  CreateRfqRequest,
  ProductFamily,
  SelectorRequest,
} from '@airtek/contracts'
import { createPublicApiClient } from './publicApiClient'
import type {
  GuestVisitRequest,
  PublishedNewsListQuery,
  PublishedProductListQuery,
} from './publicApiTypes'

export { ApiError as PublicApiError } from '@airtek/contracts'
export { createPublicApiClient } from './publicApiClient'
export * from './publicApiTypes'

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

export function submitAnalyticsEvent(payload: CreateAnalyticsEvent, idempotencyKey: string) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).submitAnalyticsEvent(payload, idempotencyKey)
}

export function submitAnalyticsConsent(payload: CreateAnalyticsConsent) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).submitAnalyticsConsent(payload)
}

export function getPublishedProduct(slug: string, family?: ProductFamily) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).getProduct(slug, family)
}

export function getPublishedProductAssets(slug: string, family?: ProductFamily) {
  return createPublicApiClient({ baseUrl: browserApiBaseUrl() }).getProductAssets(slug, family)
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
