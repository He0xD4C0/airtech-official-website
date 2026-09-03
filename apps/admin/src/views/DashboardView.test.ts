// @vitest-environment jsdom

import { defineComponent, h, type Component } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushTestUpdates, mountForTest, testTextContent } from '@/testing/testRenderer'

const mocks = vi.hoisted(() => ({
  overview: vi.fn(),
  legacySummary: vi.fn(),
  toast: vi.fn(),
}))

vi.mock('@/services/adminApi', () => ({
  adminApi: {
    analyticsOverview: mocks.overview,
    analyticsSummary: mocks.legacySummary,
  },
}))

vi.mock('@/stores/auth', () => ({
  useAuthStore: () => ({
    hasPermission: (permission?: string) => permission === 'analytics.read',
  }),
}))

vi.mock('@/stores/ui', () => ({
  useUiStore: () => ({ toast: mocks.toast }),
}))

import DashboardView from './DashboardView.vue'

const RouterLinkStub = defineComponent({
  name: 'RouterLink',
  setup(_props, { slots }) {
    return () => h('a', slots.default?.())
  },
})

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(new Date('2026-09-03T12:00:00Z'))
  mocks.overview.mockReset().mockResolvedValue({
    range: {
      from: '2026-08-05T00:00:00Z',
      toExclusive: '2026-09-04T00:00:00Z',
      timezone: 'UTC',
    },
    consentedMetrics: {
      visits: 31,
      pageViews: 72,
      engagedVisitDays: 15,
      rfqStartEvents: 8,
      rfqSubmitEvents: 4,
    },
    businessOutcomes: { rfqSubmissions: 6, contactRequests: 2 },
    generatedAt: '2026-09-03T11:00:00Z',
    containsPii: false,
    source: 'firstParty',
  })
  mocks.legacySummary.mockClear()
  mocks.toast.mockClear()
})

afterEach(() => vi.useRealTimers())

describe('Dashboard Analytics summary', () => {
  it('uses the new 30-day overview and never calls the legacy summary', async () => {
    const { app, root } = mountForTest(DashboardView, {
      RouterLink: RouterLinkStub as Component,
    })
    await flushTestUpdates()

    expect(mocks.overview).toHaveBeenCalledWith({
      from: '2026-08-05T00:00:00Z',
      to: '2026-09-04T00:00:00Z',
    })
    expect(mocks.legacySummary).not.toHaveBeenCalled()

    const text = testTextContent(root)
    expect(text).toContain('最近 30 个 UTC 日历日')
    expect(text).toContain('已同意访问31')
    expect(text).toContain('页面浏览事件72')
    expect(text).toContain('可归因 RFQ 提交事件4')
    app.unmount()
  })
})
