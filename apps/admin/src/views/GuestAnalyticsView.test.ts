import { createSSRApp, defineComponent, h, ref, shallowRef } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it, vi } from 'vitest'

vi.mock('@/composables/useCursorPagination', () => {
  return {
    useCursorPagination: () => {
      const sources = [{
        bucketDate: '2026-09-02', landingPath: '/en', locale: 'en', visits: 3,
        pageViews: 4, rfqStarts: 1, rfqSubmissions: 0,
        source: 'referral', sourceName: 'Referral',
        referrerDomain: 'example.test', utmSource: null, medium: null, campaign: null,
      }]
      return {
        items: shallowRef(sources),
        nextCursor: shallowRef('opaque-next-page'),
        pageNumber: ref(2),
        canPrevious: ref(true),
        canNext: ref(true),
        loading: shallowRef(false),
        error: shallowRef(null),
        errorStatus: shallowRef(null),
        first: vi.fn(async () => true),
        next: vi.fn(async () => true),
        previous: vi.fn(async () => true),
        refresh: vi.fn(async () => true),
      }
    },
  }
})

import GuestAnalyticsView from './GuestAnalyticsView.vue'

async function render(): Promise<string> {
  const app = createSSRApp(GuestAnalyticsView)
  app.component('RouterLink', defineComponent({
    setup(_props, { slots }) {
      return () => h('a', slots.default?.())
    },
  }))
  return renderToString(app)
}

describe('GuestAnalyticsView pagination', () => {
  it('renders server-cursor previous and next controls for source aggregates', async () => {
    const sources = await render()

    expect(sources).toContain('第 2 页')
    expect(sources).toContain('条来源聚合')
    expect(sources).toContain('上一页')
    expect(sources).toContain('下一页')
  })
})
