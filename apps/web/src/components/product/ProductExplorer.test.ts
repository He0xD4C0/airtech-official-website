import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Product } from '@airtek/contracts'
import { trackAnalyticsEvent } from '@/lib/analytics'
import { getPublishedProducts } from '@/lib/api'
import ProductExplorer from './ProductExplorer.vue'

vi.mock('@/lib/analytics', () => ({ trackAnalyticsEvent: vi.fn().mockResolvedValue(true) }))
vi.mock('@/lib/api', () => ({ getPublishedProducts: vi.fn() }))

const product: Product = {
  id: 'f1206911-5bed-4268-90c5-164f381c9020',
  stableId: 'AT-VERIFIED-001',
  model: 'Published model',
  slug: 'published-axial-product',
  locale: 'en',
  family: 'axial',
  subtype: 'Published subtype',
  motorTechnology: 'EC',
  title: 'Published axial product',
  summary: 'Summary from the published Product Master projection.',
  seo: { title: 'Published axial product', description: 'Summary from the published Product Master projection.', canonicalPath: '/en/products/axial/published-axial-product', indexable: true },
  sortOrder: 0,
  relatedContentIds: [],
  mediaGallery: [],
  specifications: [],
  performanceCurves: [],
  sourceSnapshotId: 'c44656ad-fc7a-41c0-909e-930466096b37',
  sourceRevision: 'source-8',
  currentRevision: 8,
  publishedRevision: 7,
  status: 'published',
  indexable: true,
  updatedAt: '2026-09-01T08:00:00Z',
}

const families = [
  { code: 'axial' as const, slug: 'axial', name: 'Axial', description: 'Published family', sortOrder: 1 },
  { code: 'centrifugal' as const, slug: 'centrifugal', name: 'Centrifugal', description: 'Published family', sortOrder: 2 },
]

describe('published product explorer', () => {
  beforeEach(() => {
    vi.mocked(trackAnalyticsEvent).mockClear()
    vi.mocked(getPublishedProducts).mockReset()
    vi.mocked(getPublishedProducts).mockResolvedValue({ items: [product], nextCursor: null })
  })

  it('SSR-renders only provided published Product Master records', async () => {
    const app = createSSRApp({ render: () => h(ProductExplorer, { products: [product], families }) })
    app.use(createPinia())
    const html = await renderToString(app)
    expect(html).toContain('Published axial product')
    expect(html).toContain('Published model')
    expect(html).toContain('AT-VERIFIED-001')
    expect(html).toContain('/en/products/axial/published-axial-product')
  })

  it('uses an explicit safe empty state when no product is published', async () => {
    const app = createSSRApp({ render: () => h(ProductExplorer, { products: [], families }) })
    app.use(createPinia())
    const html = await renderToString(app)
    expect(html).toContain('No validated products are available')
    expect(html).toContain('No model, performance value or compatibility claim has been inferred.')
  })

  it('filters the published projection on the client', async () => {
    const wrapper = mount(ProductExplorer, { props: { products: [product], families }, global: { plugins: [createPinia()] } })
    expect(wrapper.text()).toContain('Published axial product')
    await wrapper.get('input[type="search"]').setValue('not-present')
    expect(wrapper.text()).toContain('No matching published product')
    expect(wrapper.text()).not.toContain('Summary from the published Product Master projection.')
  })

  it('tracks only structured filter state and never the catalog query text', async () => {
    const wrapper = mount(ProductExplorer, { props: { products: [product], families }, global: { plugins: [createPinia()] } })
    const privateQuery = 'buyer@example.com confidential requirement'
    const search = wrapper.get('input[type="search"]')
    await search.setValue(privateQuery)
    await search.trigger('change')
    await flushPromises()

    expect(trackAnalyticsEvent).toHaveBeenCalledWith('filterApplied', {
      filterName: 'catalogSearch',
      resultCount: 0,
    })
    expect(JSON.stringify(vi.mocked(trackAnalyticsEvent).mock.calls)).not.toContain(privateQuery)

    vi.mocked(trackAnalyticsEvent).mockClear()
    await wrapper.findAll('select')[0]!.setValue('axial')
    await flushPromises()
    expect(trackAnalyticsEvent).toHaveBeenCalledWith('filterApplied', {
      filterName: 'family',
      resultCount: 0,
    })
  })

  it('uses opaque next cursors, retains previous-page history and labels counts as page-scoped', async () => {
    const secondProduct: Product = {
      ...product,
      id: 'f4abdf14-c27b-4d71-a22b-45cc3f7aa64d',
      stableId: 'AT-VERIFIED-002',
      slug: 'published-second-product',
      title: 'Published second product',
    }
    vi.mocked(getPublishedProducts)
      .mockResolvedValueOnce({ items: [secondProduct], nextCursor: null })
      .mockResolvedValueOnce({ items: [product], nextCursor: 'cGFnZS0y' })

    const wrapper = mount(ProductExplorer, {
      props: { products: [product], families, nextCursor: 'cGFnZS0y' },
      global: { plugins: [createPinia()] },
    })
    expect(wrapper.text()).toContain('1 matching record on page 1')
    expect(wrapper.text()).toContain('not the total catalog')

    const next = wrapper.findAll('button').find((button) => button.text() === 'Next page')
    expect(next).toBeDefined()
    await next!.trigger('click')
    await flushPromises()
    expect(getPublishedProducts).toHaveBeenNthCalledWith(1, { limit: 24, cursor: 'cGFnZS0y' })
    expect(wrapper.text()).toContain('Published second product')
    expect(wrapper.text()).toContain('Page 2')
    expect(trackAnalyticsEvent).toHaveBeenCalledWith('filterApplied', {
      filterName: 'catalogPagination', resultCount: 1,
    })

    const previous = wrapper.findAll('button').find((button) => button.text() === 'Previous page')
    await previous!.trigger('click')
    await flushPromises()
    expect(getPublishedProducts).toHaveBeenNthCalledWith(2, { limit: 24 })
    expect(wrapper.text()).toContain('Published axial product')
    expect(wrapper.text()).toContain('page 1')
  })

  it('resets cursor history when a filter changes and keeps a recoverable failure state', async () => {
    vi.mocked(getPublishedProducts)
      .mockResolvedValueOnce({ items: [{ ...product, id: crypto.randomUUID(), slug: 'page-two-product' }], nextCursor: null })
      .mockRejectedValueOnce(new Error('Published catalog temporarily unavailable.'))

    const wrapper = mount(ProductExplorer, {
      props: { products: [product], families, nextCursor: 'cGFnZS0y' },
      global: { plugins: [createPinia()] },
    })
    const next = wrapper.findAll('button').find((button) => button.text() === 'Next page')
    await next!.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('Page 2')

    await wrapper.findAll('select')[0]!.setValue('centrifugal')
    await flushPromises()
    expect(getPublishedProducts).toHaveBeenNthCalledWith(2, { limit: 24, family: 'centrifugal' })
    expect(wrapper.text()).toContain('Catalog page unavailable')
    expect(wrapper.text()).toContain('Published catalog temporarily unavailable.')
    expect(wrapper.text()).toContain('page 1')
    expect(wrapper.text()).not.toContain('Previous page')

    vi.mocked(getPublishedProducts).mockResolvedValueOnce({ items: [product], nextCursor: null })
    const reload = wrapper.findAll('button').find((button) => button.text() === 'Reload first page')
    await reload!.trigger('click')
    await flushPromises()
    expect(wrapper.text()).not.toContain('Catalog page unavailable')
  })
})
