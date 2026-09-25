import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it, vi } from 'vitest'

const pageContext = vi.hoisted<{ abortStatusCode: number }>(() => ({ abortStatusCode: 404 }))
vi.mock('vike-vue/usePageContext', () => ({ usePageContext: () => pageContext }))

import ErrorPage from './+Page.vue'

describe('public error page', () => {
  it('offers a working home entry for a real 404', async () => {
    pageContext.abortStatusCode = 404
    const html = await renderToString(createSSRApp(ErrorPage))
    expect(html).toContain('href="/en"')
    expect(html).toContain('Return home')
    expect(html).not.toContain('Try again')
  })

  it('offers reload without redirecting an unavailable site back into the failure', async () => {
    pageContext.abortStatusCode = 503
    const html = await renderToString(createSSRApp(ErrorPage))
    expect(html).toContain('temporarily unavailable')
    expect(html).toContain('Try again')
    expect(html).not.toContain('href="/en"')
  })
})
