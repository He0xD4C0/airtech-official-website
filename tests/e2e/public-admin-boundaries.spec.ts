import AxeBuilder from '@axe-core/playwright'
import { expect, test, type APIRequestContext, type APIResponse, type Page } from '@playwright/test'
import { adminStorageStatePath } from './support/environment'

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
const publicGatewayHost = process.env.E2E_PUBLIC_GATEWAY_HOST
const adminGatewayHost = process.env.E2E_ADMIN_GATEWAY_HOST
const coreRoutes = [
  '/en',
  '/en/products',
  '/en/products/selector',
  '/en/products/compare',
  '/en/search',
  '/en/company/about',
  '/en/company/contact',
  '/en/request-a-quote',
  '/en/request-a-quote/product',
  '/en/request-a-quote/selection',
  '/en/request-a-quote/project',
  '/en/request-a-quote/replacement',
  '/en/privacy',
  '/en/terms',
  '/en/cookie-settings',
] as const

type RfqJourney = 'product' | 'selection' | 'project' | 'replacement'

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
  const gatewayHost = origin === publicOrigin
    ? publicGatewayHost
    : origin === adminOrigin
      ? adminGatewayHost
      : undefined
  return request.get(absolute(gatewayControlOrigin, pathname), {
    ...options,
    headers: { Host: gatewayHost || new URL(origin).host },
  })
}

function severeAccessibilityViolations(
  violations: Awaited<ReturnType<AxeBuilder['analyze']>>['violations'],
) {
  return violations.filter(({ impact }) => impact === 'serious' || impact === 'critical')
}

async function fillRfqContext(page: Page, journey: RfqJourney): Promise<void> {
  await page.getByLabel('Application or system').fill(`Isolated ${journey} browser acceptance`)
  await page.getByLabel(/Estimated quantity/u).fill('12')
  await page.getByLabel(/Voltage/u).fill('230')
  await page.getByLabel(/Frequency/u).selectOption('50')
  await page.getByLabel(/Environment and constraints/u).fill('Indoor acceptance environment')
  if (journey === 'selection' || journey === 'replacement') {
    await page.getByLabel('Required airflow').fill('1000')
    await page.getByLabel('Required pressure').fill('250')
  }
  if (journey === 'selection') {
    await page.getByLabel(/Ambient temperature/u).fill('0')
    await page.getByLabel(/Fan family/u).selectOption('axial')
    await page.getByLabel(/Motor technology/u).fill('EC')
    await page.getByLabel(/Maximum diameter/u).fill('500')
    await page.getByLabel(/Control method/u).fill('PWM')
    await page.getByLabel(/Required certifications/u).fill('Acceptance-only')
  }
  if (journey === 'project') {
    await page.getByLabel('Project stage').selectOption({ label: 'Engineering' })
    await page.getByLabel(/Project scale/u).fill('One isolated test system')
    await page.getByLabel(/Target schedule/u).fill('Acceptance window')
    await page.getByLabel(/Engineering needs/u).fill('Review the submitted structured context.')
  }
  if (journey === 'replacement') {
    await page.getByLabel(/Existing model/u).fill('TEST-LEGACY-001')
    await page.getByLabel(/Installation constraints/u).fill('Keep the existing mounting envelope.')
    await page.getByLabel(/Replacement goal/u).fill('Request an engineering review only.')
  }
}

