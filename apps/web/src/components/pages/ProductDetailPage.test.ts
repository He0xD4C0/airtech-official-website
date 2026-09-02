import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { createPinia } from 'pinia'
import { describe, expect, it } from 'vitest'
import type { Product } from '@airtek/contracts'
import ProductDetailPage from './ProductDetailPage.vue'
import type { PublicPageModel } from '@/types/content'

const product: Product = {
  id: '77935cef-4111-4c4c-bdb8-17679a8b42fe',
  stableId: 'AT-PUBLISHED-001',
  model: 'Controlled model identity',
  slug: 'controlled-model',
  locale: 'en',
  family: 'axial',
  subtype: null,
  motorTechnology: null,
  title: 'Published product',
  summary: 'Published summary.',
  seo: { title: 'Published product', description: 'Published summary.', canonicalPath: '/en/products/axial/controlled-model', indexable: true },
  sortOrder: 0,
  relatedContentIds: [],
  specifications: [],
  performanceCurves: [{
    airflowUnit: 'm3/h',
    pressureUnit: 'Pa',
    points: [{ airflow: 0, pressure: 100 }, { airflow: 100, pressure: 0 }],
    state: 'verified',
    sourceReference: 'controlled-source',
    densityKgM3: null,
    speedRpm: null,
    voltage: null,
    testMethod: null,
  }],
  sourceSnapshotId: 'c44656ad-fc7a-41c0-909e-930466096b37',
  sourceRevision: 'source-1',
  currentRevision: 1,
  publishedRevision: 1,
  status: 'published',
  indexable: true,
  updatedAt: '2026-09-01T08:00:00Z',
}

const page: PublicPageModel = {
  kind: 'product-detail',
  canonicalPath: '/en/products/axial/controlled-model',
  title: product.title,
  metaTitle: `${product.title} | AIRTEKPOWER`,
  description: product.summary ?? '',
  eyebrow: product.model ?? product.stableId,
  breadcrumbs: [{ label: 'Home', href: '/en' }, { label: product.title }],
  indexable: true,
  category: 'axial',
  slug: product.slug,
  publishedProduct: product,
}

describe('product detail progressive enhancement', () => {
  it('keeps verified PQ data in SSR HTML and omits unavailable optional sections', async () => {
    const app = createSSRApp(ProductDetailPage, { page })
    app.use(createPinia())
    const html = await renderToString(app)

    expect(html).not.toContain('Structured specifications')
    expect(html).toContain('Airflow and pressure')
    expect(html).toContain('Equivalent performance curve data')
    expect(html).not.toContain('Controlled resources')
    expect(html).not.toContain('/en/resources/downloads')
  })
})
