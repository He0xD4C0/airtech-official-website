import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import ProductFactsTable from '@/features/catalog/components/ProductFactsTable.vue'

describe('ProductFactsTable', () => {
  it('renders API values with an accessible table contract and escapes text', async () => {
    const app = createSSRApp(ProductFactsTable, {
      specifications: [{
        key: 'airflow',
        label: '<script>alert(1)</script>',
        value: 120,
        unit: 'm3/h',
        operatingCondition: 'Verified source condition',
        state: 'verified',
        sourceReference: 'Product Master cell',
      }],
    })
    const html = await renderToString(app)

    expect(html).toContain('产品规格、单位、工况、事实状态与来源')
    expect(html).toContain('scope="row"')
    expect(html).toContain('已验证')
    expect(html).toContain('Verified source condition')
    expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;')
    expect(html).not.toContain('<script>alert(1)</script>')
  })

  it('states honestly when the API has no specifications', async () => {
    const html = await renderToString(createSSRApp(ProductFactsTable, { specifications: [] }))
    expect(html).toContain('不会生成占位参数')
    expect(html).not.toContain('<table')
  })
})