async function submitRfqJourney(page: Page, journey: RfqJourney): Promise<string> {
  const path = journey === 'product'
    ? '/en/request-a-quote/product?product=e2e-model-001&family=axial'
    : `/en/request-a-quote/${journey}`
  const response = await page.goto(absolute(publicOrigin, path))
  expect(response?.status()).toBe(200)
  if (journey === 'product') await expect(page.getByText('E2E-PRODUCT-001')).toBeVisible()
  await fillRfqContext(page, journey)
  await page.getByRole('button', { name: 'Continue' }).click()
  await expect(page.getByRole('heading', { name: 'Who should we follow up with?' })).toBeVisible()
  await page.getByLabel('Company').fill('AIRTEK browser acceptance')
  await page.getByLabel('Your name').fill('Acceptance Buyer')
  await page.getByLabel('Business email').fill(`e2e+${journey}@example.com`)
  await page.getByLabel(/^Phone/u).fill('+86 138 0000 0000')
  await page.getByLabel('Country or region').fill('CN')
  await page.getByLabel(/Additional context/u).fill('No file upload is requested by this public workflow.')
  await page.locator('form.form-card input[type="checkbox"][required]').check()
  await page.getByRole('button', { name: 'Continue' }).click()
  await expect(page.getByRole('heading', { name: 'Check your request' })).toBeVisible()
  const accepted = page.waitForResponse((candidate) => (
    new URL(candidate.url()).pathname === '/api/public/v1/rfqs'
    && candidate.request().method() === 'POST'
  ))
  await page.getByRole('button', { name: 'Submit request' }).click()
  const responseAccepted = await accepted
  expect(responseAccepted.status()).toBe(201)
  await expect(page.getByRole('heading', { name: 'Your inquiry has been recorded.' })).toBeVisible()
  return (await responseAccepted.json() as { reference: string }).reference
}

