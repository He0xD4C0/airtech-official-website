import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { trackAnalyticsEvent } from '@/features/analytics'
import { publicProjectionFixture } from '@/shared/test/publicProjectionFixture'
import type { PublicPageModel } from '@/shared/types/content'
import DownloadsPage from '@/features/content/pages/DownloadsPage.vue'
import { createPublicTestPlugins } from '@/shared/test/publicAppPlugins'

vi.mock('@/features/analytics/lib/analytics', () => ({ trackAnalyticsEvent: vi.fn().mockResolvedValue(true) }))

const page: PublicPageModel = {
  kind: 'downloads',
  canonicalPath: '/en/resources/downloads',
  title: 'Downloads',
  metaTitle: 'Downloads | AIRTEKPOWER',
  description: 'Controlled resources.',
  eyebrow: 'Controlled resources',
  breadcrumbs: [],
  indexable: true,
  projection: publicProjectionFixture({
    kind: 'download',
    templateKey: 'downloadIndex',
    typeFields: { type: 'download' },
  }),
  entries: [{
    slug: 'approved-record',
    title: 'Approved record',
    summary: 'A published controlled-resource record.',
    href: '/en/resources/downloads/approved-record',
  }, {
    slug: 'manual-record', title: 'Manual record', summary: 'A published manual.',
    href: '/en/resources/downloads/manual-record',
  }],
}

describe('downloads analytics', () => {
  beforeEach(() => {
    vi.mocked(trackAnalyticsEvent).mockClear()
  })

  it('reports only query length and result count for resource search', async () => {
    const wrapper = mount(DownloadsPage, { props: { page }, global: { plugins: createPublicTestPlugins() } })
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
    const wrapper = mount(DownloadsPage, { props: { page }, global: { plugins: createPublicTestPlugins() } })
    const link = wrapper.get('.collection-card h2 a')
    link.element.addEventListener('click', (event) => event.preventDefault())
    await link.trigger('click')

    expect(trackAnalyticsEvent).toHaveBeenCalledWith('ctaClicked', {
      ctaId: 'download-record-open',
      placement: 'downloads-list',
    })
    expect(trackAnalyticsEvent).not.toHaveBeenCalledWith('downloadStarted', expect.anything())
  })

  it('filters only on fields present in the V2 discovery contract', async () => {
    const wrapper = mount(DownloadsPage, { props: { page }, global: { plugins: createPublicTestPlugins() } })
    expect(wrapper.findAll('select')).toHaveLength(0)
    const input = wrapper.get('input[type="search"]')
    await input.setValue('published manual')
    expect(wrapper.text()).toContain('Manual record')
    expect(wrapper.text()).not.toContain('Approved record')
    expect(wrapper.text()).not.toContain('Applicable model')
    expect(wrapper.text()).not.toContain('Resource type')
  })
})
