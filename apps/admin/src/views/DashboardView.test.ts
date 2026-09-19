// @vitest-environment jsdom

import { defineComponent, h, type Component } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushTestUpdates, mountForTest, testTextContent } from '@/testing/testRenderer'

const mocks = vi.hoisted(() => ({
  dashboardSummary: vi.fn(),
  legacySummary: vi.fn(),
  toast: vi.fn(),
}))

vi.mock('@/services/adminApi', () => ({
  adminApi: {
    dashboardSummary: mocks.dashboardSummary,
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
  mocks.dashboardSummary.mockReset().mockResolvedValue({
    generatedAt: '2026-09-03T11:00:00Z',
    draftContent: { available: true, value: 4 },
    openRfqs: { available: true, value: 6 },
    readinessItemCount: 4,
    recentActivity: [],
    analytics: {
      visits: 31,
      pageViews: 72,
      rfqSubmitEvents: 4,
    },
  })
  mocks.legacySummary.mockClear()
  mocks.toast.mockClear()
})

describe('Dashboard Analytics summary', () => {
  it('uses the permission-aware server summary and never calls the legacy summary', async () => {
    const { app, root } = mountForTest(DashboardView, {
      RouterLink: RouterLinkStub as Component,
    })
    await flushTestUpdates()

    expect(mocks.dashboardSummary).toHaveBeenCalledOnce()
    expect(mocks.legacySummary).not.toHaveBeenCalled()

    const text = testTextContent(root)
    expect(text).toContain('最近 30 个 UTC 日历日')
    expect(text).toContain('已同意访问31')
    expect(text).toContain('页面浏览事件72')
    expect(text).toContain('可归因 RFQ 提交事件4')
    app.unmount()
  })
})
