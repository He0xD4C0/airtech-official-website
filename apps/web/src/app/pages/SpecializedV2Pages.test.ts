import { createSSRApp, h, type Component } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { ContentBlock, PublicContentProjection } from '@airtek/contracts'
import { publicPageFixture } from '@/shared/test/publicPageFixture'
import type { PageKind, PublicPageModel } from '@/shared/types/content'
import CatalogPage from '@/features/catalog/pages/CatalogPage.vue'
import SelectorPage from '@/features/selector/pages/SelectorPage.vue'
import CollectionPage from '@/features/content/pages/CollectionPage.vue'
import RfqRouterPage from '@/features/conversion/pages/RfqRouterPage.vue'
import RfqFormPage from '@/features/conversion/pages/RfqFormPage.vue'
import SearchPage from '@/features/search/pages/SearchPage.vue'
import ComparePage from '@/features/compare/pages/ComparePage.vue'
import { createPublicTestPlugins } from '@/shared/test/publicAppPlugins'

const mediaId = '11111111-1111-4111-8111-111111111111'
const downloadId = '33333333-3333-4333-8333-333333333333'
const blocks: ContentBlock[] = [
  {
    type: 'media',
    id: mediaId,
    media: {
      asset: { assetId: mediaId },
      altText: 'Published V2 media',
      decorative: false,
    },
    caption: 'V2 media survives the specialized page',
    layout: 'inline',
  },
  {
    type: 'downloadAsset',
    id: downloadId,
    asset: { assetId: downloadId },
    label: 'V2 controlled download',
    description: 'Published asset description',
  },
]

function pageWithProjection(
  path: string,
  kind: PageKind,
  templateKey: PublicContentProjection['templateKey'],
): PublicPageModel {
  const projection: PublicContentProjection = {
    schemaVersion: 2,
    id: '55555555-5555-4555-8555-555555555555',
    kind: 'page',
    templateKey,
    locale: 'en',
    title: 'V2 specialized page',
    summary: 'V2 composition summary',
    slug: null,
    seo: { title: null, description: null, indexable: true, socialImage: null },
    body: null,
    composition: { blocks },
    typeFields: { type: 'page' },
    isPlaceholder: false,
    publishedRevision: 2,
    updatedAt: '2026-09-11T00:00:00Z',
    resolvedRelations: [],
    resolvedLinks: [],
    resolvedMedia: [
      {
        assetId: mediaId,
        publicUrl: `/api/public/v1/media/${mediaId}`,
        previewUrl: `https://media.example.test/${mediaId}.preview.webp`,
        downloadUrl: `/api/public/v1/media/${mediaId}/download`,
        mediaType: 'image/webp', byteSize: 100, originalName: 'media.webp',
        originalWidth: 800, originalHeight: 600, previewWidth: 800, previewHeight: 600,
        previewByteSize: 50,
      },
      {
        assetId: downloadId,
        publicUrl: `/api/public/v1/media/${downloadId}`,
        previewUrl: null,
        downloadUrl: `/api/public/v1/media/${downloadId}/download`,
        mediaType: 'image/png', byteSize: 200, originalName: 'controlled.png',
        originalWidth: null, originalHeight: null, previewWidth: null, previewHeight: null,
        previewByteSize: null,
      },
    ],
  }
  return publicPageFixture(path, {
    kind,
    title: 'Legacy hero must not render',
    projection,
    blocks,
    entries: [{
      slug: 'selection',
      title: 'Selection RFQ functional marker',
      summary: 'Functional list entry',
      href: '/en/request-a-quote/selection',
    }],
    productFamilies: [],
    motorTechnologies: [],
    rfqType: 'selection',
    sections: [{ id: 'legacy-slot', title: 'Legacy slot must not render' }],
    primaryCta: {
      title: 'Legacy CTA must not render',
      description: 'Legacy CTA description',
      href: '/en/company/contact',
      label: 'Legacy CTA',
    },
  })
}

async function renderPage(component: Component, page: PublicPageModel): Promise<string> {
  const app = createSSRApp({ render: () => h(component, { page }) })
  for (const plugin of createPublicTestPlugins()) app.use(plugin)
  return renderToString(app)
}

describe('specialized V2 public pages', () => {
  it('SSR-renders the complete V2 composition and keeps every specialized workspace', async () => {
    const scenarios = [
      {
        name: 'catalog',
        component: CatalogPage,
        page: pageWithProjection('/en/products', 'catalog', 'productIndex'),
        workspace: 'No validated products are available for these filters.',
      },
      {
        name: 'selector',
        component: SelectorPage,
        page: pageWithProjection('/en/products/selector', 'selector', 'selector'),
        workspace: 'Define the duty point',
      },
      {
        name: 'collection',
        component: CollectionPage,
        page: pageWithProjection('/en/resources/articles', 'collection', 'articleIndex'),
        workspace: 'Selection RFQ functional marker',
      },
      {
        name: 'RFQ router',
        component: RfqRouterPage,
        page: pageWithProjection('/en/request-a-quote', 'rfq-router', 'rfqRouter'),
        workspace: 'Selection RFQ functional marker',
      },
      {
        name: 'RFQ form',
        component: RfqFormPage,
        page: pageWithProjection('/en/request-a-quote/selection', 'rfq-form', 'rfqForm'),
        workspace: 'Define the duty point',
      },
      {
        name: 'search',
        component: SearchPage,
        page: pageWithProjection('/en/search', 'search', 'search'),
        workspace: 'Search the public site',
      },
      {
        name: 'compare',
        component: ComparePage,
        page: pageWithProjection('/en/products/compare', 'compare', 'compare'),
        workspace: 'Loading selected published records',
      },
    ] as const

    for (const scenario of scenarios) {
      const html = await renderPage(scenario.component, scenario.page)
      expect(html, scenario.name).toContain('V2 media survives the specialized page')
      expect(html, scenario.name).toContain(`src="https://media.example.test/${mediaId}.preview.webp"`)
      expect(html, scenario.name).toContain('V2 controlled download')
      expect(html, scenario.name).toContain(scenario.workspace)
      expect(html, scenario.name).not.toContain('Legacy hero must not render')
      expect(html, scenario.name).not.toContain('Legacy slot must not render')
      expect(html, scenario.name).not.toContain('Legacy CTA must not render')
    }
  })

  it('does not revive legacy rendering for a V2 projection with no blocks', async () => {
    const page = pageWithProjection('/en/products', 'catalog', 'productIndex')
    page.blocks = []
    page.projection!.composition.blocks = []
    const html = await renderPage(CatalogPage, page)

    expect(html).not.toContain('V2 media survives the specialized page')
    expect(html).not.toContain('V2 controlled download')
    expect(html).not.toContain('Legacy hero must not render')
    expect(html).not.toContain('Legacy slot must not render')
    expect(html).not.toContain('Legacy CTA must not render')
    expect(html).toContain('No validated products are available for these filters.')
  })
})
