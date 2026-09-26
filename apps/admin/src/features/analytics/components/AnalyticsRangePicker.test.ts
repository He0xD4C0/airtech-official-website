import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import AnalyticsRangePicker from '@/features/analytics/components/AnalyticsRangePicker.vue'

describe('AnalyticsRangePicker', () => {
  it('renders accessible absolute UTC presets and active state', async () => {
    const html = await renderToString(createSSRApp(AnalyticsRangePicker, {
      range: { from: '2026-08-05', to: '2026-09-03' },
      activePreset: 30,
    }))

    expect(html).toContain('Analytics 日期范围')
    expect(html).toContain('最近 7 天')
    expect(html).toContain('最近 30 天')
    expect(html).toContain('最近 90 天')
    expect(html).toContain('aria-pressed="true"')
    expect(html).toContain('含首含尾 · UTC')
  })
})
