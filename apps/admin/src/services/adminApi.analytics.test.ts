import { describe, expect, it, vi } from 'vitest'
import { adminApi } from './adminApi'
import { response, setupAdminApiTestEnvironment } from './adminApi.testSupport'

setupAdminApiTestEnvironment()

describe('admin analytics API', () => {
  it('reads only aggregate analytics contracts without a visitor identifier', async () => {
    const source = { bucketDate: '2026-09-02', landingPath: '/en', locale: 'en', visits: 3, pageViews: 8, rfqStarts: 1, rfqSubmissions: 0, source: 'referral', sourceName: 'Referral', referrerDomain: 'example.test', utmSource: null, medium: null, campaign: null }
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      void input
      return response({ items: [source], nextCursor: null })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.listGuestSources({ cursor: 'source-cursor', limit: 10 })).resolves.toEqual({ items: [source], nextCursor: null })
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain('/analytics/sources?cursor=source-cursor&limit=10')
    expect(JSON.stringify(fetchMock.mock.calls)).not.toContain('anonymousSessionId')
  })

  it('requests the Analytics overview with an explicit half-open UTC range', async () => {
    const overview = {
      range: {
        from: '2026-08-05T00:00:00Z',
        toExclusive: '2026-09-04T00:00:00Z',
        timezone: 'UTC',
      },
      consentedMetrics: {
        visits: 12,
        pageViews: 24,
        engagedVisitDays: 7,
        rfqStartEvents: 2,
        rfqSubmitEvents: 1,
      },
      businessOutcomes: { rfqSubmissions: 2, contactRequests: 1 },
      generatedAt: '2026-09-03T12:00:00Z',
      containsPii: false,
      source: 'firstParty',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      void input
      return response(overview)
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.analyticsOverview({
      from: '2026-08-05T00:00:00Z',
      to: '2026-09-04T00:00:00Z',
    })).resolves.toEqual(overview)

    const url = String(fetchMock.mock.calls[0]?.[0])
    expect(url).toContain('/api/admin/v1/analytics/overview?')
    expect(url).toContain('from=2026-08-05T00%3A00%3A00Z')
    expect(url).toContain('to=2026-09-04T00%3A00%3A00Z')
    expect(JSON.stringify(fetchMock.mock.calls)).not.toContain('anonymousSessionId')
  })
})
