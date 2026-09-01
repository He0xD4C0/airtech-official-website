import { createPinia, setActivePinia } from 'pinia'
import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Product } from '@airtek/contracts'
import { getPublishedProduct } from '@/lib/api'
import CompareWorkspace from './CompareWorkspace.vue'

vi.mock('@/lib/api', () => ({
  getPublishedProduct: vi.fn(),
  submitAnalyticsEvent: vi.fn(),
}))

function product(slug: string, family: Product['family'], value: number): Product {
  return {
    id: crypto.randomUUID(),
    stableId: `AT-${slug.toUpperCase()}`,
    model: `Model ${value}`,
    slug,
    locale: 'en',
    family,
    subtype: null,
    motorTechnology: null,
    title: `Published ${slug}`,
    summary: 'Published comparison record.',
    specifications: [{
      key: 'capacity',
      label: 'Capacity',
      value,
      unit: 'unit',
      operatingCondition: null,
      sourceReference: null,
      state: 'verified',
    }],
    performanceCurves: [],
    sourceSnapshotId: crypto.randomUUID(),
    sourceRevision: 'source-1',
    currentRevision: 1,
    publishedRevision: 1,
    status: 'published',
    indexable: true,
    updatedAt: '2026-09-01T08:00:00Z',
  }
}

describe('published product comparison', () => {
  beforeEach(() => {
    window.sessionStorage.clear()
    window.history.replaceState({}, '', '/en/products/compare?products=first-record,second-record')
    setActivePinia(createPinia())
    vi.mocked(getPublishedProduct).mockReset()
    vi.mocked(getPublishedProduct).mockImplementation(async (slug) => (
      slug === 'first-record' ? product(slug, 'axial', 10) : product(slug, 'centrifugal', 20)
    ))
  })

  it('hydrates a share URL from live published records and highlights differences', async () => {
    const wrapper = mount(CompareWorkspace, { global: { plugins: [createPinia()] } })
    await flushPromises()

    expect(getPublishedProduct).toHaveBeenCalledTimes(2)
    expect(wrapper.text()).toContain('Published first-record')
    expect(wrapper.text()).toContain('Published second-record')
    expect(wrapper.findAll('tbody tr.is-different').length).toBeGreaterThan(0)
    expect(new URL(window.location.href).searchParams.get('products')).toBe('first-record,second-record')
  })

  it('accepts at most four safe slugs from a shared URL', async () => {
    window.history.replaceState({}, '', '/en/products/compare?products=one,two,three,four,five,../admin')
    vi.mocked(getPublishedProduct).mockImplementation(async (slug) => product(slug, 'axial', 10))
    mount(CompareWorkspace, { global: { plugins: [createPinia()] } })
    await flushPromises()

    expect(getPublishedProduct).toHaveBeenCalledTimes(4)
    expect(new URL(window.location.href).searchParams.get('products')).toBe('one,two,three,four')
  })
})
