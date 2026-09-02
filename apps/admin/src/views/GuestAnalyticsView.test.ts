import { createSSRApp, defineComponent, h, ref, shallowRef } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it, vi } from 'vitest'

vi.mock('@/composables/useCursorPagination', () => {
  let call = 0
  return {
    useCursorPagination: () => {
      const visits = [{
        bucketDate: '2026-09-02', landingPath: '/en', locale: 'en', visits: 3,
        pageViews: 4, rfqStarts: 1, rfqSubmissions: 0,
      }]
      const sources = [{
        ...visits[0], source: 'referral', sourceName: 'Referral',
        referrerDomain: 'example.test', utmSource: null, medium: null, campaign: null,
      }]
      const items = call++ % 2 === 0 ? visits : sources
      return {
        items: shallowRef(items),
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

async function render(mode: 'visits' | 'sources'): Promise<string> {
  const app = createSSRApp(GuestAnalyticsView, { mode })
  app.component('RouterLink', defineComponent({
    setup(_props, { slots }) {
      return () => h('a', slots.default?.())
    },
  }))
  return renderToString(app)
}

describe('GuestAnalyticsView pagination', () => {
  it('renders server-cursor previous and next controls for both aggregate modes', async () => {
    const visits = await render('visits')
    const sources = await render('sources')

    expect(visits).toContain('第 2 页')
    expect(visits).toContain('条访问聚合')
    expect(visits).toContain('上一页')
    expect(visits).toContain('下一页')
    expect(sources).toContain('第 2 页')
    expect(sources).toContain('条来源聚合')
  })
})
