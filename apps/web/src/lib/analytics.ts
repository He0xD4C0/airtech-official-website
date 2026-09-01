import type { AnalyticsConsentReceipt, CreateAnalyticsEvent, JsonValue } from '@airtek/contracts'
import { submitAnalyticsConsent, submitAnalyticsEvent } from './api'

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
export const analyticsPolicyVersion = 'analytics-v1' as const
let receiptRequest: Promise<AnalyticsConsentReceipt | undefined> | undefined

const allowedProperties = {
  pageView: ['contentKind', 'contentId', 'publishedRevision'],
  internalSearch: ['queryLength', 'resultCount'],
  filterApplied: ['filterName', 'filterValue', 'resultCount'],
  selectorStarted: ['constraintCount', 'preferredFamily', 'priority'],
  selectorStepCompleted: ['step', 'constraintCount'],
  selectorResult: ['outcome', 'candidateCount'],
  compareChanged: ['action', 'itemCount', 'productId', 'productRevision'],
  downloadStarted: ['downloadId', 'productId', 'productRevision'],
  faqExpanded: ['faqId', 'category'],
  ctaClicked: ['ctaId', 'destinationPath', 'placement'],
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

export function sanitizeAnalyticsProperties(
  eventName: AnalyticsEventName,
  properties: Record<string, unknown>,
): Record<string, JsonValue> {
  const allowed = new Set<string>(allowedProperties[eventName])
  const sanitized: Record<string, JsonValue> = {}
  for (const [key, value] of Object.entries(properties)) {
    if (allowed.has(key) && safeValue(value)) sanitized[key] = value
  }
  return sanitized
}

export function analyticsRuntimeDisabled(): boolean {
  const contentPreview = typeof window !== 'undefined' && window.location.pathname === '/en/preview'
  return contentPreview || (import.meta.env.DEV && import.meta.env.VITE_DISABLE_COOKIE_BANNER === 'true')
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
  if (value === 'accepted') return Boolean(await ensureAnalyticsConsentReceipt())

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

export async function trackAnalyticsEvent(
  eventName: AnalyticsEventName,
  properties: Record<string, unknown> = {},
): Promise<boolean> {
  if (typeof window === 'undefined' || analyticsRuntimeDisabled() || currentAnalyticsConsent() !== 'accepted') return false
  const anonymousSessionId = anonymousAnalyticsSessionId()
  if (!anonymousSessionId) return false
  const consentReceipt = await ensureAnalyticsConsentReceipt()
  if (!consentReceipt) return false
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
    return (await submitAnalyticsEvent(event)).accepted
  } catch {
    return false
  }
}

export function trackCurrentPageView(): Promise<boolean> {
  return trackAnalyticsEvent('pageView')
}
