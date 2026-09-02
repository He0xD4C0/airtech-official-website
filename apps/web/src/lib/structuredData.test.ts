import { describe, expect, it } from 'vitest'
import type { ContentEntry } from '@airtek/contracts'
import { publicPageFixture } from '@/test/publicPageFixture'
import { buildPublicStructuredData } from './structuredData'

function content(kind: ContentEntry['kind'], bodyContent: unknown[], attrs?: Record<string, unknown>): ContentEntry {
  const slug = kind === 'faq' ? 'technical' : 'published-article'
  const canonicalPath = kind === 'faq' ? '/en/resources/faqs/technical' : '/en/resources/articles/published-article'
  return {
    id: '09738e61-263b-4330-8c76-0fb9d1e280a4', kind, slug, locale: 'en',
    title: kind === 'faq' ? 'Technical FAQ' : 'Published article', summary: 'Published summary.',
    body: { schemaVersion: 1, doc: { type: 'doc', ...(attrs ? { attrs } : {}), content: bodyContent } },
    seo: { title: null, description: null, canonicalPath, indexable: true }, status: 'published',
    isPlaceholder: false, currentRevision: 2, publishedRevision: 1, scheduledFor: null,
    updatedAt: '2026-09-01T08:00:00Z',
  }
}

function graphTypes(value: Record<string, unknown>): string[] {
  return (value['@graph'] as Array<Record<string, unknown>>).map((node) => String(node['@type']))
}

describe('public structured data', () => {
  const faqBody = [
    { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'What is published?' }] },
    { type: 'paragraph', content: [{ type: 'text', text: 'Only a complete published answer.' }] },
  ]

  it('emits FAQPage only for a complete, indexable, non-placeholder published FAQ', () => {
    const page = {
      ...publicPageFixture('/en/resources/faqs/technical'),
      publishedContent: content('faq', faqBody), dataState: 'published' as const, indexable: true,
    }
    const schema = buildPublicStructuredData(page, 'https://www.example.test')
    expect(graphTypes(schema)).toContain('FAQPage')
    const faq = (schema['@graph'] as Array<Record<string, unknown>>).find((node) => node['@type'] === 'FAQPage')
    expect(faq?.mainEntity).toEqual([{
      '@type': 'Question', name: 'What is published?',
      acceptedAnswer: { '@type': 'Answer', text: 'Only a complete published answer.' },
    }])
  })

  it.each([
    ['placeholder', { dataState: 'placeholder' as const, indexable: true }],
    ['non-indexable', { dataState: 'published' as const, indexable: false }],
  ])('does not emit FAQPage for a %s page', (_label, policy) => {
    const page = { ...publicPageFixture('/en/resources/faqs/technical'), ...policy, publishedContent: content('faq', faqBody) }
    expect(graphTypes(buildPublicStructuredData(page, 'https://www.example.test'))).not.toContain('FAQPage')
  })

  it('does not emit FAQPage when one question is incomplete', () => {
    const incomplete = [...faqBody, { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Missing answer?' }] }]
    const page = {
      ...publicPageFixture('/en/resources/faqs/technical'), dataState: 'published' as const, indexable: true,
      publishedContent: content('faq', incomplete),
    }
    expect(graphTypes(buildPublicStructuredData(page, 'https://www.example.test'))).not.toContain('FAQPage')
  })

  it('does not emit FAQPage for content explicitly marked as placeholder', () => {
    const page = {
      ...publicPageFixture('/en/resources/faqs/technical'), dataState: 'published' as const, indexable: true,
      publishedContent: { ...content('faq', faqBody), isPlaceholder: true },
    }
    expect(graphTypes(buildPublicStructuredData(page, 'https://www.example.test'))).not.toContain('FAQPage')
  })

  it('adds only explicit safe article publication metadata', () => {
    const page = {
      ...publicPageFixture('/en/resources/articles/published-article'), dataState: 'published' as const, indexable: true,
      publishedContent: content('article', [
        { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Evidence' }] },
      ], { author: 'Engineering Team', authorType: 'Organization', publishedAt: '2026-08-30', category: 'Engineering notes' }),
    }
    const graph = buildPublicStructuredData(page, 'https://www.example.test')['@graph'] as Array<Record<string, unknown>>
    const article = graph.find((node) => node['@type'] === 'Article')
    expect(article).toMatchObject({
      datePublished: '2026-08-30', articleSection: 'Engineering notes',
      author: { '@type': 'Organization', name: 'Engineering Team' },
    })
  })

  it('does not invent a structured author type when only an author label is present', () => {
    const page = {
      ...publicPageFixture('/en/resources/articles/published-article'), dataState: 'published' as const, indexable: true,
      publishedContent: content('article', [], { author: 'Unclassified editorial byline' }),
    }
    const graph = buildPublicStructuredData(page, 'https://www.example.test')['@graph'] as Array<Record<string, unknown>>
    const article = graph.find((node) => node['@type'] === 'Article')
    expect(article).not.toHaveProperty('author')
  })
})
