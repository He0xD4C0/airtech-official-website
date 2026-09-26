import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it, vi } from 'vitest'
import { createPublicTestPlugins } from '@/shared/test/publicAppPlugins'

const pageContext = vi.hoisted<{ abortStatusCode: number }>(() => ({ abortStatusCode: 404 }))
vi.mock('vike-vue/usePageContext', () => ({ usePageContext: () => pageContext }))

import ErrorPage from './+Page.vue'

async function renderErrorPage(): Promise<string> {
  const app = createSSRApp(ErrorPage)
  for (const plugin of createPublicTestPlugins()) app.use(plugin)
  return renderToString(app)
}

describe('public error page', () => {
  it('offers a working home entry for a real 404', async () => {
    pageContext.abortStatusCode = 404
    const html = await renderErrorPage()
    expect(html).toContain('href="/en"')
    expect(html).toContain('Return home')
    expect(html).not.toContain('Try again')
  })

  it('offers reload without redirecting an unavailable site into the same failure', async () => {
    pageContext.abortStatusCode = 503
    const html = await renderErrorPage()
    expect(html).toContain('temporarily unavailable')
    expect(html).toContain('Try again')
    expect(html).not.toContain('href="/en"')
  })
})
