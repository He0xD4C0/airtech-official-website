import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import SelectorWorkspace from './SelectorWorkspace.vue'

describe('selector database facets', () => {
  it('renders only product families and motor technologies supplied by the published bootstrap', async () => {
    const html = await renderToString(createSSRApp(SelectorWorkspace, {
      productFamilies: [{
        code: 'centrifugal',
        slug: 'database-centrifugal',
        name: 'Database centrifugal family',
        description: '',
        sortOrder: 1,
      }],
      motorTechnologies: ['Published EC facet'],
    }))

    expect(html).toContain('Database centrifugal family')
    expect(html).toContain('Published EC facet')
    expect(html).not.toContain('<option value="DC">')
    expect(html).not.toContain('Axial</option>')
  })

  it('omits empty facet controls instead of inventing options', async () => {
    const html = await renderToString(createSSRApp(SelectorWorkspace, {
      productFamilies: [],
      motorTechnologies: [],
    }))

    expect(html).not.toContain('<span>Fan form</span>')
    expect(html).not.toContain('<span>Motor technology</span>')
  })
})
