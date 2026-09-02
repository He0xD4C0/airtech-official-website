import { beforeEach, describe, expect, it, vi } from 'vitest'

const apiMocks = vi.hoisted(() => ({
  submitAnalyticsConsent: vi.fn(),
  submitAnalyticsEvent: vi.fn(),
  recordGuestVisit: vi.fn(),
}))

vi.mock('./api', () => apiMocks)
import {
  analyticsConsentStorageKey,
  analyticsRuntimeDisabled,
  anonymousAnalyticsSessionId,
  currentAnalyticsConsent,
  guestVisitAcquisition,
  sanitizeAnalyticsProperties,
  setCurrentAnalyticsContext,
  setAnalyticsConsent,
  trackCurrentPageView,
  trackAnalyticsEvent,
} from './analytics'

describe('consent-aware analytics', () => {
  beforeEach(() => {
    window.localStorage.clear()
    window.sessionStorage.clear()
    window.history.replaceState({}, '', '/en')
    apiMocks.submitAnalyticsConsent.mockReset().mockImplementation(async (request) => ({
      consentReceipt: crypto.randomUUID(),
      anonymousSessionId: request.anonymousSessionId,
      policyVersion: request.policyVersion,
      analyticsAllowed: request.analyticsAllowed,
      grantedAt: '2026-09-01T08:00:00Z',
      expiresAt: '2999-02-28T08:00:00Z',
    }))
    apiMocks.submitAnalyticsEvent.mockReset().mockResolvedValue({ accepted: true, eventId: crypto.randomUUID() })
    apiMocks.recordGuestVisit.mockReset().mockImplementation(async (request) => ({
      id: crypto.randomUUID(),
      anonymousSessionId: request.anonymousSessionId,
      landingPath: request.landingPath,
      referrerDomain: request.referrerDomain ?? null,
      source: request.source ?? null,
      medium: request.medium ?? null,
      campaign: request.campaign ?? null,
      firstSeenAt: '2026-09-01T08:00:00Z',
      lastSeenAt: '2026-09-01T08:00:00Z',
      retentionUntil: '2027-09-01T08:00:00Z',
    }))
    setCurrentAnalyticsContext(undefined)
  })

  it('disables all analytics on signed content previews', () => {
    window.history.replaceState({}, '', '/en/preview?token=sensitive')
    expect(analyticsRuntimeDisabled()).toBe(true)
  })

  it('keeps an anonymous session and server receipt off until explicit consent', async () => {
    expect(currentAnalyticsConsent()).toBeNull()
    expect(anonymousAnalyticsSessionId()).toBeUndefined()

    await setAnalyticsConsent('accepted')
    expect(window.localStorage.getItem(analyticsConsentStorageKey)).toBe('accepted')
    expect(anonymousAnalyticsSessionId()).toMatch(/^[0-9a-f-]{36}$/u)
    expect(apiMocks.submitAnalyticsConsent).toHaveBeenLastCalledWith(expect.objectContaining({
      policyVersion: 'analytics-v1',
      analyticsAllowed: true,
    }))
    expect(window.sessionStorage.length).toBe(3)
    expect(apiMocks.recordGuestVisit).toHaveBeenCalledWith(expect.objectContaining({
      landingPath: '/en',
      policyVersion: 'analytics-v1',
      consentReceipt: expect.any(String),
    }))

    await setAnalyticsConsent('declined')
    expect(apiMocks.submitAnalyticsConsent).toHaveBeenLastCalledWith(expect.objectContaining({
      analyticsAllowed: false,
    }))
    expect(anonymousAnalyticsSessionId()).toBeUndefined()
    expect(window.sessionStorage.length).toBe(0)
  })

  it('does not request consent or analytics while preview mode disables tracking', async () => {
    window.history.replaceState({}, '', '/en/preview?token=sensitive')
    expect(await setAnalyticsConsent('accepted')).toBe(false)
    expect(apiMocks.submitAnalyticsConsent).not.toHaveBeenCalled()
    expect(apiMocks.submitAnalyticsEvent).not.toHaveBeenCalled()
    expect(window.sessionStorage.length).toBe(0)
  })

  it('binds every accepted event to the current anonymous session and receipt', async () => {
    window.history.replaceState({}, '', '/en/products')
    await setAnalyticsConsent('accepted')
    expect(await trackAnalyticsEvent('filterApplied', {
      filterName: 'family', filterValue: 'axial', resultCount: 2,
    })).toBe(true)
    const consent = apiMocks.submitAnalyticsConsent.mock.results[0]?.value
    await consent
    expect(apiMocks.submitAnalyticsEvent).toHaveBeenCalledWith(expect.objectContaining({
      eventName: 'filterApplied',
      consentGranted: true,
      policyVersion: 'analytics-v1',
      anonymousSessionId: expect.any(String),
      consentReceipt: expect.any(String),
      properties: { filterName: 'family', resultCount: 2 },
    }))
  })

  it('sanitizes acquisition data and omits unapproved UTM or query fields', () => {
    window.history.replaceState({}, '', '/en/resources/news?utm_source=search&utm_medium=email%40example.com&utm_campaign=launch&utm_term=private')
    expect(guestVisitAcquisition()).toEqual({
      landingPath: '/en/resources/news',
      source: 'search',
      campaign: 'launch',
    })
  })

  it('attaches the published projection context to page views without duplicating Guest visits', async () => {
    setCurrentAnalyticsContext({
      contentKind: 'news',
      contentId: '792406a9-2f19-425e-8508-78a205c0c764',
      publishedRevision: 3,
    })
    await setAnalyticsConsent('accepted')
    expect(await trackCurrentPageView()).toBe(true)
    expect(apiMocks.submitAnalyticsEvent).toHaveBeenCalledWith(expect.objectContaining({
      eventName: 'pageView',
      properties: {
        contentKind: 'news',
        contentId: '792406a9-2f19-425e-8508-78a205c0c764',
        publishedRevision: 3,
      },
    }))
    expect(apiMocks.recordGuestVisit).toHaveBeenCalledTimes(1)
  })

  it('uses an event-specific property allowlist and drops likely PII values', () => {
    expect(sanitizeAnalyticsProperties('filterApplied', {
      filterName: 'family',
      filterValue: 'axial',
      resultCount: 3,
      email: 'buyer@example.com',
      fieldName: 'not-allowed-for-this-event',
    })).toEqual({ filterName: 'family', resultCount: 3 })

    expect(sanitizeAnalyticsProperties('ctaClicked', {
      ctaId: 'contact buyer@example.com',
      destinationPath: '/en/request-a-quote',
      placement: 'hero',
    })).toEqual({ placement: 'hero' })
  })

  it('rejects arbitrary text in every identifier-like analytics dimension', () => {
    expect(sanitizeAnalyticsProperties('pageView', { contentKind: 'Jane Doe' })).toEqual({})
    expect(sanitizeAnalyticsProperties('filterApplied', {
      filterName: 'jane-doe', resultCount: 2,
    })).toEqual({ resultCount: 2 })
    expect(sanitizeAnalyticsProperties('faqExpanded', { faqId: 'jane-doe' })).toEqual({})
    expect(sanitizeAnalyticsProperties('ctaClicked', {
      ctaId: 'private-note', placement: 'jane-doe', destinationPath: '/en/jane-doe',
    })).toEqual({})
    expect(sanitizeAnalyticsProperties('rfqValidationError', {
      journey: 'product', step: 1, fieldName: 'jane-doe', errorCode: 'private-note',
    })).toEqual({ journey: 'product', step: 1 })
  })

  it('retains only fixed dictionary values, UUIDs and numeric FAQ identifiers', () => {
    expect(sanitizeAnalyticsProperties('faqExpanded', { faqId: 'faq-12' })).toEqual({ faqId: 'faq-12' })
    expect(sanitizeAnalyticsProperties('ctaClicked', {
      ctaId: 'request-quote', placement: 'product-detail',
    })).toEqual({ ctaId: 'request-quote', placement: 'product-detail' })
    expect(sanitizeAnalyticsProperties('rfqValidationError', {
      journey: 'product', step: 1, fieldName: 'productContext', errorCode: 'publishedContextRequired',
    })).toEqual({
      journey: 'product', step: 1, fieldName: 'productContext', errorCode: 'publishedContextRequired',
    })
  })
})
