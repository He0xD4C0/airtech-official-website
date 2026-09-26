import AxeBuilder from '@axe-core/playwright'
import { expect, test } from '@playwright/test'
import { mkdir } from 'node:fs/promises'
import { join } from 'node:path'

const configuredBindAddress = process.env.AIRTEK_BIND_ADDRESS ?? '127.0.0.1'
const diagnosticHost = configuredBindAddress === '0.0.0.0' || configuredBindAddress === '::'
  ? '127.0.0.1'
  : configuredBindAddress
const urlHost = diagnosticHost.includes(':') ? `[${diagnosticHost}]` : diagnosticHost
const publicOrigin = process.env.E2E_PUBLIC_ORIGIN
  ?? `http://${urlHost}:${process.env.AIRTEK_PUBLIC_HOST_PORT ?? '3000'}`
const productsUrl = new URL('/en/products', `${publicOrigin.replace(/\/$/u, '')}/`).toString()

function severeViolations(
  violations: Awaited<ReturnType<AxeBuilder['analyze']>>['violations'],
) {
  return violations.filter(({ impact }) => impact === 'serious' || impact === 'critical')
}

test.describe('public UI locale preference', () => {
  test('keeps English content contracts while switching the product UI to Simplified Chinese', async ({ page, request }, testInfo) => {
    const ssr = await request.get(productsUrl)
    expect(ssr.status()).toBe(200)
    const ssrHtml = await ssr.text()
    expect(ssrHtml).toContain('lang="en"')
    expect(ssrHtml).toContain('Search the published catalog')
    expect(ssrHtml).not.toContain('搜索已发布产品目录')

    await page.setViewportSize({ width: 390, height: 900 })
    await page.goto(productsUrl)
    await page.evaluate(() => window.localStorage.removeItem('airtek.public.ui-locale.v1'))
    await page.reload()

    await expect(page.locator('html')).toHaveAttribute('lang', 'en')
    await expect(page.getByLabel('Search the published catalog')).toBeVisible()
    await expect(page.getByRole('heading', { name: 'E2E-MODEL-001' })).toBeVisible()
    const canonical = page.locator('link[rel="canonical"]')
    await expect(canonical).toHaveAttribute('href', /\/en\/products$/u)

    await page.getByRole('button', { name: 'Open menu' }).click()
    const language = page.getByLabel('Interface language')
    await language.focus()
    await expect(language).toBeFocused()
    await language.selectOption('zh-CN')
    await page.getByRole('button', { name: '关闭菜单' }).click()

    await expect(page.getByLabel('搜索已发布产品目录')).toBeVisible()
    await expect(page.getByRole('button', { name: '卡片' })).toBeVisible()
    await expect(page.getByRole('button', { name: '比较' })).toBeVisible()
    await expect(page.locator('.product-filter-panel')).toHaveAttribute('aria-label', '产品筛选')
    await expect(page.locator('.product-filter-panel').locator('..')).toHaveAttribute('lang', 'zh-CN')
    await expect(page.getByRole('heading', { name: 'E2E-MODEL-001' })).toBeVisible()
    const productCard = page.locator('article.product-card').filter({ hasText: 'E2E-MODEL-001' })
    await expect(productCard).toContainText('E2E published product summary.')
    const cookieBanner = page.getByRole('complementary', { name: '分析 Cookie 偏好' })
    await expect(cookieBanner).toContainText('隐私选项')
    await cookieBanner.getByRole('button', { name: '保持关闭分析功能' }).click()
    await productCard.getByRole('button', { name: '比较' }).click()
    const compareTray = page.getByRole('complementary', { name: '产品比较栏' })
    await expect(compareTray).toContainText('已选择 1/4 条记录')
    await compareTray.getByRole('button', { name: /从比较中移除/u }).click()
    await expect(page).toHaveURL(/\/en\/products/u)
    await expect(canonical).toHaveAttribute('href', /\/en\/products$/u)

    const catalogResponse = page.waitForResponse((response) => {
      const url = new URL(response.url())
      return url.pathname === '/api/public/v1/products' && url.searchParams.get('q') === 'E2E-MODEL-001'
    })
    const search = page.getByLabel('搜索已发布产品目录')
    await search.fill('E2E-MODEL-001')
    await search.press('Enter')
    const response = await catalogResponse
    const apiUrl = new URL(response.url())
    expect(apiUrl.searchParams.get('locale')).not.toBe('zh-CN')
    expect(apiUrl.pathname).toBe('/api/public/v1/products')
    const catalog = await response.json() as { items?: Array<{ model?: string; title?: string; summary?: string }> }
    expect(catalog.items?.[0]).toMatchObject({
      model: 'E2E-MODEL-001',
      title: 'E2E imported product',
      summary: 'E2E published product summary.',
    })

    await page.evaluate(() => {
      if (document.activeElement instanceof HTMLElement) document.activeElement.blur()
    })
    const evidenceDirectory = join(process.cwd(), '.local', 'qa', 'evidence')
    await mkdir(evidenceDirectory, { recursive: true })
    const screenshotPath = join(evidenceDirectory, 'products-zh-CN.png')
    const stickyHeader = page.locator('.site-header')
    await stickyHeader.evaluate((element) => {
      if (element instanceof HTMLElement) element.style.visibility = 'hidden'
    })
    await page.locator('section.section.shell').screenshot({ path: screenshotPath })
    await stickyHeader.evaluate((element) => {
      if (element instanceof HTMLElement) element.style.visibility = ''
    })
    await testInfo.attach('中文产品目录', { path: screenshotPath, contentType: 'image/png' })

    const accessibility = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
      .analyze()
    expect(severeViolations(accessibility.violations)).toEqual([])
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)
    expect(overflow).toBeLessThanOrEqual(1)

    await page.reload()
    await expect(page.getByLabel('搜索已发布产品目录')).toBeVisible()
    expect(await page.evaluate(() => window.localStorage.getItem('airtek.public.ui-locale.v1'))).toBe('zh-CN')
    await page.getByRole('button', { name: '打开菜单' }).click()
    await page.getByLabel('界面语言').selectOption('en')
    await expect(page.getByLabel('Search the published catalog')).toBeVisible()
  })
})
