import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import CollectionPage from '@/features/content/pages/CollectionPage.vue'
import type { PublicPageModel } from '@/shared/types/content'
import { createPublicTestPlugins } from '@/shared/test/publicAppPlugins'

const page: PublicPageModel = {
  kind: 'collection',
  canonicalPath: '/en/resources/articles',
  title: 'Articles',
  metaTitle: 'Articles | AIRTEKPOWER',
  description: 'Published articles.',
  eyebrow: 'Resources',
  breadcrumbs: [],
  indexable: true,
  collection: 'articles',
  entries: [
    { slug: 'airflow', title: 'Airflow context', summary: 'Pressure and units.', href: '/en/resources/articles/airflow' },
    { slug: 'data-state', title: 'Product data states', summary: 'Missing values.', href: '/en/resources/articles/data-state' },
  ],
}

describe('article collection', () => {
  it('renders every published entry for SSR and filters it after hydration', async () => {
    const wrapper = mount(CollectionPage, { props: { page }, global: { plugins: createPublicTestPlugins() } })
    expect(wrapper.text()).toContain('Airflow context')
    expect(wrapper.text()).toContain('Product data states')

    await wrapper.get('input[type="search"]').setValue('pressure')
    expect(wrapper.text()).toContain('Airflow context')
    expect(wrapper.text()).not.toContain('Product data states')
  })
})
