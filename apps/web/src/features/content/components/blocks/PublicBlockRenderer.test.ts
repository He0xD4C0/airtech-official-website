import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { ContentBlock } from '@airtek/contracts'
import PublicBlockRenderer from '@/features/content/components/blocks/PublicBlockRenderer.vue'
import { createPublicTestPlugins } from '@/shared/test/publicAppPlugins'
import { linkTargetHref, type PublicContentProjection } from '@/shared/types/projection'

const heroId = '11111111-1111-4111-8111-111111111111'
const bodyId = '22222222-2222-4222-8222-222222222222'
const gridId = '33333333-3333-4333-8333-333333333333'
const evidenceId = '44444444-4444-4444-8444-444444444444'
const ctaId = '55555555-5555-4555-8555-555555555555'
const relationId = '66666666-6666-4666-8666-666666666666'
const faqId = '77777777-7777-4777-8777-777777777777'
const mediaId = '88888888-8888-4888-8888-888888888888'
const downloadId = '99999999-9999-4999-8999-999999999999'
const contactId = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'
const relationCardId = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb'
const developmentBodyCopy = 'This development fixture verifies database-backed rendering. Replace it with reviewed editorial content before launch.'

const blocks: ContentBlock[] = [
  {
    type: 'hero',
    id: heroId,
    eyebrow: 'Platform',
    heading: 'Published hero',
    lead: 'Hero lead',
    media: null,
    actions: [{ label: 'Contact us', target: { targetType: 'content', contentId: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc' } }],
    variant: 'standard',
  },
  { type: 'body', id: bodyId, width: 'standard' },
  { type: 'media', id: mediaId, media: { asset: { assetId: mediaId }, altText: 'Fan photo', decorative: false }, caption: 'Media caption', layout: 'inline' },
  { type: 'featureGrid', id: gridId, heading: 'Capabilities', items: [{ id: gridId, title: 'Airflow', description: 'Feature copy', icon: null }] },
  { type: 'evidence', id: evidenceId, heading: 'Evidence', items: [{ id: evidenceId, label: 'Tested', statement: 'Verified statement', sourceNote: 'Report 2026' }] },
  { type: 'cta', id: ctaId, eyebrow: 'Next', heading: 'Talk to engineering', body: 'CTA body', action: { label: 'Request a quote', target: { targetType: 'route', path: '/en/request-a-quote' } }, variant: 'standard' },
  { type: 'relationCollection', id: relationId, heading: 'Related', relationIds: [relationCardId], presentation: 'cards' },
  { type: 'faqCollection', id: faqId, heading: 'FAQ' },
  { type: 'downloadAsset', id: downloadId, asset: { assetId: downloadId }, label: 'Download image', description: 'Published image' },
  { type: 'contactBlock', id: contactId, heading: 'Contact', channels: ['email', 'phone'], action: { label: 'Write to us', target: { targetType: 'route', path: '/en/company/contact' } } },
]

const projection: PublicContentProjection = {
  schemaVersion: 2,
  id: 'dddddddd-dddd-4ddd-8ddd-dddddddddddd',
  kind: 'page',
  templateKey: 'productIndex',
  locale: 'en',
  title: 'Projection title',
  summary: 'Projection summary',
  slug: 'products',
  seo: { title: null, description: null, indexable: true, socialImage: null },
  body: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: developmentBodyCopy }] }] },
  composition: { blocks },
  typeFields: {
    type: 'faq',
    items: [{ id: faqId, question: 'What is airflow?', answer: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Answer copy' }] }] } }],
  },
  isPlaceholder: false,
  publishedRevision: 3,
  updatedAt: '2026-09-10T00:00:00Z',
  resolvedRelations: [{
    relationId: relationCardId,
    entityType: 'content',
    title: 'Related article',
    summary: 'Related summary',
    href: '/en/resources/articles/related',
    eyebrow: 'Article',
    tags: [],
  }],
  resolvedLinks: [{
    contentId: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc',
    href: '/en/company/contact',
  }],
  resolvedMedia: [
    {
      assetId: mediaId,
      publicUrl: `/api/public/v1/media/${mediaId}`,
      previewUrl: `https://media.example.test/${mediaId}.preview.webp`,
      downloadUrl: `/api/public/v1/media/${mediaId}/download`,
      mediaType: 'image/webp', byteSize: 1024, originalName: 'fan.webp',
      originalWidth: 1800, originalHeight: 1200, previewWidth: 1600, previewHeight: 1067,
      previewByteSize: 512,
    },
    {
      assetId: downloadId,
      publicUrl: `/api/public/v1/media/${downloadId}`,
      previewUrl: null,
      downloadUrl: `/api/public/v1/media/${downloadId}/download`,
      mediaType: 'image/png', byteSize: 2048, originalName: 'diagram.png',
      originalWidth: null, originalHeight: null, previewWidth: null, previewHeight: null,
      previewByteSize: null,
    },
  ],
}

async function render(): Promise<string> {
  const app = createSSRApp({
    render: () => h(PublicBlockRenderer, { blocks, projection }),
  })
  for (const plugin of createPublicTestPlugins()) app.use(plugin)
  return renderToString(app)
}

describe('PublicBlockRenderer', () => {
  it('renders every V2 block kind from the published projection', async () => {
    const html = await render()
    expect(html).toContain('Published hero')
    expect(html).toContain('href="/en/company/contact"')
    expect(html).toContain(developmentBodyCopy)
    expect(html).toContain(`src="https://media.example.test/${mediaId}.preview.webp"`)
    expect(html).not.toContain(`src="http://localhost:8080/api/public/v1/media/${mediaId}"`)
    expect(html).toContain('Media caption')
    expect(html).toContain('Airflow')
    expect(html).toContain('Verified statement')
    expect(html).toContain('Talk to engineering')
    expect(html).toContain('Related article')
    expect(html).toContain('What is airflow?')
    expect(html).toContain('Answer copy')
    expect(html).toContain('href="http://localhost:8080/api/public/v1/media/99999999-9999-4999-8999-999999999999/download"')
    expect(html).toContain('Download image')
    expect(html).toContain('Email')
    expect(html).toContain('Write to us')
  })

  it('resolves content link targets through the server-provided link map', () => {
    expect(linkTargetHref(
      { targetType: 'content', contentId: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc' },
      projection.resolvedLinks,
    )).toBe('/en/company/contact')
    expect(linkTargetHref({ targetType: 'route', path: '/en/products' }, [])).toBe('/en/products')
    expect(linkTargetHref({ targetType: 'content', contentId: 'missing' }, [])).toBeUndefined()
  })
})
