import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { ContentBlock, PublicContentProjection } from '@airtek/contracts'
import PageRenderer from '@/app/pages/PageRenderer.vue'
import { publicPageFixture } from '@/shared/test/publicPageFixture'
import { publicProjectionFixture, tiptapDocument } from '@/shared/test/publicProjectionFixture'
import type { PublicPageModel } from '@/shared/types/content'
import { createPublicTestPlugins } from '@/shared/test/publicAppPlugins'

interface ProjectionOptions {
  kind?: PublicContentProjection['kind']
  templateKey?: PublicContentProjection['templateKey']
  slug?: string | null
  typeFields?: PublicContentProjection['typeFields']
  extraBlocks?: ContentBlock[]
}

function projectionWithBody(bodyContent: unknown[], options: ProjectionOptions = {}): PublicContentProjection {
  const body = tiptapDocument(bodyContent)
  return publicProjectionFixture({
    kind: options.kind ?? 'page',
    templateKey: options.templateKey ?? 'home',
    slug: options.slug ?? null,
    body,
    composition: {
      blocks: [
        ...(bodyContent.length
          ? [{ type: 'body' as const, id: '11111111-1111-4111-8111-111111111111', width: 'standard' as const }]
          : []),
        ...(options.extraBlocks ?? []),
      ],
    },
    typeFields: options.typeFields ?? { type: 'page' },
  })
}

function pageWithProjection(
  path: string,
  projection: PublicContentProjection,
  overrides: Partial<PublicPageModel> = {},
): PublicPageModel {
  return publicPageFixture(path, {
    projection,
    blocks: projection.composition.blocks,
    dataState: 'published',
    ...overrides,
  })
}

async function render(page: PublicPageModel): Promise<string> {
  const app = createSSRApp({ render: () => h(PageRenderer, { page }) })
  for (const plugin of createPublicTestPlugins()) app.use(plugin)
  return renderToString(app)
}

describe('native V2 CMS page rendering', () => {
  it('composes a Home document with database-projected taxonomy', async () => {
    const projection = projectionWithBody([
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'CMS V2 body wins' }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'Visible without client JavaScript.' }] },
    ], { kind: 'home', templateKey: 'home', typeFields: { type: 'home' } })
    const page = pageWithProjection('/en', projection, {
      productFamilies: [{
        code: 'axial', slug: 'axial', name: 'Database family',
        description: 'Database family description', sortOrder: 1,
      }],
    })
    const html = await render(page)

    expect(html).toContain('CMS V2 body wins')
    expect(html).toContain('Visible without client JavaScript.')
    expect(html).toContain('Product families')
    expect(html).toContain('Database family')
    expect(html).not.toContain('class="home-hero"')
  })

  it('does not resurrect the removed static Home scaffold when V2 composition is empty', async () => {
    const projection = projectionWithBody([], { kind: 'home', templateKey: 'home', typeFields: { type: 'home' } })
    expect(await render(pageWithProjection('/en', projection))).not.toContain('class="home-hero"')
  })

  it('keeps specialized workspaces while rendering their native V2 documents', async () => {
    const scenarios: Array<{
      path: string
      options: ProjectionOptions
      marker: string
    }> = [
      { path: '/en/resources/articles', options: { templateKey: 'articleIndex' }, marker: 'V2 editorial remains visible.' },
      {
        path: '/en/resources/faqs',
        options: { kind: 'faq', templateKey: 'faqIndex', typeFields: { type: 'faq', items: [] } },
        marker: 'FAQ categories',
      },
      {
        path: '/en/resources/downloads',
        options: { kind: 'download', templateKey: 'downloadIndex', typeFields: { type: 'download' } },
        marker: 'Search title or description',
      },
      {
        path: '/en/company/about',
        options: { kind: 'company', templateKey: 'about', typeFields: { type: 'company' } },
        marker: 'V2 editorial remains visible.',
      },
      {
        path: '/en/company/contact',
        options: { kind: 'company', templateKey: 'contact', typeFields: { type: 'company' } },
        marker: 'Business email',
      },
    ]

    for (const scenario of scenarios) {
      const projection = projectionWithBody([
        { type: 'paragraph', content: [{ type: 'text', text: 'V2 editorial remains visible.' }] },
      ], scenario.options)
      const html = await render(pageWithProjection(scenario.path, projection))
      expect(html, scenario.path).toContain('V2 editorial remains visible.')
      expect(html, scenario.path).toContain(scenario.marker)
    }
  })

  it('uses the V2 body as the ordinary detail document', async () => {
    const path = '/en/resources/articles/reading-a-fan-curve'
    const projection = projectionWithBody([
      { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Controlled V2 article body' }] },
    ], {
      kind: 'article', templateKey: 'articleDetail', slug: 'reading-a-fan-curve',
      typeFields: {
        type: 'article', category: null, authorDisplayName: null,
        publicationAt: null, cover: null, featured: false,
      },
    })
    const html = await render(pageWithProjection(path, projection))
    expect(html).toContain('Controlled V2 article body')
    expect(html).not.toContain('Read the axes before the line')
  })

  it('renders a controlled download only from a V2 downloadAsset block', async () => {
    const path = '/en/resources/downloads/approved-resource'
    const assetId = '22222222-2222-4222-8222-222222222222'
    const projection = projectionWithBody([
      { type: 'paragraph', content: [{ type: 'text', text: 'Controlled resource context.' }] },
    ], {
      kind: 'download', templateKey: 'downloadDetail', slug: 'approved-resource',
      typeFields: { type: 'download', resourceType: 'Manual', versionLabel: 'V2', versionNotes: null },
      extraBlocks: [{
        type: 'downloadAsset', id: '33333333-3333-4333-8333-333333333333',
        asset: { assetId },
        label: 'Download resource', description: 'Published file description.',
      }],
    })
    projection.resolvedMedia = [{
      assetId,
      publicUrl: `/api/public/v1/media/${assetId}`,
      previewUrl: null,
      downloadUrl: `/api/public/v1/media/${assetId}/download`,
      mediaType: 'image/webp', byteSize: 200, originalName: 'approved-resource.webp',
      originalWidth: null, originalHeight: null, previewWidth: null, previewHeight: null,
      previewByteSize: null,
    }]
    const html = await render(pageWithProjection(path, projection))
    expect(html).toContain('Controlled resource context.')
    expect(html).toContain('Published file description.')
    expect(html).toContain(`href="http://localhost:8080/api/public/v1/media/${assetId}/download"`)
    expect(html).toContain('Download resource')
  })
})
