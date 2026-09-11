import AxeBuilder from '@axe-core/playwright'
import { expect, test, type Page } from '@playwright/test'
import {
  absolute,
  adminOrigin,
  adminStorageStatePath,
  gatewayControlOrigin,
  gatewayHostHeaders,
  publicOrigin,
  runAdminWorkflows,
} from './support/environment'

/** A real 1x1 PNG so the upload path exercises magic-byte validation. */
const ONE_PIXEL_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==',
  'base64',
)

function severeViolations(
  violations: Awaited<ReturnType<AxeBuilder['analyze']>>['violations'],
) {
  return violations.filter(({ impact }) => impact === 'serious' || impact === 'critical')
}

async function analyzeStable(page: Page): Promise<Awaited<ReturnType<AxeBuilder['analyze']>>> {
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.waitForTimeout(400)
  return new AxeBuilder({ page }).analyze()
}

test.describe('Media upload pipeline', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack so uploads use a disposable stack.')
  test.describe.configure({ mode: 'serial' })
  test.use({ storageState: adminStorageStatePath })

  test('uploads, reviews, publishes, serves, and revokes a public media object', async ({ page, browser }, testInfo) => {
    test.setTimeout(120_000)
    const suffix = `${testInfo.workerIndex}-${testInfo.retry}-${Date.now().toString(36)}`
    const fileName = `e2e-media-${suffix}.png`
    await page.goto(absolute(adminOrigin, '/media'))
    await expect(page.getByRole('heading', { name: '媒体中心' })).toBeVisible()
    await expect(page.locator('.media-toolbar')).toBeVisible()

    const uploadResponse = page.waitForResponse((response) => (
      response.url().includes('/api/admin/v1/media/uploads')
      && response.request().method() === 'POST'
    ))
    await page.setInputFiles('input[type="file"]', {
      name: fileName,
      mimeType: 'image/png',
      buffer: ONE_PIXEL_PNG,
    })
    const response = await uploadResponse
    expect(response.status()).toBe(201)
    const asset = await response.json() as { id: string, scanStatus: string }

    const row = page.locator('table.data-table tbody tr', { hasText: fileName })
    await expect(row).toBeVisible()
    if (asset.scanStatus !== 'clean') {
      await row.getByRole('button', { name: '标记可用' }).click()
      const approval = page.getByRole('dialog', { name: new RegExp(`标记 ${fileName} 为可用`) })
      await approval.getByRole('textbox').fill('E2E administrator verified the uploaded raster image.')
      await approval.getByRole('button', { name: '确认可用' }).click()
      await expect(approval).toHaveCount(0)
      await expect(row.locator('.status-badge')).toHaveText('clean')
    }

    const objectUrl = absolute(gatewayControlOrigin, `/media/${asset.id}`)
    const publicHeaders = gatewayHostHeaders(publicOrigin)
    const served = await page.request.get(objectUrl, { headers: publicHeaders })
    expect(served.status()).toBe(200)
    expect(served.headers()['content-type']).toBe('image/png')
    expect(served.headers()['cache-control']).toContain('immutable')
    expect(
      served.headers()['x-content-type-options']
        ?.split(',')
        .map((value) => value.trim()),
    ).toEqual(expect.arrayContaining(['nosniff']))
    const notModified = await page.request.get(objectUrl, {
      headers: { ...publicHeaders, 'If-None-Match': served.headers().etag ?? '' },
    })
    expect(notModified.status()).toBe(304)

    const download = await page.request.get(
      absolute(gatewayControlOrigin, `/media/${asset.id}/download`),
      { headers: publicHeaders },
    )
    expect(download.status()).toBe(200)
    expect(download.headers()['content-disposition']).toContain('attachment')

    await row.getByRole('button', { name: '隔离' }).click()
    const dialog = page.getByRole('dialog', { name: new RegExp(`隔离 ${fileName}`) })
    await expect(dialog).toBeVisible()
    await dialog.getByRole('button', { name: '确认隔离' }).click()
    await expect(dialog.getByRole('alert')).toHaveText('人工审核决定必须填写原因。')
    await dialog.getByRole('textbox').fill('E2E verification quarantines the fixture.')
    await dialog.getByRole('button', { name: '确认隔离' }).click()
    await expect(dialog).toHaveCount(0)
    await expect(row.locator('.status-badge')).toHaveText('quarantined')

    const revoked = await page.request.get(objectUrl, { headers: publicHeaders })
    expect(revoked.status()).toBe(404)

    await row.getByRole('button', { name: '标记可用' }).click()
    const approval = page.getByRole('dialog', { name: new RegExp(`标记 ${fileName} 为可用`) })
    await approval.getByRole('button', { name: '确认可用' }).click()
    await expect(approval.getByRole('alert')).toHaveText('人工审核决定必须填写原因。')
    await approval.getByRole('textbox').fill('E2E administrator re-approved the reviewed fixture.')
    await approval.getByRole('button', { name: '确认可用' }).click()
    await expect(approval).toHaveCount(0)
    await expect(row.locator('.status-badge')).toHaveText('clean')
    expect((await page.request.get(objectUrl, { headers: publicHeaders })).status()).toBe(200)

    const slug = `e2e-media-article-${suffix}`
    const canonicalPath = `/en/resources/articles/${slug}`
    const articleTitle = `E2E media SSR ${suffix}`
    const mediaAlt = `Verified uploaded AIRTEK media ${suffix}`
    const mediaCaption = `Uploaded media caption ${suffix}`

    await page.goto(absolute(adminOrigin, '/content/new'))
    await page.locator('input[type="radio"][value="articleDetail"]').check()
    await page.getByLabel('标题', { exact: true }).fill(articleTitle)
    await page.getByLabel(/Slug/u).fill(slug)
    await page.getByRole('button', { name: '创建草稿' }).click()
    await expect(page.getByRole('heading', { name: '基本信息' })).toBeVisible()

    await page.getByRole('button', { name: '添加媒体区块' }).click()
    const blockDialog = page.getByRole('dialog', { name: '选择媒体资产' }).first()
    await expect(blockDialog).toBeVisible()
    await blockDialog.getByRole('button', { name: '选择', exact: true }).click()

    const libraryDialog = page.getByRole('dialog', { name: '选择媒体资产' }).last()
    await expect(libraryDialog).toBeVisible()
    await libraryDialog.getByPlaceholder('按文件名搜索').fill(fileName)
    const assetOption = libraryDialog.locator('button.media-dialog__option', { hasText: fileName })
    await expect(assetOption).toBeEnabled({ timeout: 10_000 })
    await assetOption.click()
    await expect(page.getByRole('dialog', { name: '选择媒体资产' })).toHaveCount(1)
    await blockDialog.getByRole('button', { name: '加入区块' }).click()

    const inspector = page.locator('aside[aria-label="编辑器检查器"]')
    await expect(inspector.getByRole('tab', { name: '区块' })).toHaveAttribute('aria-selected', 'true')
    await inspector.getByLabel('替代文本', { exact: true }).fill(mediaAlt)
    await inspector.getByLabel('图注', { exact: true }).fill(mediaCaption)
    await expect(inspector.getByRole('checkbox', { name: /^装饰性媒体/u })).not.toBeChecked()

    await page.getByLabel('分类').fill('E2E acceptance')
    await page.getByLabel('作者显示名').fill('AIRTEKPOWER E2E')
    const body = page.locator('[contenteditable="true"]').first()
    await body.click()
    await body.fill(`Media pipeline browser acceptance body ${suffix}.`)
    const placeholder = page.getByRole('checkbox', { name: /^占位内容/u })
    if (await placeholder.isChecked()) await placeholder.uncheck()
    await expect(page.locator('.save-state')).toContainText('已保存', { timeout: 15_000 })

    await page.getByRole('button', { name: '发布', exact: true }).click()
    const publishDialog = page.getByRole('dialog', { name: '发布内容' })
    await expect(publishDialog.getByRole('button', { name: '确认发布' })).toBeEnabled()
    await publishDialog.getByRole('button', { name: '确认发布' }).click()
    await expect(page.getByText('内容已发布')).toBeVisible()

    const ssr = await page.request.get(absolute(gatewayControlOrigin, canonicalPath), {
      headers: publicHeaders,
    })
    expect(ssr.status()).toBe(200)
    const ssrMarkup = await ssr.text()
    expect(ssrMarkup).toContain(`src="/media/${asset.id}"`)
    expect(ssrMarkup).toContain(mediaAlt)
    expect(ssrMarkup).toContain(mediaCaption)
    expect(ssrMarkup).toContain(articleTitle)

    const publicContext = await browser.newContext({ javaScriptEnabled: false })
    const publicPage = await publicContext.newPage()
    try {
      const publishedResponse = await publicPage.goto(absolute(publicOrigin, canonicalPath))
      expect(publishedResponse?.status()).toBe(200)
      await expect(publicPage).toHaveTitle(articleTitle)
      await expect(publicPage.getByRole('heading', { level: 1, name: articleTitle })).toBeVisible()
      const image = publicPage.locator(`img[src="/media/${asset.id}"]`)
      await expect(image).toHaveAttribute('alt', mediaAlt)
      await expect(publicPage.locator('figcaption', { hasText: mediaCaption })).toBeVisible()
    } finally {
      await publicContext.close()
    }
  })

  test('rejects unsupported bytes and keeps the media page accessible', async ({ page }) => {
    await page.goto(absolute(adminOrigin, '/media'))
    await expect(page.getByRole('heading', { name: '媒体中心' })).toBeVisible()

    const uploadResponse = page.waitForResponse((response) => (
      response.url().includes('/api/admin/v1/media/uploads')
      && response.request().method() === 'POST'
    ))
    await page.setInputFiles('input[type="file"]', {
      name: 'e2e-not-an-image.svg',
      mimeType: 'image/svg+xml',
      buffer: Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"><script/></svg>'),
    })
    expect((await uploadResponse).status()).toBe(415)
    await expect(page.getByRole('status').first()).toContainText('PNG')

    const accessibility = await analyzeStable(page)
    expect(severeViolations(accessibility.violations)).toEqual([])
  })
})
