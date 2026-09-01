import { createSSRApp, h } from 'vue'
import { mount } from '@vue/test-utils'
import { renderToString } from 'vue/server-renderer'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ContentEntry } from '@airtek/contracts'
import { trackAnalyticsEvent } from '@/lib/analytics'
import DownloadResourcePanel from './DownloadResourcePanel.vue'
import type { PublicPageModel } from '@/types/content'

vi.mock('@/lib/analytics', () => ({ trackAnalyticsEvent: vi.fn().mockResolvedValue(true) }))

function content(downloadUrl = '/media/public/resource.pdf', fileStatus: Record<string, unknown> = { scan: 'clean', access: 'public' }): ContentEntry {
  return {
    id: '06bcc3a7-1efe-4760-97b5-d64e1223fc03', kind: 'download', slug: 'resource', locale: 'en',
    title: 'Published resource', summary: 'Controlled file.', body: { schemaVersion: 1, doc: { type: 'doc', attrs: {
      version: 'V2', applicableModels: ['MODEL-01'], resourceType: 'Manual',
      fileDescription: 'Approved resource description.', downloadUrl, fileStatus,
    }, content: [] } },
    seo: { title: null, description: null, canonicalPath: '/en/resources/downloads/resource', indexable: true },
    status: 'published', isPlaceholder: false, currentRevision: 2, publishedRevision: 1,
    scheduledFor: null, updatedAt: '2026-09-01T08:00:00Z',
  }
}

function page(publishedContent = content()): PublicPageModel {
  return {
    kind: 'detail', collection: 'downloads', canonicalPath: '/en/resources/downloads/resource',
    title: 'Published resource', metaTitle: 'Published resource | AIRTEKPOWER', description: 'Controlled file.',
    eyebrow: 'Controlled resource', breadcrumbs: [], indexable: true, dataState: 'published', publishedContent,
  }
}

describe('published download resource panel', () => {
  beforeEach(() => vi.mocked(trackAnalyticsEvent).mockClear())

  it('SSR-renders explicit file metadata and a safe clean/public download', async () => {
    const html = await renderToString(createSSRApp({ render: () => h(DownloadResourcePanel, { page: page() }) }))
    expect(html).toContain('<dt>Resource type</dt>')
    expect(html).toContain('Manual')
    expect(html).toContain('<dt>Version</dt>')
    expect(html).toContain('MODEL-01')
    expect(html).toContain('Approved resource description.')
    expect(html).toContain('href="/media/public/resource.pdf"')
    expect(html).toContain('>Download resource</a>')
  })

  it('records only the controlled record ID when a real download begins', async () => {
    const wrapper = mount(DownloadResourcePanel, { props: { page: page() } })
    const link = wrapper.get('a.button')
    link.element.addEventListener('click', (event) => event.preventDefault())
    await link.trigger('click')
    expect(trackAnalyticsEvent).toHaveBeenCalledWith('downloadStarted', {
      downloadId: '06bcc3a7-1efe-4760-97b5-d64e1223fc03',
    })
    expect(JSON.stringify(vi.mocked(trackAnalyticsEvent).mock.calls)).not.toContain('resource.pdf')
  })

  it('SSR-renders an honest unavailable state and no link for an unsafe or unclean file', async () => {
    const unsafe = content('javascript:alert(1)', { scan: 'pending', access: 'public' })
    const html = await renderToString(createSSRApp({ render: () => h(DownloadResourcePanel, { page: page(unsafe) }) }))
    expect(html).toContain('File unavailable')
    expect(html).toContain('A clean file status has not been published')
    expect(html).not.toContain('href="javascript:')
    expect(html).not.toContain('>Download resource</a>')
  })

  it('adds external-link protection to an explicit HTTPS resource', async () => {
    const html = await renderToString(createSSRApp({
      render: () => h(DownloadResourcePanel, { page: page(content('https://cdn.example.test/public/resource.pdf')) }),
    }))
    expect(html).toContain('href="https://cdn.example.test/public/resource.pdf"')
    expect(html).toContain('rel="noopener noreferrer"')
  })
})
