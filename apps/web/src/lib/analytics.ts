import type { AnalyticsConsentReceipt, CreateAnalyticsEvent, JsonValue } from '@airtek/contracts'
import { newIdempotencyKey, recordGuestVisit, submitAnalyticsConsent, submitAnalyticsEvent } from './api'
import type { PublicAnalyticsContext } from '@/types/content'

export type AnalyticsEventName =
  | 'pageView'
  | 'internalSearch'
  | 'filterApplied'
  | 'selectorStarted'
  | 'selectorStepCompleted'
  | 'selectorResult'
  | 'compareChanged'
  | 'downloadStarted'
  | 'faqExpanded'
  | 'ctaClicked'
  | 'rfqRouteSelected'
  | 'rfqStarted'
  | 'rfqStepCompleted'
  | 'rfqValidationError'
  | 'rfqSubmitted'
  | 'rfqSubmitFailed'
export type AnalyticsConsent = 'accepted' | 'declined'

export const analyticsConsentStorageKey = 'airtek.public.analytics-consent.v1'
const analyticsSessionStorageKey = 'airtek.public.analytics-session.v1'
const analyticsReceiptStorageKey = 'airtek.public.analytics-receipt.v1'
const analyticsVisitStorageKey = 'airtek.public.analytics-visit.v1'
export const analyticsPolicyVersion = 'analytics-v1' as const
let receiptRequest: Promise<AnalyticsConsentReceipt | undefined> | undefined
let currentPageContext: PublicAnalyticsContext | undefined

const allowedProperties = {
  pageView: ['contentKind', 'contentId', 'publishedRevision'],
  internalSearch: ['queryLength', 'resultCount'],
  filterApplied: ['filterName', 'resultCount'],
  selectorStarted: ['constraintCount', 'preferredFamily', 'priority'],
  selectorStepCompleted: ['step', 'constraintCount'],
  selectorResult: ['outcome', 'candidateCount'],
  compareChanged: ['action', 'itemCount', 'productId', 'productRevision'],
  downloadStarted: ['downloadId', 'productId', 'productRevision'],
  faqExpanded: ['faqId'],
  ctaClicked: ['ctaId', 'placement'],
  rfqRouteSelected: ['journey'],
  rfqStarted: ['journey', 'productId', 'productRevision'],
  rfqStepCompleted: ['journey', 'step'],
  rfqValidationError: ['journey', 'step', 'fieldName', 'errorCode'],
  rfqSubmitted: ['journey'],
  rfqSubmitFailed: ['journey', 'errorCode'],
} satisfies Record<AnalyticsEventName, readonly string[]>

function safeValue(value: unknown): value is JsonValue {
  if (value === null || typeof value === 'boolean') return true
  if (typeof value === 'number') return Number.isFinite(value)
  if (typeof value !== 'string' || value.length > 160) return false
  if (/\b[^\s@]+@[^\s@]+\.[^\s@]+\b/u.test(value)) return false
  if (/\+?\d[\d\s().-]{7,}\d/u.test(value)) return false
  return true
}

const controlledAnalyticsValues = {
  contentKind: ['content', 'news', 'product'],
  filterName: [
    'resourceType',
    'applicableModel',
    'contentType',
    'catalogSearch',
    'family',
    'motorTechnology',
    'catalogFilters',
    'catalogPagination',
  ],
  preferredFamily: ['open', 'centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'],
  priority: ['efficiency', 'noise', 'size', 'headroom'],
  outcome: ['matched', 'noValidatedCandidates', 'engineeringReviewRequired'],
  action: ['add', 'remove', 'clear'],
  ctaId: ['content-primary', 'download-record-open', 'request-quote'],
  placement: ['content-panel', 'downloads-list', 'product-detail', 'hero'],
  journey: ['product', 'selection', 'project', 'replacement'],
  fieldName: ['productContext'],
  errorCode: ['publishedContextRequired', 'apiRejected'],
} as const

function controlledValue(key: keyof typeof controlledAnalyticsValues, value: string): boolean {
  return (controlledAnalyticsValues[key] as readonly string[]).includes(value)
}

function uuidValue(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/iu.test(value)
}

function faqIdValue(value: string): boolean {
  return uuidValue(value) || /^faq-[1-9][0-9]{0,5}$/u.test(value)
}

