// @vitest-environment jsdom

import { defineComponent, h, type Component } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  findTestElements,
  flushTestUpdates,
  mountForTest,
  testTextContent,
  type TestElement,
} from '@/testing/testRenderer'

type Query = Record<string, string | undefined>

const mocks = vi.hoisted(() => ({
  route: null as { query: Query } | null,
  replace: vi.fn(),
  overview: vi.fn(),
  toast: vi.fn(),
}))

vi.mock('vue-router', async (importOriginal) => {
  const actual = await importOriginal<typeof import('vue-router')>()
  const { reactive } = await import('vue')
  const route = reactive<{ query: Query }>({ query: {} })
  mocks.route = route
  return {
    ...actual,
    useRoute: () => route,
    useRouter: () => ({
      replace: async (location: { query: Query }) => {
        mocks.replace(location)
        route.query = location.query
      },
    }),
  }
})

vi.mock('@/services/adminApi', () => ({
  adminApi: { analyticsOverview: mocks.overview },
}))

vi.mock('@/stores/ui', () => ({
  useUiStore: () => ({ toast: mocks.toast }),
}))

import AnalyticsView from './AnalyticsView.vue'

const NOW = new Date('2026-09-03T12:00:00Z')
const VALID_QUERY = { from: '2026-08-05', to: '2026-09-03' }
const RouterLinkStub = defineComponent({
  name: 'RouterLink',
  setup(_props, { slots }) {
    return () => h('a', slots.default?.())
  },
})

function overview(overrides: {
  visits?: number
  pageViews?: number
  engagedVisitDays?: number
  rfqStartEvents?: number
  rfqSubmitEvents?: number
  rfqSubmissions?: number
  contactRequests?: number
} = {}) {
  return {
    range: {
      from: '2026-08-05T00:00:00Z',
      toExclusive: '2026-09-04T00:00:00Z',
      timezone: 'UTC',
    },
    consentedMetrics: {
      visits: overrides.visits ?? 11,
      pageViews: overrides.pageViews ?? 22,
      engagedVisitDays: overrides.engagedVisitDays ?? 8,
      rfqStartEvents: overrides.rfqStartEvents ?? 3,
      rfqSubmitEvents: overrides.rfqSubmitEvents ?? 2,
    },
    businessOutcomes: {
      rfqSubmissions: overrides.rfqSubmissions ?? 4,
      contactRequests: overrides.contactRequests ?? 1,
    },
    generatedAt: '2026-09-03T11:00:00Z',
    containsPii: false,
    source: 'firstParty',
  }
}

function mountAnalytics(query: Query = VALID_QUERY): { app: { unmount: () => void }; root: TestElement } {
  if (!mocks.route) throw new Error('Router mock was not initialized.')
  mocks.route.query = query
  return mountForTest(AnalyticsView, { RouterLink: RouterLinkStub as Component })
}

function presetButton(root: TestElement, days: number): TestElement {
  const label = `最近 ${days} 天`
  const button = findTestElements(root, 'button')
    .find((candidate) => testTextContent(candidate).trim() === label)
  if (!button) throw new Error(`Missing preset button: ${label}`)
  return button
}

async function click(node: TestElement): Promise<void> {
  const handler = node.props.onClick
  if (typeof handler !== 'function') throw new Error('Element does not have a click handler.')
  await handler()
  await flushTestUpdates()
}

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(NOW)
  mocks.replace.mockClear()
  mocks.overview.mockReset().mockResolvedValue(overview())
  mocks.toast.mockClear()
})

afterEach(() => vi.useRealTimers())

describe('AnalyticsView query and request behavior', () => {
  it.each([
    ['missing', {}],
    ['invalid', { from: '2026-09-01', to: '2026-09-04' }],
  ])('normalizes %s initial query to an absolute 30-day UTC range', async (_case, query) => {
    const { app } = mountAnalytics(query)
    await flushTestUpdates()

    expect(mocks.replace).toHaveBeenCalledWith({ query: VALID_QUERY })
    expect(mocks.overview).toHaveBeenCalledWith({
      from: '2026-08-05T00:00:00Z',
      to: '2026-09-04T00:00:00Z',
    })
    app.unmount()
  })

  it('updates the URL and requests a half-open range when a preset changes', async () => {
    const { app, root } = mountAnalytics()
    await flushTestUpdates()
    mocks.replace.mockClear()
    mocks.overview.mockClear()

    await click(presetButton(root, 7))

    expect(mocks.replace).toHaveBeenCalledWith({
      query: { from: '2026-08-28', to: '2026-09-03' },
    })
    expect(mocks.overview).toHaveBeenCalledWith({
      from: '2026-08-28T00:00:00Z',
      to: '2026-09-04T00:00:00Z',
    })

    mocks.replace.mockClear()
    mocks.overview.mockClear()
    await click(presetButton(root, 7))
    expect(mocks.replace).not.toHaveBeenCalled()
    expect(mocks.overview).toHaveBeenCalledWith({
      from: '2026-08-28T00:00:00Z',
      to: '2026-09-04T00:00:00Z',
    })
    app.unmount()
  })
})

describe('AnalyticsView states', () => {
  it('renders ready metrics and independent business outcomes', async () => {
    const { app, root } = mountAnalytics()
    await flushTestUpdates()
    const text = testTextContent(root)

    expect(text).toContain('已同意访问11')
    expect(text).toContain('页面浏览事件22')
    expect(text).toContain('可归因 RFQ 提交事件2')
    expect(text).toContain('独立业务结果')
    expect(text).toContain('事件写入去重仍待下一增量完成')
    app.unmount()
  })

  it('renders the consented-empty state without hiding business outcomes', async () => {
    mocks.overview.mockResolvedValue(overview({
      visits: 0,
      pageViews: 0,
      engagedVisitDays: 0,
      rfqStartEvents: 0,
      rfqSubmitEvents: 0,
      rfqSubmissions: 6,
    }))
    const { app, root } = mountAnalytics()
    await flushTestUpdates()
    const text = testTextContent(root)

    expect(text).toContain('所选范围暂无已同意行为数据')
    expect(text).toContain('RFQ 提交记录6')
    app.unmount()
  })

  it('renders API error and forbidden states', async () => {
    mocks.overview.mockRejectedValueOnce(new Error('Overview temporarily unavailable'))
    const errorView = mountAnalytics()
    await flushTestUpdates()
    expect(testTextContent(errorView.root)).toContain('Overview temporarily unavailable')
    errorView.app.unmount()

    mocks.overview.mockRejectedValueOnce({ status: 403 })
    const forbiddenView = mountAnalytics()
    await flushTestUpdates()
    expect(testTextContent(forbiddenView.root)).toContain('没有访问权限')
    forbiddenView.app.unmount()
  })
})
