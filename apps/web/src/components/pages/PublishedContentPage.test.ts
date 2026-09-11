import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { PublicContentProjection } from '@airtek/contracts'
import { publicPageFixture } from '@/test/publicPageFixture'
import { publicProjectionFixture, tiptapDocument } from '@/test/publicProjectionFixture'
import PublishedContentPage from './PublishedContentPage.vue'

function articleProjection(
  typeFields: Extract<PublicContentProjection['typeFields'], { type: 'article' }>,
): PublicContentProjection {
  const body = tiptapDocument([
    { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Operating context' }] },
    { type: 'paragraph', content: [{ type: 'text', text: 'Published V2 body.' }] },
    { type: 'heading', attrs: { level: 3 }, content: [{ type: 'text', text: 'Operating context' }] },
  ])
  return publicProjectionFixture({
    kind: 'article',
    templateKey: 'articleDetail',
    slug: 'visible-metadata',
    title: 'Visible metadata',
    body,
    composition: {
      blocks: [{ type: 'body', id: '11111111-1111-4111-8111-111111111111', width: 'standard' }],
    },
    typeFields,
  })
}

async function render(projection: PublicContentProjection): Promise<string> {
  const page = publicPageFixture('/en/resources/articles/visible-metadata', {
    projection, dataState: 'published', indexable: true,
  })
  return renderToString(createSSRApp({ render: () => h(PublishedContentPage, { page }) }))
}

describe('native V2 article metadata', () => {
  it('SSR-renders explicit author, dates, category and a heading-linked table of contents', async () => {
    const html = await render(articleProjection({
      type: 'article',
      authorDisplayName: 'Technical Editorial Team',
      publicationAt: '2026-08-31T00:00:00Z',
      category: 'Engineering notes',
      cover: null,
      featured: false,
    }))
    expect(html).toContain('<dt>Author</dt>')
    expect(html).toContain('Technical Editorial Team')
    expect(html).toContain('<dt>Published</dt>')
    expect(html).toContain('datetime="2026-08-31T00:00:00Z"')
    expect(html).toContain('<dt>Last updated</dt>')
    expect(html).toContain('<dt>Category</dt>')
    expect(html).toContain('aria-label="On this page"')
    expect(html).toContain('href="#operating-context"')
    expect(html).toContain('href="#operating-context-2"')
    expect(html).toContain('<h2 id="operating-context">Operating context</h2>')
    expect(html).toContain('<h3 id="operating-context-2">Operating context</h3>')
  })

  it('omits absent author, publication date and category without inventing fallbacks', async () => {
    const html = await render(articleProjection({
      type: 'article', category: null, authorDisplayName: null,
      publicationAt: null, cover: null, featured: false,
    }))
    expect(html).not.toContain('<dt>Author</dt>')
    expect(html).not.toContain('<dt>Published</dt>')
    expect(html).not.toContain('<dt>Category</dt>')
    expect(html).toContain('<dt>Last updated</dt>')
    expect(html).toContain('Published V2 body.')
  })
})