function analyticsPropertyValue(
  eventName: AnalyticsEventName,
  key: string,
  value: unknown,
): value is JsonValue {
  if (typeof value === 'number') {
    if (!Number.isSafeInteger(value)) return false
    if (key === 'queryLength') return value >= 1 && value <= 500
    if (['resultCount', 'candidateCount', 'constraintCount'].includes(key)) return value >= 0 && value <= 1_000_000
    if (key === 'itemCount') return value >= 0 && value <= 4
    if (key === 'step') return value >= 1 && value <= 20
    if (key === 'publishedRevision' || key === 'productRevision') return value >= 1
    return false
  }
  if (typeof value !== 'string' || !safeValue(value)) return false
  if (key === 'contentId' || key === 'productId' || key === 'downloadId') return uuidValue(value)
  if (key === 'faqId') return faqIdValue(value)
  if (key === 'action') return eventName === 'compareChanged' && controlledValue('action', value)
  if (key in controlledAnalyticsValues) {
    return controlledValue(key as keyof typeof controlledAnalyticsValues, value)
  }
  return false
}

export function sanitizeAnalyticsProperties(
  eventName: AnalyticsEventName,
  properties: Record<string, unknown>,
): Record<string, JsonValue> {
  const allowed = new Set<string>(allowedProperties[eventName])
  const sanitized: Record<string, JsonValue> = {}
  for (const [key, value] of Object.entries(properties)) {
    if (allowed.has(key) && analyticsPropertyValue(eventName, key, value)) sanitized[key] = value
  }
  return sanitized
}

export function analyticsRuntimeDisabled(): boolean {
  return import.meta.env.DEV && import.meta.env.VITE_DISABLE_COOKIE_BANNER === 'true'
}

export function currentAnalyticsConsent(): AnalyticsConsent | null {
  if (typeof window === 'undefined') return null
  const saved = window.localStorage.getItem(analyticsConsentStorageKey)
  return saved === 'accepted' || saved === 'declined' ? saved : null
}

export async function setAnalyticsConsent(value: AnalyticsConsent): Promise<boolean> {
  if (typeof window === 'undefined') return false
  window.localStorage.setItem(analyticsConsentStorageKey, value)
  window.dispatchEvent(new CustomEvent('airtek:consent', { detail: value }))
  if (analyticsRuntimeDisabled()) {
    clearAnalyticsSession()
    return false
  }
  if (value === 'accepted') {
    const receipt = await ensureAnalyticsConsentReceipt()
    return Boolean(receipt && await ensureGuestVisit(receipt))
  }

  // Make the local opt-out effective before awaiting the network. If an allow
  // request is already in flight, let it settle and immediately supersede it
  // with a denial for the same anonymous session.
  if (receiptRequest) await receiptRequest.catch(() => undefined)
  const anonymousSessionId = window.sessionStorage.getItem(analyticsSessionStorageKey)
  try {
    if (anonymousSessionId) {
      await submitAnalyticsConsent({
        anonymousSessionId,
        policyVersion: analyticsPolicyVersion,
        analyticsAllowed: false,
      })
    }
  } catch {
    return false
  } finally {
    clearAnalyticsSession()
  }
  return true
}

export function anonymousAnalyticsSessionId(): string | undefined {
  if (typeof window === 'undefined' || currentAnalyticsConsent() !== 'accepted' || analyticsRuntimeDisabled()) return undefined
  const existing = window.sessionStorage.getItem(analyticsSessionStorageKey)
  if (existing) return existing
  const created = window.crypto.randomUUID()
  window.sessionStorage.setItem(analyticsSessionStorageKey, created)
  return created
}

function clearAnalyticsSession(): void {
  if (typeof window === 'undefined') return
  window.sessionStorage.removeItem(analyticsSessionStorageKey)
  window.sessionStorage.removeItem(analyticsReceiptStorageKey)
  window.sessionStorage.removeItem(analyticsVisitStorageKey)
}

function storedAnalyticsReceipt(sessionId: string): AnalyticsConsentReceipt | undefined {
  try {
    const raw = window.sessionStorage.getItem(analyticsReceiptStorageKey)
    const receipt = raw ? JSON.parse(raw) as Partial<AnalyticsConsentReceipt> : undefined
    if (!receipt
      || receipt.analyticsAllowed !== true
      || receipt.policyVersion !== analyticsPolicyVersion
      || receipt.anonymousSessionId !== sessionId
      || typeof receipt.consentReceipt !== 'string'
      || typeof receipt.expiresAt !== 'string'
      || Date.parse(receipt.expiresAt) <= Date.now()) return undefined
    return receipt as AnalyticsConsentReceipt
  } catch {
    return undefined
  }
}

export async function ensureAnalyticsConsentReceipt(): Promise<AnalyticsConsentReceipt | undefined> {
  if (typeof window === 'undefined' || analyticsRuntimeDisabled() || currentAnalyticsConsent() !== 'accepted') return undefined
  const anonymousSessionId = anonymousAnalyticsSessionId()
  if (!anonymousSessionId) return undefined
  const stored = storedAnalyticsReceipt(anonymousSessionId)
  if (stored) return stored
  if (receiptRequest) return receiptRequest

  receiptRequest = submitAnalyticsConsent({
    anonymousSessionId,
    policyVersion: analyticsPolicyVersion,
    analyticsAllowed: true,
  }).then((receipt) => {
    if (receipt.analyticsAllowed
      && receipt.policyVersion === analyticsPolicyVersion
      && receipt.anonymousSessionId === anonymousSessionId
      && Date.parse(receipt.expiresAt) > Date.now()) {
      window.sessionStorage.setItem(analyticsReceiptStorageKey, JSON.stringify(receipt))
      return receipt
    }
    return undefined
  }).catch(() => undefined).finally(() => {
    receiptRequest = undefined
  })
  return receiptRequest
}

