import { describe, expect, it } from 'vitest'
import type { PublicContentProjection } from '@airtek/contracts'
import { publicPageFixture } from '@/test/publicPageFixture'
import { publicProjectionFixture, tiptapDocument } from '@/test/publicProjectionFixture'
import { buildPublicStructuredData } from './structuredData'

type FaqItems = Extract<PublicContentProjection['typeFields'], { type: 'faq' }>['items']

function faqProjection(
  items: FaqItems,
  overrides: Partial<PublicContentProjection> = {},
): PublicContentProjection {
  return publicProjectionFixture({
    kind: 'faq',
    templateKey: 'faqDetail',
    slug: 'technical',
    title: 'Technical FAQ',
    typeFields: { type: 'faq', items },
    composition: {
      blocks: [{
        type: 'faqCollection',
        id: '11111111-1111-4111-8111-111111111111',
        heading: 'Questions',
      }],
    },
    ...overrides,
  })
}

function faqItems(answerContent: unknown[] = [
  { type: 'paragraph', content: [{ type: 'text', text: 'Only a complete published answer.' }] },
]): FaqItems {
  return [{
    id: '22222222-2222-4222-8222-222222222222',
    question: 'What is published?',
    answer: tiptapDocument(answerContent),
  }]
}

function graphTypes(value: Record<string, unknown>): string[] {
  return (value['@graph'] as Array<Record<string, unknown>>).map((node) => String(node['@type']))
}

describe('public structured data', () => {
  it('emits FAQPage only from a complete native V2 FAQ projection', () => {
    const projection = faqProjection(faqItems())
    const page = publicPageFixture('/en/resources/faqs/technical', {
      projection, dataState: 'published', indexable: true,
    })
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
    const projection = faqProjection(faqItems(), {
      isPlaceholder: policy.dataState === 'placeholder',
    })
    const page = publicPageFixture('/en/resources/faqs/technical', { ...policy, projection })
    expect(graphTypes(buildPublicStructuredData(page, 'https://www.example.test'))).not.toContain('FAQPage')
  })

  it('does not emit FAQPage when any native V2 question is incomplete', () => {
    const projection = faqProjection([
      ...faqItems(),
      {
        id: '33333333-3333-4333-8333-333333333333',
        question: 'Missing answer?',
        answer: tiptapDocument([]),
      },
    ])
    const page = publicPageFixture('/en/resources/faqs/technical', {
      projection, dataState: 'published', indexable: true,
    })
    expect(graphTypes(buildPublicStructuredData(page, 'https://www.example.test'))).not.toContain('FAQPage')
  })

  it('adds article metadata only from native V2 type fields', () => {
    const projection = publicProjectionFixture({
      kind: 'article',
      templateKey: 'articleDetail',
      slug: 'published-article',
      typeFields: {
        type: 'article',
        authorDisplayName: 'Engineering Team',
        publicationAt: '2026-08-30T00:00:00Z',
        category: 'Engineering notes',
        cover: null,
        featured: false,
      },
    })
    const page = publicPageFixture('/en/resources/articles/published-article', {
      projection, dataState: 'published', indexable: true,
    })
    const graph = buildPublicStructuredData(page, 'https://www.example.test')['@graph'] as Array<Record<string, unknown>>
    const article = graph.find((node) => node['@type'] === 'Article')
    expect(article).toMatchObject({
      datePublished: '2026-08-30T00:00:00Z',
      articleSection: 'Engineering notes',
      author: { '@type': 'Organization', name: 'Engineering Team' },
    })
  })

  it('does not invent an article author when the V2 field is absent', () => {
    const projection = publicProjectionFixture({
      kind: 'article',
      templateKey: 'articleDetail',
      slug: 'published-article',
      typeFields: {
        type: 'article', category: null, authorDisplayName: null,
        publicationAt: null, cover: null, featured: false,
      },
    })
    const page = publicPageFixture('/en/resources/articles/published-article', {
      projection, dataState: 'published', indexable: true,
    })
    const graph = buildPublicStructuredData(page, 'https://www.example.test')['@graph'] as Array<Record<string, unknown>>
    const article = graph.find((node) => node['@type'] === 'Article')
    expect(article).not.toHaveProperty('author')
  })
})
