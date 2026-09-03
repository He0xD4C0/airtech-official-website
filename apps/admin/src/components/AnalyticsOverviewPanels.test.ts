import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import AnalyticsOverviewPanels from './AnalyticsOverviewPanels.vue'

describe('AnalyticsOverviewPanels', () => {
  it('separates five consented metrics from independent business outcomes', async () => {
    const html = await renderToString(createSSRApp(AnalyticsOverviewPanels, {
      overview: {
        range: {
          from: '2026-08-05T00:00:00Z',
          toExclusive: '2026-09-04T00:00:00Z',
          timezone: 'UTC',
        },
        consentedMetrics: {
          visits: 101,
          pageViews: 220,
          engagedVisitDays: 48,
          rfqStartEvents: 12,
          rfqSubmitEvents: 5,
        },
        businessOutcomes: { rfqSubmissions: 8, contactRequests: 3 },
        generatedAt: '2026-09-03T12:00:00Z',
        containsPii: false,
        source: 'firstParty',
      },
    }))

    for (const label of ['已同意访问', '页面浏览事件', '参与访问日', 'RFQ 开始事件', '可归因 RFQ 提交事件']) {
      expect(html).toContain(label)
    }
    expect(html).toContain('独立业务结果')
    expect(html).toContain('不参与行为漏斗')
    expect(html).toContain('事件写入去重仍待下一增量完成，行为事件数可能受重试影响。')
    expect(html).not.toContain('PII 进入事件')
    expect(html).not.toContain('转化路径')
  })
})
