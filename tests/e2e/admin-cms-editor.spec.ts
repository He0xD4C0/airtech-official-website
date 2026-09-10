import AxeBuilder from '@axe-core/playwright'
import { expect, test, type Page } from '@playwright/test'
import {
  absolute,
  adminOrigin,
  adminStorageStatePath,
  runAdminWorkflows,
} from './support/environment'

function severeViolations(
  violations: Awaited<ReturnType<AxeBuilder['analyze']>>['violations'],
) {
  return violations.filter(({ impact }) => impact === 'serious' || impact === 'critical')
}

function describeViolations(
  violations: Awaited<ReturnType<AxeBuilder['analyze']>>['violations'],
): string {
  return severeViolations(violations).flatMap((violation) => violation.nodes.map((node) => {
    const data = (node.any?.[0]?.data ?? {}) as Record<string, unknown>
    const detail = ['fgColor', 'bgColor', 'contrastRatio']
      .filter((key) => data[key] !== undefined)
      .map((key) => `${key}=${String(data[key])}`)
      .join(' ')
    return `${violation.id} ${violation.impact} ${node.target.join(' ')} ${detail}`.trim()
  })).join('\n')
}

async function analyzeStable(page: Page): Promise<Awaited<ReturnType<AxeBuilder['analyze']>>> {
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.waitForTimeout(400)
  return new AxeBuilder({ page }).analyze()
}

test.describe('Unified CMS content editor', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack so mutations use a disposable PostgreSQL volume.')
  test.describe.configure({ mode: 'serial' })
  test.use({ storageState: adminStorageStatePath })

  test('serves list search, filters and type counts from the API', async ({ page }) => {
    const listRequests: string[] = []
    page.on('request', (request) => {
      if (request.url().includes('/api/admin/v1/content?')) listRequests.push(request.url())
    })

    await page.goto(absolute(adminOrigin, '/content?kind=news'))
    await expect(page.getByRole('heading', { name: '新闻内容' })).toBeVisible()
    await expect(page.locator('.content-type-strip')).toContainText('全部内容')
    await expect(page.locator('.content-type-strip button[aria-pressed="true"]')).toContainText('新闻')

    const rows = page.locator('table.data-table tbody tr')
    for (const row of await rows.all()) {
      await expect(row.locator('td').nth(1)).toHaveText('新闻')
    }

    await page.getByPlaceholder('按标题或 slug 搜索（服务端）').fill('airtek-e2e')
    await expect.poll(() => listRequests.some((url) => url.includes('q=airtek-e2e')), { timeout: 5_000 }).toBe(true)
    expect(listRequests.some((url) => url.includes('kind=news'))).toBe(true)
  })

  test('creates, autosaves and reloads a draft without switching kind or template', async ({ page }) => {
    await page.goto(absolute(adminOrigin, '/content/new'))
    await page.locator('input[type="radio"][value="articleDetail"]').check()
    await page.getByLabel('标题', { exact: true }).fill('E2E CMS round trip')
    await page.getByLabel(/Slug/u).fill('e2e-cms-round-trip')
    await page.getByRole('button', { name: '创建草稿' }).click()

    await expect(page.getByRole('heading', { name: '基本信息' })).toBeVisible()
    await expect(page.getByText('articleDetail').first()).toBeVisible()
    await expect(page.getByLabel('内容类型')).toHaveCount(0)
    await expect(page.getByRole('combobox', { name: /模板|Template/u })).toHaveCount(0)

    await page.getByLabel('标题', { exact: true }).fill('E2E CMS round trip updated')
    await expect(page.locator('.save-state')).toContainText('已保存', { timeout: 15_000 })

    await page.reload()
    await expect(page.getByLabel('标题', { exact: true })).toHaveValue('E2E CMS round trip updated')
  })

  test('derives canonical as a read-only value', async ({ page }) => {
    await page.goto(absolute(adminOrigin, '/content/new'))
    await page.locator('input[type="radio"][value="articleDetail"]').check()
    await page.getByLabel('标题', { exact: true }).fill('E2E canonical derivation')
    await page.getByLabel(/Slug/u).fill('e2e-canonical-derivation')
    await page.getByRole('button', { name: '创建草稿' }).click()

    await page.getByRole('tab', { name: 'SEO' }).click()
    await expect(
      page.locator('code').filter({ hasText: '/en/e2e-canonical-derivation' }).first(),
    ).toBeVisible()
    await expect(page.getByLabel(/canonical/i)).toHaveCount(0)
    expect(await page.locator('input[value*="e2e-canonical-derivation"]').count()).toBe(1)
  })

  test('supports keyboard operation and passes axe checks', async ({ page }) => {
    await page.goto(absolute(adminOrigin, '/content'))
    const listResults = await analyzeStable(page)
    expect(describeViolations(listResults.violations)).toBe('')

    await page.goto(absolute(adminOrigin, '/content/new'))
    const createResults = await analyzeStable(page)
    expect(describeViolations(createResults.violations)).toBe('')

    await page.locator('input[type="radio"][value="articleDetail"]').check()
    await page.getByLabel('标题', { exact: true }).fill('E2E keyboard editor')
    await page.getByLabel(/Slug/u).fill('e2e-keyboard-editor')
    await page.getByRole('button', { name: '创建草稿' }).click()
    await expect(page.getByRole('heading', { name: '基本信息' })).toBeVisible()

    const editorResults = await analyzeStable(page)
    expect(describeViolations(editorResults.violations)).toBe('')

    const reachedNames = new Set<string>()
    for (let index = 0; index < 60; index += 1) {
      await page.keyboard.press('Tab')
      const name = await page.evaluate(() => {
        const active = document.activeElement as HTMLElement | null
        return active?.getAttribute('aria-label') ?? active?.textContent?.trim() ?? ''
      })
      if (name) reachedNames.add(name)
    }
    expect([...reachedNames].some((name) => name.includes('保存'))).toBe(true)

    await page.getByRole('tab', { name: '区块' }).focus()
    await page.keyboard.press('Enter')
    await expect(page.getByRole('tab', { name: '区块' })).toHaveAttribute('aria-selected', 'true')
  })
})
