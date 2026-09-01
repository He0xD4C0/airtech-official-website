import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { trackAnalyticsEvent } from '@/lib/analytics'
import { resolvePublicRoute } from '@/content/routes'
import SearchPage from './SearchPage.vue'

vi.mock('@/lib/analytics', () => ({ trackAnalyticsEvent: vi.fn().mockResolvedValue(true) }))

describe('public search analytics', () => {
  beforeEach(() => {
    vi.mocked(trackAnalyticsEvent).mockClear()
  })

  it('reports only query length and result count for an internal search', async () => {
    const wrapper = mount(SearchPage, { props: { page: resolvePublicRoute('/en/search') } })
    const privateQuery = 'buyer@example.com confidential requirement'
    const input = wrapper.get('input[type="search"]')
    await input.setValue(privateQuery)
    await input.trigger('change')

    expect(trackAnalyticsEvent).toHaveBeenCalledWith('internalSearch', {
      queryLength: privateQuery.length,
      resultCount: 0,
    })
    expect(JSON.stringify(vi.mocked(trackAnalyticsEvent).mock.calls)).not.toContain(privateQuery)
  })

  it('reports the bounded content-type facet without search text', async () => {
    const page = {
      ...resolvePublicRoute('/en/search'),
      entries: [{ slug: 'published-faq', title: 'Published FAQ', summary: 'Approved summary.', eyebrow: 'FAQ', href: '/en/resources/faqs/published-faq' }],
    }
    const wrapper = mount(SearchPage, { props: { page } })
    await wrapper.get('select').setValue('FAQ')
    const resultCount = Number.parseInt(wrapper.get('.result-count').text(), 10)

    expect(trackAnalyticsEvent).toHaveBeenCalledWith('filterApplied', {
      filterName: 'contentType',
      filterValue: 'FAQ',
      resultCount,
    })
  })
})
