import AxeBuilder from '@axe-core/playwright'
import { expect, test, type Page } from '@playwright/test'
import {
  absolute,
  adminOrigin,
  adminSecondaryStorageStatePath,
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

async function chooseArticleTemplate(page: Page): Promise<void> {
  const cards = page.locator('.create-form__cards')
  await cards.getByRole('button', { name: /^文章 /u }).click()
  await cards.getByRole('button').filter({ hasText: 'articleDetail' }).click()
}

test.describe('Unified CMS content editor', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack so mutations use a disposable PostgreSQL volume.')
  test.describe.configure({ mode: 'serial' })
  test.use({ storageState: adminStorageStatePath })

  test('serves private draft search and counts from the API', async ({ page }) => {
    const listRequests: string[] = []
    page.on('request', (request) => {
      if (request.url().includes('/api/admin/v1/content-drafts?')) listRequests.push(request.url())
    })

    await page.goto(absolute(adminOrigin, '/content/drafts'))
    await expect(page.getByRole('heading', { name: '私人草稿', exact: true })).toBeVisible()
    await page.getByPlaceholder('按标题或 slug 搜索').fill('airtek-e2e')
    await page.getByRole('button', { name: '搜索' }).click()
    await expect.poll(() => listRequests.some((url) => url.includes('q=airtek-e2e')), { timeout: 5_000 }).toBe(true)
  })

  test('creates, explicitly saves and reloads a draft without switching kind or template', async ({ page }, testInfo) => {
    const slug = `e2e-cms-round-trip-${testInfo.workerIndex}-${testInfo.retry}`
    await page.goto(absolute(adminOrigin, '/content/drafts/new'))
    await chooseArticleTemplate(page)
    await page.getByLabel('标题', { exact: true }).fill('E2E CMS round trip')
    await page.getByLabel(/Slug/u).fill(slug)
    await page.getByRole('button', { name: '创建草稿' }).click()

    await expect(page.getByRole('heading', { name: '基本信息' })).toBeVisible()
    await expect(page.getByText('articleDetail').first()).toBeVisible()
    await expect(page.getByLabel('内容类型')).toHaveCount(0)
    await expect(page.getByRole('combobox', { name: /模板|Template/u })).toHaveCount(0)

    const writes: string[] = []
    page.on('request', (request) => {
      if (request.method() !== 'GET') writes.push(request.url())
    })
    await page.getByLabel('标题', { exact: true }).fill('E2E CMS round trip updated')
    await expect(page.locator('.save-state')).toContainText('有未保存更改')
    await page.keyboard.press(process.platform === 'darwin' ? 'Meta+Z' : 'Control+Z')
    await expect(page.getByLabel('标题', { exact: true })).toHaveValue('E2E CMS round trip')
    await page.keyboard.press(process.platform === 'darwin' ? 'Meta+Shift+Z' : 'Control+Shift+Z')
    await expect(page.getByLabel('标题', { exact: true })).toHaveValue('E2E CMS round trip updated')
    await page.getByRole('button', { name: '本地预览' }).click()
    await expect(page.getByRole('region', { name: '编辑器内存预览' })).toContainText('E2E CMS round trip updated')
    expect(writes).toEqual([])
    await page.getByRole('button', { name: '保存', exact: true }).click()
    await expect(page.locator('.save-state')).toContainText('已保存')
    expect(writes).toHaveLength(1)

    await page.reload()
    await expect(page.getByLabel('标题', { exact: true })).toHaveValue('E2E CMS round trip updated')
  })

  test('derives canonical as a read-only value', async ({ page }, testInfo) => {
    const slug = `e2e-canonical-derivation-${testInfo.workerIndex}-${testInfo.retry}`
    await page.goto(absolute(adminOrigin, '/content/drafts/new'))
    await chooseArticleTemplate(page)
    await page.getByLabel('标题', { exact: true }).fill('E2E canonical derivation')
    await page.getByLabel(/Slug/u).fill(slug)
    await page.getByRole('button', { name: '创建草稿' }).click()

    await page.getByRole('tab', { name: 'SEO', exact: true }).click()
    await expect(page.locator('.canonical-preview code')).toHaveText(`/en/resources/articles/${slug}`)
    await expect(page.getByLabel(/canonical/i)).toHaveCount(0)
    expect(await page.locator(`input[value="${slug}"]`).count()).toBe(1)
  })

  test('stops a stale second editor from overwriting the server draft', async ({ browser }, testInfo) => {
    const suffix = `${Date.now().toString(36)}-${testInfo.workerIndex}-${testInfo.retry}`
    const slug = `e2e-concurrent-edit-${suffix}`
    const initialTitle = `E2E concurrent edit ${suffix}`
    const savedByA = `E2E saved by session A ${suffix}`
    const staleInB = `E2E stale edit from session B ${suffix}`
    const postConflictInB = `E2E blocked edit from session B ${suffix}`
    const contextA = await browser.newContext({ storageState: adminStorageStatePath })
    const contextB = await browser.newContext({ storageState: adminSecondaryStorageStatePath })
    const pageA = await contextA.newPage()
    const pageB = await contextB.newPage()

    try {
      await pageA.goto(absolute(adminOrigin, '/content/drafts/new'))
      await chooseArticleTemplate(pageA)
      await pageA.getByLabel('标题', { exact: true }).fill(initialTitle)
      await pageA.getByLabel(/Slug/u).fill(slug)
      await pageA.getByRole('button', { name: '创建草稿' }).click()
      await expect(pageA.getByRole('heading', { name: '基本信息' })).toBeVisible()

      const editorUrl = pageA.url()
      const routeMatch = new URL(editorUrl).pathname.match(/^\/content\/drafts\/([^/]+)$/u)
      if (!routeMatch?.[1]) throw new Error(`Unexpected content editor URL: ${editorUrl}`)
      const draftPath = `/api/admin/v1/content-drafts/${routeMatch[1]}`
      const isDraftRequest = (url: string, method: string): boolean => (
        new URL(url).pathname === draftPath && method === 'GET'
      )
      const readA = pageA.waitForResponse((response) => (
        isDraftRequest(response.url(), response.request().method())
      ))
      const readB = pageB.waitForResponse((response) => (
        isDraftRequest(response.url(), response.request().method())
      ))

      await Promise.all([pageA.reload(), pageB.goto(editorUrl)])
      const [initialA, initialB] = await Promise.all([readA, readB])
      const recordA = await initialA.json() as { draftVersion: number }
      const recordB = await initialB.json() as { draftVersion: number }
      const initialEtag = initialA.headers().etag

      expect(initialA.ok()).toBe(true)
      expect(initialB.ok()).toBe(true)
      expect(recordA.draftVersion).toBeGreaterThan(0)
      expect(recordB.draftVersion).toBe(recordA.draftVersion)
      expect(initialEtag).toBe(`"draft-${recordA.draftVersion}"`)
      expect(initialB.headers().etag).toBe(initialEtag)
      await expect(pageA.getByLabel('标题', { exact: true })).toHaveValue(initialTitle)
      await expect(pageB.getByLabel('标题', { exact: true })).toHaveValue(initialTitle)

      const patchRequestsFromB: string[] = []
      pageB.on('request', (request) => {
        if (new URL(request.url()).pathname === draftPath && request.method() === 'PATCH') {
          patchRequestsFromB.push(request.headers()['if-match'] ?? '')
        }
      })

      await pageA.getByLabel('标题', { exact: true }).fill(savedByA)
      const saveA = pageA.waitForResponse((response) => (
        new URL(response.url()).pathname === draftPath
        && response.request().method() === 'PATCH'
      ))
      await pageA.getByRole('button', { name: '保存', exact: true }).click()
      const savedResponse = await saveA
      const savedRecord = await savedResponse.json() as { document: { title: string } }
      expect(savedResponse.status()).toBe(200)
      expect(savedResponse.request().headers()['if-match']).toBe(initialEtag)
      expect(savedRecord.document.title).toBe(savedByA)
      await expect(pageA.locator('.save-state')).toContainText('已保存')

      await pageB.getByLabel('标题', { exact: true }).fill(staleInB)
      const saveB = pageB.waitForResponse((response) => (
        new URL(response.url()).pathname === draftPath
        && response.request().method() === 'PATCH'
      ))
      await pageB.getByRole('button', { name: '保存', exact: true }).click()
      const conflictResponse = await saveB
      expect(conflictResponse.status()).toBe(409)
      expect(conflictResponse.request().headers()['if-match']).toBe(initialEtag)

      await expect(pageB.locator('.save-state')).toHaveText('版本冲突')
      await expect(pageB.locator('.save-error')).toBeVisible()
      await expect(pageB.getByLabel('标题', { exact: true })).toHaveValue(staleInB)
      await pageB.getByLabel('标题', { exact: true }).fill(postConflictInB)
      await pageB.waitForTimeout(1_500)
      expect(patchRequestsFromB).toEqual([initialEtag])
      const reloadB = pageB.waitForResponse((response) => (
        isDraftRequest(response.url(), response.request().method())
      ))
      await pageB.getByRole('button', { name: '重新载入' }).click()
      expect((await reloadB).ok()).toBe(true)
      await expect(pageB.getByLabel('标题', { exact: true })).toHaveValue(savedByA)
    } finally {
      await contextB.close()
      await contextA.close()
    }
  })

  test('supports keyboard operation and passes axe checks', async ({ page }, testInfo) => {
    await page.goto(absolute(adminOrigin, '/content/drafts'))
    const listResults = await analyzeStable(page)
    expect(describeViolations(listResults.violations)).toBe('')

    await page.goto(absolute(adminOrigin, '/content/drafts/new'))
    const createResults = await analyzeStable(page)
    expect(describeViolations(createResults.violations)).toBe('')

    await chooseArticleTemplate(page)
    await page.getByLabel('标题', { exact: true }).fill('E2E keyboard editor')
    await page.getByLabel(/Slug/u).fill(`e2e-keyboard-editor-${testInfo.workerIndex}-${testInfo.retry}`)
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

    const blockTab = page.getByRole('tab', { name: '区块', exact: true })
    await blockTab.focus()
    await page.keyboard.press('Enter')
    await expect(blockTab).toHaveAttribute('aria-selected', 'true')
  })
})
