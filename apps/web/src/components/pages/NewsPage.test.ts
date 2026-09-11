import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { PublicContentProjection } from '@airtek/contracts'
import { getPublishedNews } from '@/lib/api'
import { publicPageFixture } from '@/test/publicPageFixture'
import NewsPage from './NewsPage.vue'

vi.mock('@/lib/api', () => ({ getPublishedNews: vi.fn() }))

function news(slug: string, title: string): PublicContentProjection {
  return {
    id: crypto.randomUUID(),
    schemaVersion: 2,
    kind: 'news',
    templateKey: 'newsDetail',
    slug,
    locale: 'en',
    title,
    summary: `${title} summary`,
    body: { type: 'doc', content: [] },
    composition: { blocks: [] },
    typeFields: {
      type: 'news', category: 'Company', authorDisplayName: 'Editorial', cover: null,
      publicationAt: '2026-09-02T00:00:00Z', featured: false,
    },
    seo: { title, description: `${title} summary`, indexable: true, socialImage: null },
    isPlaceholder: false,
    publishedRevision: 1,
    updatedAt: '2026-09-02T00:00:00Z',
    resolvedRelations: [],
    resolvedLinks: [],
  }
}

describe('published News cursor pagination', () => {
  beforeEach(() => {
    vi.mocked(getPublishedNews).mockReset()
  })

  it('SSR-renders the first page and exposes the next cursor without JavaScript', async () => {
    const page = {
      ...publicPageFixture('/en/resources/news'),
      kind: 'news' as const,
      entries: [{ slug: 'first', title: 'First published News', summary: '', href: '/en/resources/news/first' }],
      newsNextCursor: 'next-news-cursor',
    }
    const html = await renderToString(createSSRApp({ render: () => h(NewsPage, { page }) }))

    expect(html).toContain('First published News')
    expect(html).toContain('Next page')
  })

  it('loads the next cursor page instead of silently truncating the News index', async () => {
    vi.mocked(getPublishedNews).mockResolvedValue({
      items: [{
        content: news('second', 'Second published News'),
        category: 'Company',
        authorDisplayName: 'Editorial',
        coverMediaId: null,
        publishedAt: '2026-09-02T00:00:00Z',
        featured: false,
        dataClass: 'editorial',
      }],
      nextCursor: null,
    })
    const page = {
      ...publicPageFixture('/en/resources/news'),
      kind: 'news' as const,
      entries: [{ slug: 'first', title: 'First published News', summary: '', href: '/en/resources/news/first' }],
      newsNextCursor: 'next-news-cursor',
    }
    const wrapper = mount(NewsPage, { props: { page } })

    const nextButton = wrapper.findAll('button').find((button) => button.text() === 'Next page')
    expect(nextButton).toBeDefined()
    await nextButton!.trigger('click')
    await flushPromises()

    expect(getPublishedNews).toHaveBeenCalledWith({ locale: 'en', limit: 48, cursor: 'next-news-cursor' })
    expect(wrapper.text()).toContain('Second published News')
    expect(wrapper.text()).toContain('Page 2')
  })
})
