import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Product } from '@airtek/contracts'
import { trackAnalyticsEvent } from '@/features/analytics'
import { getPublishedProducts } from '@/shared/lib/api'
import ProductExplorer from '@/features/catalog/components/ProductExplorer.vue'

vi.mock('@/features/analytics/lib/analytics', () => ({ trackAnalyticsEvent: vi.fn().mockResolvedValue(true) }))
vi.mock('@/shared/lib/api', () => ({ getPublishedProducts: vi.fn() }))

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
    specifications: [], sourceFacts: [],
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

function productPage(items: Product[] = [product], nextCursor: string | null = null, total = items.length) {
  return {
    items,
    nextCursor,
    total,
    familyCounts: total ? [{ value: 'axial', count: total }] : [],
    motorTechnologyCounts: total ? [{ value: 'EC', count: total }] : [],
  }
}

const catalogProps = {
  products: [product],
  families,
  total: 1,
  familyCounts: [{ value: 'axial', count: 1 }],
  motorTechnologyCounts: [{ value: 'EC', count: 1 }],
}

describe('published product explorer', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/en/products')
    vi.mocked(trackAnalyticsEvent).mockClear()
    vi.mocked(getPublishedProducts).mockReset()
    vi.mocked(getPublishedProducts).mockResolvedValue(productPage())
  })

  it('SSR-renders only provided published Product Master records', async () => {
    const app = createSSRApp({ render: () => h(ProductExplorer, catalogProps) })
    app.use(createPinia())
    const html = await renderToString(app)
    expect(html).toContain('Published model')
    expect(html).toContain('/en/products/axial/published-axial-product')
  })

  it('uses an explicit safe empty state when no product is published', async () => {
    const app = createSSRApp({ render: () => h(ProductExplorer, { products: [], families }) })
    app.use(createPinia())
    const html = await renderToString(app)
    expect(html).toContain('No validated products are available')
    expect(html).toContain('No model, performance value or compatibility claim has been inferred.')
  })

  it('queries the published projection on the server', async () => {
    const nativePush = window.history.pushState.bind(window.history)
    const history = vi.spyOn(window.history, 'pushState').mockImplementation((state, title, url) => {
      // jsdom omits the real browser's DataCloneError check.
      structuredClone(state)
      nativePush(state, title, url)
    })
    vi.mocked(getPublishedProducts).mockReset().mockResolvedValue(productPage([], null, 0))
    const wrapper = mount(ProductExplorer, { props: catalogProps, global: { plugins: [createPinia()] } })
    expect(wrapper.text()).toContain('Published model')
    const search = wrapper.get('input[type="search"]')
    await search.setValue('not-present')
    await search.trigger('change')
    await flushPromises()
    expect(getPublishedProducts).toHaveBeenCalledWith({ limit: 24, q: 'not-present' })
    expect(wrapper.text()).toContain('No matching published records')
    expect(wrapper.text()).not.toContain('Summary from the published Product Master projection.')
    history.mockRestore()
    wrapper.unmount()
  })

  it('runs a catalog search when the search field receives Enter', async () => {
    vi.mocked(getPublishedProducts).mockReset().mockResolvedValue(productPage([], null, 0))
    const wrapper = mount(ProductExplorer, { props: catalogProps, global: { plugins: [createPinia()] } })
    const search = wrapper.get('input[type="search"]')
    await search.setValue('T30D-24-D718N-02')
    await search.trigger('keydown', { key: 'Enter' })
    await flushPromises()

    expect(getPublishedProducts).toHaveBeenCalledWith({ limit: 24, q: 'T30D-24-D718N-02' })
    expect(new URL(window.location.href).searchParams.get('q')).toBe('T30D-24-D718N-02')
    wrapper.unmount()
  })

  it('tracks only structured filter state and never the catalog query text', async () => {
    vi.mocked(getPublishedProducts).mockResolvedValue(productPage([], null, 0))
    const wrapper = mount(ProductExplorer, { props: catalogProps, global: { plugins: [createPinia()] } })
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

  it('uses opaque next cursors, retains previous-page history and shows total counts', async () => {
    const secondProduct: Product = {
      ...product,
      id: 'f4abdf14-c27b-4d71-a22b-45cc3f7aa64d',
      stableId: 'AT-VERIFIED-002',
      slug: 'published-second-product',
      title: 'Published second product',
      model: 'Published second model',
    }
    vi.mocked(getPublishedProducts)
      .mockResolvedValueOnce(productPage([secondProduct], null, 2))
      .mockResolvedValueOnce(productPage([product], 'cGFnZS0y', 2))

    const wrapper = mount(ProductExplorer, {
      props: { ...catalogProps, total: 2, nextCursor: 'cGFnZS0y' },
      global: { plugins: [createPinia()] },
    })
    expect(wrapper.text()).toContain('2 matching records')
    expect(wrapper.text()).toContain('Showing 1 records on page 1')

    const next = wrapper.findAll('button').find((button) => button.text() === 'Next page')
    expect(next).toBeDefined()
    await next!.trigger('click')
    await flushPromises()
    expect(getPublishedProducts).toHaveBeenNthCalledWith(1, { limit: 24, cursor: 'cGFnZS0y' })
    expect(wrapper.text()).toContain('Published second model')
    expect(wrapper.text()).toContain('Page 2')
    expect(new URL(window.location.href).searchParams.get('cursor')).toBe('cGFnZS0y')
    expect(trackAnalyticsEvent).toHaveBeenCalledWith('filterApplied', {
      filterName: 'catalogPagination', resultCount: 1,
    })

    const previous = wrapper.findAll('button').find((button) => button.text() === 'Previous page')
    await previous!.trigger('click')
    await flushPromises()
    expect(getPublishedProducts).toHaveBeenNthCalledWith(2, { limit: 24 })
    expect(wrapper.text()).toContain('Published model')
    expect(wrapper.text()).toContain('page 1')
    expect(new URL(window.location.href).searchParams.has('cursor')).toBe(false)
  })

  it('restores filters, cursor and view on browser history changes without erasing router state', async () => {
    window.history.replaceState({ routerMarker: 'preserved' }, '', '/en/products')
    const wrapper = mount(ProductExplorer, { props: catalogProps, global: { plugins: [createPinia()] } })
    await wrapper.get('input[type="search"]').setValue('model')
    await wrapper.get('input[type="search"]').trigger('change')
    await flushPromises()
    expect(window.history.state.routerMarker).toBe('preserved')
    window.history.replaceState({ publicQueryCursors: [null, 'opaque'] }, '', '/en/products?q=restored&family=axial&motorTechnology=EC&view=table&cursor=opaque')
    window.dispatchEvent(new PopStateEvent('popstate'))
    await flushPromises()
    expect(getPublishedProducts).toHaveBeenLastCalledWith({ limit: 24, q: 'restored', family: 'axial', motorTechnology: 'EC', cursor: 'opaque' })
    expect(wrapper.get<HTMLInputElement>('input[type="search"]').element.value).toBe('restored')
    expect(wrapper.find('table').exists()).toBe(true)
    wrapper.unmount()
  })

  it('resets cursor history when a filter changes and keeps a recoverable failure state', async () => {
    vi.mocked(getPublishedProducts)
      .mockResolvedValueOnce(productPage([{ ...product, id: crypto.randomUUID(), slug: 'page-two-product' }], null, 2))
      .mockRejectedValueOnce(new Error('Published catalog temporarily unavailable.'))

    const wrapper = mount(ProductExplorer, {
      props: { ...catalogProps, total: 2, nextCursor: 'cGFnZS0y' },
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

    vi.mocked(getPublishedProducts).mockResolvedValueOnce(productPage())
    const reload = wrapper.findAll('button').find((button) => button.text() === 'Reload first page')
    await reload!.trigger('click')
    await flushPromises()
    expect(wrapper.text()).not.toContain('Catalog page unavailable')
  })
})
