import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { ContentPreviewResponse } from '@airtek/contracts'
import ContentPreviewPage from './ContentPreviewPage.vue'

describe('content preview page', () => {
  it('SSR-renders an obvious marker and escapes structured draft content', async () => {
    const preview: ContentPreviewResponse = {
      content: {
        schemaVersion: 2,
        id: '77935cef-4111-4c4c-bdb8-17679a8b42fe',
        kind: 'article',
        templateKey: 'articleDetail',
        slug: 'private-preview',
        locale: 'en',
        title: 'Private draft',
        summary: 'Not published.',
        body: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: '<script>alert(1)</script>' }] }] },
        composition: { blocks: [
          { type: 'hero', id: 'hero', eyebrow: 'Preview', heading: 'Private draft', lead: 'Not published.', media: null, actions: [], variant: 'standard' },
          { type: 'body', id: 'body', width: 'standard' },
        ] },
        typeFields: { type: 'article', category: null, authorDisplayName: null, publicationAt: null, cover: null, featured: false },
        seo: { title: null, description: null, indexable: false, socialImage: null },
        isPlaceholder: false,
        publishedRevision: 4,
        updatedAt: '2026-09-01T08:00:00Z',
        resolvedRelations: [],
        resolvedLinks: [],
        resolvedMedia: [],
      },
      previewExpiresAt: '2026-09-01T08:10:00Z',
    }
    const html = await renderToString(createSSRApp({ render: () => h(ContentPreviewPage, { preview }) }))
    expect(html).toContain('Preview · Not published')
    expect(html).toContain('Revision 4 · Draft')
    expect(html).toContain('Private draft')
    expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;')
    expect(html).not.toContain('<script>alert(1)</script>')
  })
})
