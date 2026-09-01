import { beforeEach, describe, expect, it, vi } from 'vitest'

const apiMocks = vi.hoisted(() => ({
  submitAnalyticsConsent: vi.fn(),
  submitAnalyticsEvent: vi.fn(),
}))

vi.mock('./api', () => apiMocks)
import {
  analyticsConsentStorageKey,
  analyticsRuntimeDisabled,
  anonymousAnalyticsSessionId,
  currentAnalyticsConsent,
  sanitizeAnalyticsProperties,
  setAnalyticsConsent,
  trackAnalyticsEvent,
} from './analytics'

describe('consent-aware analytics', () => {
  beforeEach(() => {
    window.localStorage.clear()
    window.sessionStorage.clear()
    window.history.replaceState({}, '', '/')
    apiMocks.submitAnalyticsConsent.mockReset().mockImplementation(async (request) => ({
      consentReceipt: crypto.randomUUID(),
      anonymousSessionId: request.anonymousSessionId,
      policyVersion: request.policyVersion,
      analyticsAllowed: request.analyticsAllowed,
      grantedAt: '2026-09-01T08:00:00Z',
      expiresAt: '2999-02-28T08:00:00Z',
    }))
    apiMocks.submitAnalyticsEvent.mockReset().mockResolvedValue({ accepted: true, eventId: crypto.randomUUID() })
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
    expect(window.sessionStorage.length).toBe(2)

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
    }))
  })

  it('uses an event-specific property allowlist and drops likely PII values', () => {
    expect(sanitizeAnalyticsProperties('filterApplied', {
      filterName: 'family',
      filterValue: 'axial',
      resultCount: 3,
      email: 'buyer@example.com',
      fieldName: 'not-allowed-for-this-event',
    })).toEqual({ filterName: 'family', filterValue: 'axial', resultCount: 3 })

    expect(sanitizeAnalyticsProperties('ctaClicked', {
      ctaId: 'contact buyer@example.com',
      destinationPath: '/en/request-a-quote',
      placement: 'hero',
    })).toEqual({ destinationPath: '/en/request-a-quote', placement: 'hero' })
  })
})