test.describe('Public SSR contract', () => {
  test('redirects the root permanently to the English canonical prefix', async ({ request }) => {
    const response = await gatewayGet(request, publicOrigin, '/', { maxRedirects: 0 })

    expect(response.status()).toBe(308)
    expect(response.headers().location).toBe('/en')
  })

  test('renders title, navigation, canonical link and body without JavaScript', async ({ browser }) => {
    const context = await browser.newContext({ javaScriptEnabled: false })
    const page = await context.newPage()

    try {
      const response = await page.goto(absolute(publicOrigin, '/en'))

      expect(response?.status()).toBe(200)
      await expect(page).toHaveTitle(/AIRTEKPOWER/u)
      await expect(page.locator('link[rel="canonical"]')).toHaveAttribute('href', /\/en$/u)
      await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeVisible()
      await expect(page.getByRole('heading', { level: 1 })).toHaveText('AIRTEKPOWER Production Image Acceptance')
      await expect(page.locator('main#main-content')).toContainText(
        'This placeholder verifies the published-content pipeline without introducing unreviewed product or business claims.')
      await expect(page.locator('meta[name="robots"]')).toHaveAttribute('content', /^index,/iu)
      await expect(page.locator('main#main-content a[href="/en/request-a-quote"]').first()).toBeVisible()
    } finally {
      await context.close()
    }
  })

  test('serves every core route and every local shell link', async ({ page, request }) => {
    for (const pathname of coreRoutes) {
      const response = await gatewayGet(request, publicOrigin, pathname)
      expect(response.status(), pathname).toBe(200)
    }

    await page.goto(absolute(publicOrigin, '/en'))
    const shellLinks = await page.locator('header a[href^="/"], footer a[href^="/"]')
      .evaluateAll((links) => [...new Set(
        links.map((link) => link.getAttribute('href')).filter((href): href is string => Boolean(href)),
      )])
    expect(shellLinks.length).toBeGreaterThan(0)
    for (const pathname of shellLinks) {
      const response = await gatewayGet(request, publicOrigin, pathname)
      expect(response.status(), pathname).toBe(200)
    }
  })

  test('returns a real 404 with a usable home entry after the site shell is ready', async ({ request }) => {
    const response = await gatewayGet(request, publicOrigin, '/en/not-a-published-route')
    expect(response.status()).toBe(404)
    expect(await response.text()).toMatch(/Return home/iu)
  })

  test('does not expose the Admin route on the Public origin', async ({ request }) => {
    for (const pathname of ['/admin', '/admin/users']) {
      const response = await gatewayGet(request, publicOrigin, pathname)
      expect(response.status(), pathname).toBe(404)
    }
  })

  test('has no serious or critical automated accessibility violations on the SSR home page', async ({ page }) => {
    const response = await page.goto(absolute(publicOrigin, '/en'))
    expect(response?.status()).toBe(200)
    await expect(page.locator('main#main-content')).toBeVisible()

    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
      .analyze()

    expect(severeAccessibilityViolations(results.violations)).toEqual([])
  })

  test('submits contact details including a phone number', async ({ page }) => {
    await page.goto(absolute(publicOrigin, '/en/company/contact'))
    await page.getByLabel('Topic').selectOption('technical')
    await page.getByLabel(/Company/u).fill('AIRTEK browser acceptance')
    await page.getByLabel('Your name').fill('Acceptance Buyer')
    await page.getByLabel('Business email').fill('e2e-contact@example.com')
    await page.getByLabel(/^Phone/u).fill('+86 138 0000 0000')
    await page.getByLabel('How can we help?').fill('Please record this isolated browser acceptance inquiry.')
    await page.locator('form.form-card input[type="checkbox"][required]').check()
    const accepted = page.waitForResponse((response) => (
      new URL(response.url()).pathname === '/api/public/v1/contact'
      && response.request().method() === 'POST'
    ))
    await page.getByRole('button', { name: 'Send message' }).click()
    expect((await accepted).status()).toBe(201)
    await expect(page.getByRole('heading', { name: 'Thank you for contacting us.' })).toBeVisible()
  })

  test('searches the server catalog, preserves URL state and shares a comparison', async ({ page }) => {
    await page.goto(absolute(publicOrigin, '/en/products'))
    // This preference panel appears only after Vue has mounted, not in SSR HTML.
    await expect(page.getByRole('complementary', { name: 'Analytics cookie preferences' })).toBeVisible()
    const productCard = page.locator('article.product-card').filter({ hasText: 'E2E-MODEL-001' })
    await expect(productCard).toBeVisible()
    await expect(page.locator('.result-count')).toContainText('1 matching record')
    await expect(productCard).toContainText('E2E published product summary.')

    const catalogResponse = page.waitForResponse((response) => {
      const url = new URL(response.url())
      return url.pathname === '/api/public/v1/products'
        && url.searchParams.get('q') === 'E2E-MODEL-001'
    })
    const search = page.getByLabel('Search the published catalog')
    await search.fill('E2E-MODEL-001')
    await search.press('Tab')
    expect((await catalogResponse).status()).toBe(200)
    await expect(page).toHaveURL(/q=E2E-MODEL-001/u)
    await search.fill('no-matching-product')
    await search.press('Tab')
    await expect(page.locator('.result-count')).toContainText('0 matching records')
    await page.goBack()
    await expect(search).toHaveValue('E2E-MODEL-001')
    await expect(productCard).toBeVisible()

    await productCard.getByRole('button', { name: 'Compare' }).click()
    await expect(page.getByRole('complementary', { name: 'Analytics cookie preferences' })).toBeVisible()
    const tray = await page.getByRole('complementary', { name: 'Product comparison tray' }).boundingBox()
    const cookie = await page.getByRole('complementary', { name: 'Analytics cookie preferences' }).boundingBox()
    expect(tray && cookie && tray.y + tray.height <= cookie.y).toBe(true)
    await expect(page.getByRole('complementary', { name: 'Product comparison tray' })).toContainText('1/4 records selected')
    await page.getByRole('complementary', { name: 'Product comparison tray' }).getByRole('link', { name: 'Compare' }).click()
    await expect(page.getByText('1/4 published records')).toBeVisible()
    await expect(page).toHaveURL(/products=axial%7Ee2e-model-001/u)
    await expect(page.getByRole('row', { name: /Verified PQ curves/u })).toContainText('No verified curve')
    await page.getByRole('button', { name: 'Copy share URL' }).click()
    await expect(page.locator('.share-status')).toHaveText(/Share URL copied|Copy was blocked/u)
  })

  test('returns canonical public search results from the published projection', async ({ page }) => {
    await page.goto(absolute(publicOrigin, '/en/search?q=E2E'))
    await expect(page.getByRole('link', { name: 'E2E imported product' })).toHaveAttribute(
      'href',
      '/en/products/axial/e2e-model-001',
    )
  })

  test('submits the public selector against published product data', async ({ page }) => {
    await page.goto(absolute(publicOrigin, '/en/products/selector'))
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible()

    await page.getByLabel('Required airflow').fill('1000')
    await page.getByLabel('Required pressure').fill('250')
    await page.getByRole('button', { name: 'Continue' }).click()
    await page.getByLabel(/Maximum diameter/u).fill('500')
    await page.getByLabel(/Environment or installation notes/u).fill('Carry this note to the RFQ only.')
    await page.getByLabel(/Control method/u).fill('PWM')
    await page.getByRole('button', { name: 'Continue' }).click()
    const selectorResponse = page.waitForResponse((response) => (
      new URL(response.url()).pathname === '/api/public/v1/selector'
      && response.request().method() === 'POST'
    ))
    await page.getByRole('button', { name: 'Evaluate published records' }).click()

    expect((await selectorResponse).status()).toBe(200)
    await expect(page.getByRole('heading', { name: 'Engineering review required' })).toBeVisible()
    await expect(page.getByText('The published data is not sufficient for an automated recommendation. Continue through engineering review.')).toBeVisible()
  })

  test('blocks a generic Product RFQ without immutable product context', async ({ page }) => {
    await page.goto(absolute(publicOrigin, '/en/request-a-quote/product'))
    await expect(page.getByText('Published context required')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Continue' })).toBeDisabled()
  })

  for (const journey of ['product', 'selection', 'project', 'replacement'] as const) {
    test(`submits the ${journey} RFQ and reads its complete context in Admin`, async ({ page, browser }) => {
      const reference = await submitRfqJourney(page, journey)
      const admin = await browser.newContext({ storageState: adminStorageStatePath })
      try {
        const inbox = await admin.newPage()
        await inbox.goto(absolute(adminOrigin, `/rfqs?q=${encodeURIComponent(reference)}`))
        const context = inbox.getByRole('region', { name: 'RFQ 工程需求' })
        await expect(context).toHaveCount(0)
        const sensitive = inbox.waitForResponse((response) => /\/rfqs\/[^/]+\/pii$/u.test(new URL(response.url()).pathname))
        await inbox.getByRole('button', { name: '读取并记录审计' }).click()
        const response = await sensitive
        expect(response.headers()['cache-control']).toContain('no-store')
        await expect(context).toContainText(`Isolated ${journey} browser acceptance`)
        await expect(inbox.getByText('+86 138 0000 0000', { exact: true })).toBeVisible()
        await expect(context).toContainText('Indoor acceptance environment')
        await expect(context).toContainText('No file upload is requested')
        const extras = { product: '12', selection: 'PWM', project: 'One isolated test system', replacement: 'Keep the existing mounting envelope.' }
        await expect(context).toContainText(extras[journey])
        if (journey === 'selection') {
          await expect(context.locator('dl > div', { hasText: '环境温度' }).locator('dd')).toHaveText('0')
          await expect(context).toContainText('axial')
          await expect(context).toContainText('EC')
        }
      } finally { await admin.close() }
    })
  }

  for (const scenario of [
    { width: 390, path: '/en/products' },
    { width: 768, path: '/en/products/selector' },
    { width: 1280, path: '/en' },
  ]) {
    test(`passes keyboard, overflow and axe checks at ${scenario.width}px`, async ({ page }) => {
      await page.setViewportSize({ width: scenario.width, height: 900 })
      await page.goto(absolute(publicOrigin, scenario.path))
      await page.keyboard.press('Tab')
      await expect(page.locator('.skip-link')).toBeFocused()
      await page.keyboard.press('Enter')
      await expect(page).toHaveURL(/#main-content$/u)
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)
      expect(overflow).toBeLessThanOrEqual(1)
      const results = await new AxeBuilder({ page })
        .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
        .analyze()
      expect(severeAccessibilityViolations(results.violations)).toEqual([])
    })
  }
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
