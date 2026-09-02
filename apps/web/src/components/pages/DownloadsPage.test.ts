import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { trackAnalyticsEvent } from '@/lib/analytics'
import type { PublicPageModel } from '@/types/content'
import DownloadsPage from './DownloadsPage.vue'

vi.mock('@/lib/analytics', () => ({ trackAnalyticsEvent: vi.fn().mockResolvedValue(true) }))

const page: PublicPageModel = {
  kind: 'downloads',
  canonicalPath: '/en/resources/downloads',
  title: 'Downloads',
  metaTitle: 'Downloads | AIRTEKPOWER',
  description: 'Controlled resources.',
  eyebrow: 'Controlled resources',
  breadcrumbs: [],
  indexable: true,
  entries: [{
    slug: 'approved-record',
    title: 'Approved record',
    summary: 'A published controlled-resource record.',
    href: '/en/resources/downloads/approved-record',
    download: { resourceType: 'Datasheet', version: 'V2', applicableModels: ['MODEL-A'], fileDescription: 'Published file.' },
  }, {
    slug: 'manual-record', title: 'Manual record', summary: 'A published manual.',
    href: '/en/resources/downloads/manual-record',
    download: { resourceType: 'Manual', version: 'V1', applicableModels: ['MODEL-B'], fileDescription: 'Controlled manual.' },
  }],
}

describe('downloads analytics', () => {
  beforeEach(() => {
    vi.mocked(trackAnalyticsEvent).mockClear()
  })

  it('reports only query length and result count for resource search', async () => {
    const wrapper = mount(DownloadsPage, { props: { page } })
    const privateQuery = 'confidential requested filename.pdf'
    const input = wrapper.get('input[type="search"]')
    await input.setValue(privateQuery)
    await input.trigger('change')

    expect(trackAnalyticsEvent).toHaveBeenCalledWith('internalSearch', {
      queryLength: privateQuery.length,
      resultCount: 0,
    })
    expect(JSON.stringify(vi.mocked(trackAnalyticsEvent).mock.calls)).not.toContain(privateQuery)
  })

  it('tracks a detail entrance as a CTA and does not claim a file download', async () => {
    const wrapper = mount(DownloadsPage, { props: { page } })
    const link = wrapper.get('.collection-card h2 a')
    link.element.addEventListener('click', (event) => event.preventDefault())
    await link.trigger('click')

    expect(trackAnalyticsEvent).toHaveBeenCalledWith('ctaClicked', {
      ctaId: 'download-record-open',
      placement: 'downloads-list',
    })
    expect(trackAnalyticsEvent).not.toHaveBeenCalledWith('downloadStarted', expect.anything())
  })

  it('filters only by published type and model metadata and records filter analytics', async () => {
    const wrapper = mount(DownloadsPage, { props: { page } })
    const selects = wrapper.findAll('select')
    await selects[0].setValue('Manual')
    expect(wrapper.text()).toContain('Manual record')
    expect(wrapper.text()).not.toContain('Approved record')
    expect(trackAnalyticsEvent).toHaveBeenLastCalledWith('filterApplied', {
      filterName: 'resourceType', resultCount: 1,
    })

    await selects[0].setValue('all')
    await selects[1].setValue('MODEL-A')
    expect(wrapper.text()).toContain('Approved record')
    expect(wrapper.text()).not.toContain('Manual record')
    expect(trackAnalyticsEvent).toHaveBeenLastCalledWith('filterApplied', {
      filterName: 'applicableModel', resultCount: 1,
    })
  })
})
