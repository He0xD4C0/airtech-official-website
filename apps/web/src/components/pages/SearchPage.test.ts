import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { trackAnalyticsEvent } from '@/lib/analytics'
import { searchPublishedSite } from '@/lib/api'
import { publicPageFixture } from '@/test/publicPageFixture'
import SearchPage from './SearchPage.vue'

vi.mock('@/lib/analytics', () => ({ trackAnalyticsEvent: vi.fn().mockResolvedValue(true) }))
vi.mock('@/lib/api', () => ({ searchPublishedSite: vi.fn() }))

const publishedFaq = {
  entityType: 'content' as const,
  entityId: '7cc94f0c-fd80-4c8a-9960-43956576012d',
  title: 'Published FAQ',
  summary: 'Approved summary.',
  canonicalPath: '/en/resources/faqs/published-faq',
  displayType: 'faq' as const,
}

describe('public search analytics', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/en/search')
    vi.mocked(trackAnalyticsEvent).mockClear()
    vi.mocked(searchPublishedSite).mockReset()
    vi.mocked(searchPublishedSite).mockResolvedValue({
      items: [], nextCursor: null, total: 0, typeCounts: [],
    })
  })

  it('reports only query length and result count for an internal search', async () => {
    const wrapper = mount(SearchPage, { props: { page: publicPageFixture('/en/search', { kind: 'search', indexable: false }) } })
    const privateQuery = 'buyer@example.com confidential requirement'
    const input = wrapper.get('input[type="search"]')
    await input.setValue(privateQuery)
    await input.trigger('change')
    await flushPromises()

    expect(searchPublishedSite).toHaveBeenCalledWith({ limit: 20, q: privateQuery })
    expect(trackAnalyticsEvent).toHaveBeenCalledWith('internalSearch', {
      queryLength: privateQuery.length,
      resultCount: 0,
    })
    expect(JSON.stringify(vi.mocked(trackAnalyticsEvent).mock.calls)).not.toContain(privateQuery)
  })

  it('searches and synchronizes the query when the user presses Enter', async () => {
    const wrapper = mount(SearchPage, { props: { page: publicPageFixture('/en/search', { kind: 'search', indexable: false }) } })
    const input = wrapper.get('input[type="search"]')
    await input.setValue('T30D-24-D718N-02')
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()

    expect(searchPublishedSite).toHaveBeenCalledWith({ limit: 20, q: 'T30D-24-D718N-02' })
    expect(new URL(window.location.href).searchParams.get('q')).toBe('T30D-24-D718N-02')
  })

  it('reports the bounded content-type facet without search text', async () => {
    const page = {
      ...publicPageFixture('/en/search', { kind: 'search', indexable: false }),
      searchResults: [publishedFaq],
      searchTotal: 1,
      searchTypeCounts: [{ value: 'faq', count: 1 }],
    }
    vi.mocked(searchPublishedSite).mockResolvedValue({
      items: [publishedFaq], nextCursor: null, total: 1, typeCounts: [{ value: 'faq', count: 1 }],
    })
    const wrapper = mount(SearchPage, { props: { page } })
    await wrapper.get('select').setValue('faq')
    await flushPromises()
    const resultCount = Number.parseInt(wrapper.get('.result-count').text(), 10)

    expect(searchPublishedSite).toHaveBeenCalledWith({ limit: 20, type: 'faq' })
    expect(trackAnalyticsEvent).toHaveBeenCalledWith('filterApplied', {
      filterName: 'contentType',
      resultCount,
    })
  })
})
