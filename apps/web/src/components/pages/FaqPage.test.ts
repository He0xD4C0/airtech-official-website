import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { ContentEntry } from '@airtek/contracts'
import { resolvePublicRoute } from '@/content/routes'
import FaqPage from './FaqPage.vue'

function publishedFaq(): ContentEntry {
  return {
    id: '95e418fc-6b52-45b4-b868-c340c0133ec8', kind: 'faq', slug: 'technical', locale: 'en',
    title: 'Technical FAQ', summary: 'Published questions.', body: { schemaVersion: 1, doc: { type: 'doc', content: [
      { type: 'paragraph', content: [{ type: 'text', text: 'Published introduction.' }] },
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Which context is visible?' }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'Only the answer stored in this published body.' }] },
    ] } },
    seo: { title: null, description: null, canonicalPath: '/en/resources/faqs/technical', indexable: true },
    status: 'published', isPlaceholder: false, currentRevision: 2, publishedRevision: 1, scheduledFor: null,
    updatedAt: '2026-09-01T08:00:00Z',
  }
}

describe('published FAQ page', () => {
  it('SSR-renders only published-body questions, answers and editorial introduction', async () => {
    const page = {
      ...resolvePublicRoute('/en/resources/faqs/technical'), publishedContent: publishedFaq(),
      dataState: 'published' as const, indexable: true,
    }
    const html = await renderToString(createSSRApp({ render: () => h(FaqPage, { page }) }))
    expect(html).toContain('Published introduction.')
    expect(html).toContain('<details>')
    expect(html).toContain('Which context is visible?')
    expect(html).toContain('Only the answer stored in this published body.')
    expect(html).not.toContain('Why do operating conditions matter for a PQ curve?')
  })
})
