import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import type { ContentEntry } from '@airtek/contracts'
import PageRenderer from './PageRenderer.vue'
import { resolvePublicRoute } from '@/content/routes'

function publishedContent(
  bodyContent: unknown[],
  options: { kind?: ContentEntry['kind']; slug?: string; canonicalPath?: string; attrs?: Record<string, unknown> } = {},
): ContentEntry {
  return {
    id: 'c8e0af27-6335-4e4d-b25d-a45867ed66f9',
    kind: options.kind ?? 'home',
    slug: options.slug ?? 'home',
    locale: 'en',
    title: 'Published home',
    summary: 'Published summary',
    body: { schemaVersion: 1, doc: { type: 'doc', ...(options.attrs ? { attrs: options.attrs } : {}), content: bodyContent } as never },
    seo: { title: 'Published page | AIRTEKPOWER', description: 'Published summary', canonicalPath: options.canonicalPath ?? '/en', indexable: true },
    status: 'published',
    isPlaceholder: false,
    currentRevision: 3,
    publishedRevision: 2,
    scheduledFor: null,
    updatedAt: '2026-09-01T08:00:00Z',
  }
}

describe('published CMS page rendering', () => {
  it('composes a non-empty Home body without removing the core discovery modules', async () => {
    const page = {
      ...resolvePublicRoute('/en'),
      title: 'Published home',
      publishedContent: publishedContent([
        { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'CMS body wins' }] },
        { type: 'paragraph', content: [{ type: 'text', text: 'Visible without client JavaScript.' }] },
      ]),
      dataState: 'published' as const,
    }
    const html = await renderToString(createSSRApp({ render: () => h(PageRenderer, { page }) }))

    expect(html).toContain('CMS body wins')
    expect(html).toContain('Visible without client JavaScript.')
    expect(html).toContain('Turn airflow requirements into a clear engineering path.')
    expect(html).toContain('Product families')
    expect(html).toContain('Open Fan Selector')
  })

  it('keeps the safe static scaffold when the published body is empty', async () => {
    const page = { ...resolvePublicRoute('/en'), publishedContent: publishedContent([]), dataState: 'published' as const }
    const html = await renderToString(createSSRApp({ render: () => h(PageRenderer, { page }) }))
    expect(html).toContain('Turn airflow requirements into a clear engineering path.')
  })

  it('keeps route-specific collection, FAQ, download, About and Contact components after publication', async () => {
    const scenarios: Array<{
      path: string
      kind: ContentEntry['kind']
      slug: string
      marker: string
    }> = [
      { path: '/en/resources/articles', kind: 'article', slug: 'index', marker: 'Search published articles' },
      { path: '/en/resources/faqs', kind: 'faq', slug: 'index', marker: 'FAQ categories' },
      { path: '/en/resources/downloads', kind: 'download', slug: 'index', marker: 'Search title, description, type or model' },
      { path: '/en/company/about', kind: 'company', slug: 'about', marker: 'Innovation · Quality · Service' },
      { path: '/en/company/contact', kind: 'company', slug: 'contact', marker: 'Business email' },
    ]

    for (const scenario of scenarios) {
      const route = resolvePublicRoute(scenario.path)
      const page = {
        ...route,
        publishedContent: publishedContent([
          { type: 'paragraph', content: [{ type: 'text', text: 'Editorial remains visible.' }] },
        ], { kind: scenario.kind, slug: scenario.slug, canonicalPath: scenario.path }),
        dataState: 'published' as const,
      }
      const html = await renderToString(createSSRApp({ render: () => h(PageRenderer, { page }) }))
      expect(html, scenario.path).toContain('Editorial remains visible.')
      expect(html, scenario.path).toContain(scenario.marker)
    }
  })

  it('continues to use the CMS body as the primary ordinary detail document', async () => {
    const path = '/en/resources/articles/reading-a-fan-curve'
    const page = {
      ...resolvePublicRoute(path),
      publishedContent: publishedContent([
        { type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: 'Controlled article body' }] },
      ], { kind: 'article', slug: 'reading-a-fan-curve', canonicalPath: path }),
      dataState: 'published' as const,
    }
    const html = await renderToString(createSSRApp({ render: () => h(PageRenderer, { page }) }))
    expect(html).toContain('Controlled article body')
    expect(html).not.toContain('Read the axes before the line')
  })

  it('keeps a published Download body and adds the controlled-file panel', async () => {
    const path = '/en/resources/downloads/approved-resource'
    const page = {
      ...resolvePublicRoute(path),
      publishedContent: publishedContent([
        { type: 'paragraph', content: [{ type: 'text', text: 'Controlled resource context.' }] },
      ], {
        kind: 'download', slug: 'approved-resource', canonicalPath: path,
        attrs: {
          resourceType: 'Manual', version: 'V2', applicableModels: ['MODEL-A'],
          fileDescription: 'Published file description.', downloadUrl: '/media/public/manual.pdf',
          fileStatus: { scan: 'clean', access: 'public' },
        },
      }),
      dataState: 'published' as const,
      indexable: true,
    }
    const html = await renderToString(createSSRApp({ render: () => h(PageRenderer, { page }) }))
    expect(html).toContain('Controlled resource context.')
    expect(html).toContain('Published file description.')
    expect(html).toContain('href="/media/public/manual.pdf"')
    expect(html).toContain('Download resource')
  })
})
