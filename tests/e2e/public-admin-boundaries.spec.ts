import AxeBuilder from '@axe-core/playwright'
import { expect, test, type APIRequestContext, type APIResponse } from '@playwright/test'

const configuredBindAddress = process.env.AIRTEK_BIND_ADDRESS ?? '127.0.0.1'
const diagnosticHost = configuredBindAddress === '0.0.0.0' || configuredBindAddress === '::'
  ? '127.0.0.1'
  : configuredBindAddress
const urlHost = diagnosticHost.includes(':') ? `[${diagnosticHost}]` : diagnosticHost
const publicOrigin = process.env.E2E_PUBLIC_ORIGIN
  ?? `http://${urlHost}:${process.env.AIRTEK_PUBLIC_HOST_PORT ?? '3000'}`
const adminOrigin = process.env.E2E_ADMIN_ORIGIN
  ?? `http://${urlHost}:${process.env.AIRTEK_ADMIN_HOST_PORT ?? '3100'}`
const gatewayControlOrigin = process.env.E2E_GATEWAY_CONTROL_ORIGIN

function absolute(origin: string, pathname: string): string {
  return new URL(pathname, `${origin.replace(/\/$/u, '')}/`).toString()
}

function gatewayGet(
  request: APIRequestContext,
  origin: string,
  pathname: string,
  options: { maxRedirects?: number } = {},
): Promise<APIResponse> {
  if (!gatewayControlOrigin) return request.get(absolute(origin, pathname), options)
  return request.get(absolute(gatewayControlOrigin, pathname), {
    ...options,
    headers: { Host: new URL(origin).host },
  })
}

function severeAccessibilityViolations(
  violations: Awaited<ReturnType<AxeBuilder['analyze']>>['violations'],
) {
  return violations.filter(({ impact }) => impact === 'serious' || impact === 'critical')
}

test.describe('Public SSR contract', () => {
  test('redirects the root permanently to the English canonical prefix', async ({ request }) => {
    const response = await gatewayGet(request, publicOrigin, '/', { maxRedirects: 0 })

    expect(response.status()).toBe(308)
    expect(response.headers().location).toBe('/en')
  })

  // TODO(public-projection-cutover): the Admin API now publishes unified CMS V2
  // revisions (`document`), while the public SSR still reads the legacy
  // `content_revisions.payload`. Re-enable once publishing writes the public
  // projection; the V2 fixtures created by global-setup are already in place.
  test.fixme('renders title, navigation, canonical link and body without JavaScript', async ({ browser }) => {
    const context = await browser.newContext({ javaScriptEnabled: false })
    const page = await context.newPage()

    try {
      const response = await page.goto(absolute(publicOrigin, '/en'))

      expect(response?.status()).toBe(200)
      await expect(page).toHaveTitle(/AIRTEKPOWER/u)
      await expect(page.locator('link[rel="canonical"]')).toHaveAttribute('href', /\/en$/u)
      await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeVisible()
      await expect(page.getByRole('heading', { level: 1 })).toHaveText('AIRTEKPOWER Development Preview')
      await expect(page.locator('main#main-content')).toContainText('Database-backed content')
      await expect(page.locator('meta[name="robots"]')).toHaveAttribute('content', /noindex/iu)
      await expect(page.locator('main#main-content a[href="/en/request-a-quote"]').first()).toBeVisible()
    } finally {
      await context.close()
    }
  })

  test('does not expose the Admin route on the Public origin', async ({ request }) => {
    for (const pathname of ['/admin', '/admin/users']) {
      const response = await gatewayGet(request, publicOrigin, pathname)
      expect(response.status(), pathname).toBe(404)
    }
  })

  // TODO(public-projection-cutover): same dependency as the SSR rendering test above.
  test.fixme('has no serious or critical automated accessibility violations on the SSR home page', async ({ page }) => {
    const response = await page.goto(absolute(publicOrigin, '/en'))
    expect(response?.status()).toBe(200)
    await expect(page.locator('main#main-content')).toBeVisible()

    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
      .analyze()

    expect(severeAccessibilityViolations(results.violations)).toEqual([])
  })
})

test.describe('Admin crawl isolation contract', () => {
  test('clears invitation tokens from the URL and never persists them in browser storage', async ({ page }) => {
    const token = 'A'.repeat(43)
    const response = await page.goto(absolute(adminOrigin, `/accept-invitation?token=${token}`))

    expect(response?.status()).toBe(200)
    await expect(page).toHaveURL(absolute(adminOrigin, '/accept-invitation'))
    await expect(page.getByText('一次性邀请令牌已读取')).toBeVisible()
    const persistedValues = await page.evaluate(() => [
      ...Object.values(window.localStorage),
      ...Object.values(window.sessionStorage),
    ])
    expect(persistedValues.join('\n')).not.toContain(token)
  })

  test('serves crawler denial and noindex on every checked response', async ({ request }) => {
    const robots = await gatewayGet(request, adminOrigin, '/robots.txt')
    expect(robots.status()).toBe(200)
    expect(await robots.text()).toMatch(/User-agent:\s*\*[\s\S]*Disallow:\s*\//iu)
    expect(robots.headers()['x-robots-tag']).toMatch(/noindex,\s*nofollow,\s*noarchive/iu)

    const login = await gatewayGet(request, adminOrigin, '/login')
    expect(login.status()).toBe(200)
    expect(login.headers()['x-robots-tag']).toMatch(/noindex,\s*nofollow,\s*noarchive/iu)
    expect(await login.text()).toMatch(/<meta\s+name=["']robots["']\s+content=["']noindex, nofollow, noarchive["']/iu)
  })

  test('returns 404 for sitemap variants and Public routes', async ({ request }) => {
    for (const pathname of [
      '/sitemap.xml',
      '/sitemap-pages.xml',
      '/sitemap-products.xml',
      '/en',
      '/en/products',
    ]) {
      const response = await gatewayGet(request, adminOrigin, pathname)
      expect(response.status(), pathname).toBe(404)
      expect(response.headers()['x-robots-tag'], pathname).toMatch(/noindex/iu)
    }
  })

  test('has no serious or critical automated accessibility violations on login', async ({ page }) => {
    const response = await page.goto(absolute(adminOrigin, '/login'))
    expect(response?.status()).toBe(200)
    await expect(page.locator('main.auth-layout')).toBeVisible()

    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
      .analyze()

    expect(severeAccessibilityViolations(results.violations)).toEqual([])
  })
})