function sanitizedAttributionValue(value: string | null): string | undefined {
  if (!value) return undefined
  const normalized = value.normalize('NFKC').trim()
  if (!normalized || normalized.length > 120 || /[\u0000-\u001f\u007f]/u.test(normalized) || !safeValue(normalized)) return undefined
  return normalized
}

export function guestVisitAcquisition(): {
  landingPath: string
  referrerDomain?: string
  source?: string
  medium?: string
  campaign?: string
} | undefined {
  if (typeof window === 'undefined') return undefined
  const landingPath = window.location.pathname
  if (landingPath.length > 512
    || (landingPath !== '/en' && !landingPath.startsWith('/en/'))
    || /[?#\u0000-\u001f\u007f]/u.test(landingPath)) return undefined

  let referrerDomain: string | undefined
  if (document.referrer) {
    try {
      const referrer = new URL(document.referrer)
      const hostname = referrer.hostname.toLowerCase()
      if (hostname
        && hostname !== window.location.hostname.toLowerCase()
        && hostname.length <= 253
        && /^[a-z0-9.-]+$/u.test(hostname)) referrerDomain = hostname
    } catch {
      // Invalid referrers are omitted rather than copied into analytics storage.
    }
  }

  const query = new URLSearchParams(window.location.search)
  return {
    landingPath,
    ...(referrerDomain ? { referrerDomain } : {}),
    ...(sanitizedAttributionValue(query.get('utm_source')) ? { source: sanitizedAttributionValue(query.get('utm_source')) } : {}),
    ...(sanitizedAttributionValue(query.get('utm_medium')) ? { medium: sanitizedAttributionValue(query.get('utm_medium')) } : {}),
    ...(sanitizedAttributionValue(query.get('utm_campaign')) ? { campaign: sanitizedAttributionValue(query.get('utm_campaign')) } : {}),
  }
}

export async function ensureGuestVisit(receipt?: AnalyticsConsentReceipt): Promise<boolean> {
  if (typeof window === 'undefined' || analyticsRuntimeDisabled() || currentAnalyticsConsent() !== 'accepted') return false
  const activeReceipt = receipt ?? await ensureAnalyticsConsentReceipt()
  if (!activeReceipt?.analyticsAllowed) return false
  if (window.sessionStorage.getItem(analyticsVisitStorageKey) === activeReceipt.anonymousSessionId) return true
  const acquisition = guestVisitAcquisition()
  if (!acquisition) return false
  try {
    await recordGuestVisit({
      anonymousSessionId: activeReceipt.anonymousSessionId,
      consentReceipt: activeReceipt.consentReceipt,
      policyVersion: analyticsPolicyVersion,
      ...acquisition,
    })
    window.sessionStorage.setItem(analyticsVisitStorageKey, activeReceipt.anonymousSessionId)
    return true
  } catch {
    return false
  }
}

export function setCurrentAnalyticsContext(value?: PublicAnalyticsContext): void {
  if (typeof window === 'undefined') return
  currentPageContext = value
}

export async function trackAnalyticsEvent(
  eventName: AnalyticsEventName,
  properties: Record<string, unknown> = {},
): Promise<boolean> {
  if (typeof window === 'undefined' || analyticsRuntimeDisabled() || currentAnalyticsConsent() !== 'accepted') return false
  const anonymousSessionId = anonymousAnalyticsSessionId()
  if (!anonymousSessionId) return false
  const consentReceipt = await ensureAnalyticsConsentReceipt()
  if (!consentReceipt) return false
  if (!await ensureGuestVisit(consentReceipt)) return false
  const event: CreateAnalyticsEvent = {
    eventName,
    anonymousSessionId,
    sourcePath: window.location.pathname,
    locale: 'en',
    consentGranted: true,
    policyVersion: analyticsPolicyVersion,
    consentReceipt: consentReceipt.consentReceipt,
    properties: sanitizeAnalyticsProperties(eventName, properties),
  }
  try {
    return (await submitAnalyticsEvent(event, newIdempotencyKey())).accepted
  } catch {
    return false
  }
}

export function trackCurrentPageView(): Promise<boolean> {
  return trackAnalyticsEvent('pageView', currentPageContext ? { ...currentPageContext } : {})
}
