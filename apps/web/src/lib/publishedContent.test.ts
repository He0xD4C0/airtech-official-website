import { describe, expect, it } from 'vitest'
import type { ContentEntry, RichTextDocument } from '@airtek/contracts'
import {
  extractArticleOutline,
  extractPublishedArticleMetadata,
  extractPublishedFaqContent,
} from './publishedContent'

function document(content: unknown[], attrs?: Record<string, unknown>): RichTextDocument {
  return { schemaVersion: 1, doc: { type: 'doc', ...(attrs ? { attrs } : {}), content } }
}

function article(body: RichTextDocument, updatedAt = '2026-09-01T08:00:00Z'): ContentEntry {
  return {
    id: '8ea14e88-a075-4559-b758-a23f22ecb7e6',
    kind: 'article', slug: 'safe-article', locale: 'en', title: 'Safe article', summary: null,
    body, seo: { title: null, description: null, canonicalPath: '/en/resources/articles/safe-article', indexable: true },
    status: 'published', isPlaceholder: false, currentRevision: 2, publishedRevision: 1,
    scheduledFor: null, updatedAt,
  }
}

describe('published FAQ extraction', () => {
  it('derives visible complete pairs from the published body and preserves editorial copy separately', () => {
    const result = extractPublishedFaqContent(document([
      { type: 'paragraph', content: [{ type: 'text', text: 'Published FAQ introduction.' }] },
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'What data is required?' }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'Provide the declared operating context.' }] },
      { type: 'bulletList', content: [{ type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Include units.' }] }] }] },
      { type: 'heading', attrs: { level: 3 }, content: [{ type: 'text', text: 'When is engineering review needed?' }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'Use review when required data is incomplete.' }] },
    ]))

    expect(result.complete).toBe(true)
    expect(result.items).toHaveLength(2)
    expect(result.items[0]).toMatchObject({
      id: 'faq-1', question: 'What data is required?',
      answer: 'Provide the declared operating context. Include units.',
    })
    expect(JSON.stringify(result.editorialDocument)).toContain('Published FAQ introduction.')
    expect(JSON.stringify(result.editorialDocument)).not.toContain('What data is required?')
  })

  it('withholds FAQ completeness when any published question lacks a textual answer', () => {
    const result = extractPublishedFaqContent(document([
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Complete question?' }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'Complete answer.' }] },
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Incomplete question?' }] },
      { type: 'image', attrs: { src: 'https://cdn.example.test/context.webp', alt: 'Context only' } },
    ]))
    expect(result.items).toHaveLength(1)
    expect(result.complete).toBe(false)
  })
})

describe('published article presentation data', () => {
  it('accepts explicit safe metadata and creates stable unique heading anchors', () => {
    const body = document([
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Read the data' }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'Body.' }] },
      { type: 'heading', attrs: { level: 3 }, content: [{ type: 'text', text: 'Read the data' }] },
    ], {
      author: 'Technical Editorial Team', authorType: 'Organization',
      publishedAt: '2026-08-31T09:30:00+08:00', category: 'Engineering notes',
    })
    expect(extractPublishedArticleMetadata(article(body))).toEqual({
      author: 'Technical Editorial Team', authorType: 'Organization',
      publishedAt: '2026-08-31T09:30:00+08:00', publishedDate: '2026-08-31',
      category: 'Engineering notes', updatedAt: '2026-09-01T08:00:00Z', updatedDate: '2026-09-01',
      outline: [
        { id: 'read-the-data', title: 'Read the data', level: 2 },
        { id: 'read-the-data-2', title: 'Read the data', level: 3 },
      ],
    })
    expect(extractArticleOutline(body).map((item) => item.id)).toEqual(['read-the-data', 'read-the-data-2'])
  })

  it('omits invalid or absent editorial facts rather than deriving them from route labels', () => {
    const metadata = extractPublishedArticleMetadata(article(document([], {
      author: 'Unsafe\u0000name', authorType: 'Team', publishedAt: '2026-02-30', category: '',
    }), 'not-a-date'))
    expect(metadata).toEqual({ outline: [] })
  })
})
