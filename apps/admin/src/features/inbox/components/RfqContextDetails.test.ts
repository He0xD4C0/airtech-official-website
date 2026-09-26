import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import RfqContextDetails from '@/features/inbox/components/RfqContextDetails.vue'

describe('controlled RFQ context presentation', () => {
  it('preserves zero, units and free text without turning customer requirements into claims', async () => {
    const html = await renderToString(createSSRApp(RfqContextDetails, { snapshot: {
      journey: 'selection', context: {
        application: '<script>private</script>',
        dutyPoint: { airflow: 1200, airflowUnit: 'm3/h', pressure: 300, pressureUnit: 'Pa' },
        ambientTemperatureC: 0, requiredCertifications: ['CE', 'UL'], motorTechnology: 'EC',
      },
    } }))
    for (const value of ['1200', 'm3/h', '300', 'Pa', 'CE, UL', '环境温度', '<dd>0</dd>', '不代表已验证']) expect(html).toContain(value)
    expect(html).not.toContain('<script>')
    expect(html).toContain('&lt;script&gt;')
  })
})
