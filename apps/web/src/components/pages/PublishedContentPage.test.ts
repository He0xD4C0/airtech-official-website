import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { ContentEntry } from '@airtek/contracts'
import { resolvePublicRoute } from '@/content/routes'
import PublishedContentPage from './PublishedContentPage.vue'

function publishedArticle(attrs?: Record<string, unknown>): ContentEntry {
  return {
    id: '792406a9-2f19-425e-8508-78a205c0c764', kind: 'article', slug: 'visible-metadata', locale: 'en',
    title: 'Visible metadata', summary: 'Published summary.',
    body: { schemaVersion: 1, doc: { type: 'doc', ...(attrs ? { attrs } : {}), content: [
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Operating context' }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'Published body.' }] },
      { type: 'heading', attrs: { level: 3 }, content: [{ type: 'text', text: 'Operating context' }] },
    ] } },
    seo: { title: null, description: null, canonicalPath: '/en/resources/articles/visible-metadata', indexable: true },
    status: 'published', isPlaceholder: false, currentRevision: 2, publishedRevision: 1, scheduledFor: null,
    updatedAt: '2026-09-01T08:00:00Z',
  }
}

async function render(content: ContentEntry): Promise<string> {
  const page = {
    ...resolvePublicRoute('/en/resources/articles/visible-metadata'),
    publishedContent: content, dataState: 'published' as const, indexable: true,
  }
  return renderToString(createSSRApp({ render: () => h(PublishedContentPage, { page }) }))
}

describe('published article metadata', () => {
  it('SSR-renders explicit author, dates, category and a heading-linked table of contents', async () => {
    const html = await render(publishedArticle({
      author: 'Technical Editorial Team', authorType: 'Organization',
      publishedAt: '2026-08-31', category: 'Engineering notes',
    }))
    expect(html).toContain('<dt>Author</dt>')
    expect(html).toContain('Technical Editorial Team')
    expect(html).toContain('<dt>Published</dt>')
    expect(html).toContain('datetime="2026-08-31"')
    expect(html).toContain('<dt>Last updated</dt>')
    expect(html).toContain('<dt>Category</dt>')
    expect(html).toContain('aria-label="On this page"')
    expect(html).toContain('href="#operating-context"')
    expect(html).toContain('href="#operating-context-2"')
    expect(html).toContain('<h2 id="operating-context">Operating context</h2>')
    expect(html).toContain('<h3 id="operating-context-2">Operating context</h3>')
  })

  it('omits absent author, publication date and category without inventing fallbacks', async () => {
    const html = await render(publishedArticle())
    expect(html).not.toContain('<dt>Author</dt>')
    expect(html).not.toContain('<dt>Published</dt>')
    expect(html).not.toContain('<dt>Category</dt>')
    expect(html).toContain('<dt>Last updated</dt>')
  })
})
