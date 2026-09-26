import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import SelectorWorkspace from '@/features/selector/components/SelectorWorkspace.vue'
import { createPublicTestPlugins } from '@/shared/test/publicAppPlugins'

describe('selector database facets', () => {
  it('renders only product families and motor technologies supplied by the published bootstrap', async () => {
    const wrapper = mount(SelectorWorkspace, {
      props: { productFamilies: [{
        code: 'centrifugal',
        slug: 'database-centrifugal',
        name: 'Database centrifugal family',
        description: '',
        sortOrder: 1,
      }],
      motorTechnologies: ['Published EC facet'],
      },
      global: { plugins: createPublicTestPlugins() },
    })
    await wrapper.get('input[aria-label="Required airflow"]').setValue('1200')
    await wrapper.get('input[aria-label="Required pressure"]').setValue('320')
    await wrapper.get('form').trigger('submit')
    await wrapper.get('form').trigger('submit')
    const html = wrapper.html()

    expect(html).toContain('Database centrifugal family')
    expect(html).toContain('Published EC facet')
    expect(html).not.toContain('<option value="DC">')
    expect(html).not.toContain('Axial</option>')
  })

  it('omits empty facet controls instead of inventing options', async () => {
    const wrapper = mount(SelectorWorkspace, {
      props: { productFamilies: [], motorTechnologies: [] },
      global: { plugins: createPublicTestPlugins() },
    })
    await wrapper.get('input[aria-label="Required airflow"]').setValue('1200')
    await wrapper.get('input[aria-label="Required pressure"]').setValue('320')
    await wrapper.get('form').trigger('submit')
    await wrapper.get('form').trigger('submit')
    const html = wrapper.html()

    expect(html).not.toContain('<span>Fan form</span>')
    expect(html).not.toContain('<span>Motor technology</span>')
  })
})
