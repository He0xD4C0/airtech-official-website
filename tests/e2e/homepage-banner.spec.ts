import { execFileSync } from 'node:child_process'
import { createHash, randomUUID } from 'node:crypto'
import AxeBuilder from '@axe-core/playwright'
import { expect, test, type Page } from '@playwright/test'
import type { CmsPrivateDraft, CmsPublishedContent } from '../../packages/contracts/src/generated/openapi'
import { absolute, adminOrigin, adminStorageStatePath, apiOrigin, publicOrigin, isolatedStack } from './support/environment'

const IMAGE = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEklEQVR4nGP4z8DAAMH///8HACDtBftEyeG3AAAAAElFTkSuQmCC', 'base64')

async function adminApi<T>(page: Page, path: string, method = 'GET', body?: unknown, version?: number): Promise<T> {
  return await page.evaluate(async ({ origin, path, method, body, version }) => {
    const session = await fetch(`${origin}/api/admin/v1/auth/session`, { credentials: 'include' })
    const response = await fetch(`${origin}/api/admin/v1${path}`, {
      method, credentials: 'include', headers: {
        'Content-Type': 'application/json',
        ...(method === 'GET' ? {} : { 'X-CSRF-Token': session.headers.get('x-csrf-token')!, 'Idempotency-Key': crypto.randomUUID() }),
        ...(version === undefined ? {} : { 'If-Match': `"draft-${version}"` }),
      }, ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    })
    if (!response.ok) throw new Error(`Banner API ${path}: ${response.status} ${await response.text()}`)
    return await response.json()
  }, { origin: apiOrigin, path, method, body, version }) as T
}

function sql(statement: string): string {
  const project = process.env.COMPOSE_PROJECT_NAME ?? ''
  const database = process.env.POSTGRES_DB ?? ''
  if (!/^airtek(?:power)?-e2e-[a-z0-9-]+$/.test(project) || !/^airtek_test_[a-f0-9]+$/.test(database)) {
    throw new Error('Banner database assertions require a disposable E2E database.')
  }
  return execFileSync('docker', ['compose', 'exec', '-T', 'postgres', 'psql', '-U', 'airtek', '-d', database, '-Atc', statement], { encoding: 'utf8' }).trim()
}

test.describe('CMS-managed homepage banners', () => {
  test.skip(!isolatedStack, 'Banner publishing uses only the disposable full stack.')
  test.use({ storageState: adminStorageStatePath })

  test('uploads, persists and publishes three layers and controllable slides', async ({ page, context }, testInfo) => {
    test.setTimeout(120_000)
    await page.goto(absolute(adminOrigin, '/content/published'))
    const records = await adminApi<{ items: CmsPublishedContent[] }>(page, '/published-content?limit=100')
    const home = records.items.find(record => record.document.kind === 'home')!
    expect(home).toBeTruthy()
    const original = structuredClone(home.document)
    let draft = await adminApi<CmsPrivateDraft>(page, `/published-content/${home.contentId}/drafts`, 'POST')
    const visitor = await context.newPage()
    const banner = visitor.locator('.banner-carousel')
    const heading = () => visitor.locator('.banner-carousel__slide:not([aria-hidden]) h1')
    try {
      await visitor.goto(absolute(publicOrigin, '/en'))
      await expect(banner.locator('.banner-carousel__slide')).toHaveCount(1)
      await expect(banner.locator('.banner-carousel__controls')).toHaveCount(0)
      await expect(banner.locator('.banner-carousel__gradient')).toBeVisible()
      await page.goto(absolute(adminOrigin, `/content/drafts/${draft.draftId}`))
      const editor = page.getByRole('region', { name: '首页 Banner 管理' })
      await expect(editor.getByRole('button', { name: '删除 Banner 1', exact: true })).toBeDisabled()
      for (let index = 1; index <= 3; index += 1) {
        if (index > 1) await editor.getByRole('button', { name: '添加 Banner 页面' }).click()
        await editor.getByLabel('主标题', { exact: true }).fill(`E2E Banner ${index}`)
        await editor.getByLabel('引导文案', { exact: true }).fill(`Controlled banner text ${index}`)
        await editor.locator('input[type=file]').setInputFiles({ name: `banner-${randomUUID()}.png`, mimeType: 'image/png', buffer: IMAGE })
        await editor.locator('.media-field__alt input').fill(`Banner image ${index}`)
      }
      await editor.getByRole('button', { name: '添加 Banner 页面' }).click()
      await editor.getByRole('button', { name: '删除 Banner 4', exact: true }).click()
      await editor.getByRole('button', { name: '上移 Banner 3', exact: true }).click()
      await page.getByRole('button', { name: '本地预览', exact: true }).click()
      const preview = page.getByRole('region', { name: '编辑器内存预览' })
      await preview.getByRole('button', { name: '下一页 Banner', exact: true }).click()
      await expect(preview.locator('.banner-carousel__slide:not([aria-hidden])')).toContainText('E2E Banner 3')
      await page.getByRole('button', { name: '保存', exact: true }).click()
      await expect(page.locator('.save-state')).toContainText('已保存', { timeout: 20_000 })
      draft = await adminApi<CmsPrivateDraft>(page, `/content-drafts/${draft.draftId}`)
      const heroes = draft.document.composition.blocks.filter(block => block.type === 'hero')
      expect(heroes.map(block => block.heading)).toEqual(['E2E Banner 1', 'E2E Banner 3', 'E2E Banner 2'])
      expect(sql(`SELECT document->'composition'->'blocks' FROM cms_drafts WHERE draft_id='${draft.draftId}'`)).toContain('E2E Banner 3')
      for (const hero of heroes) {
        const id = hero.media!.asset.assetId
        const asset = await adminApi<{ publicUrl: string; previewUrl: string }>(page, `/media/assets/${id}`)
        expect(sql(`SELECT storage_key || '|' || public_url FROM media_assets WHERE id='${id}'`)).toContain(asset.publicUrl)
        const url = new URL(asset.publicUrl); url.hostname = '127.0.0.1'
        const object = await page.request.get(url.toString())
        expect(object.ok()).toBe(true)
        expect(createHash('sha256').update(await object.body()).digest('hex')).toBe(createHash('sha256').update(IMAGE).digest('hex'))
      }
      await page.reload()
      await expect(page.getByRole('region', { name: '首页 Banner 管理' })).toContainText('3 页')
      await visitor.reload()
      await expect(heading()).not.toContainText('E2E Banner')
      const invalidId = heroes[0]!.media!.asset.assetId
      sql(`UPDATE media_assets SET media_type='application/pdf' WHERE id='${invalidId}'`)
      try {
        const blocked = page.waitForResponse(response => response.url().endsWith('/submit') && response.request().method() === 'POST')
        await page.getByRole('button', { name: '提交审核', exact: true }).click()
        expect((await blocked).status()).toBe(422)
        await expect(page).toHaveURL(/\/content\/drafts\//)
      } finally { sql(`UPDATE media_assets SET media_type='image/png' WHERE id='${invalidId}'`) }
      await page.getByRole('button', { name: '提交审核', exact: true }).click()
      await expect(page).toHaveURL(/\/content\/published\//)
      await visitor.clock.install()
      await visitor.goto(absolute(publicOrigin, '/en'))
      await visitor.mouse.move(0, 0)
      await expect(heading()).toHaveText('E2E Banner 1')
      await expect(banner.locator('img').first()).toHaveJSProperty('naturalWidth', 2)
      expect(sql(`SELECT document->'composition'->'blocks' FROM cms_published_content WHERE content_id='${home.contentId}'`)).toContain('E2E Banner 3')
      await visitor.clock.fastForward(6100)
      await expect(heading()).toHaveText('E2E Banner 3')
      await banner.hover()
      await visitor.clock.fastForward(12000)
      await expect(heading()).toHaveText('E2E Banner 3')
      await banner.getByRole('button', { name: 'Pause banners' }).click()
      await visitor.mouse.move(0, 0)
      await visitor.locator('body').click({ position: { x: 1, y: 1 } })
      await visitor.clock.fastForward(12000)
      await expect(heading()).toHaveText('E2E Banner 3')
      await banner.getByRole('button', { name: 'Next banner' }).click()
      await expect(heading()).toHaveText('E2E Banner 2')
      await banner.press('ArrowLeft')
      await expect(heading()).toHaveText('E2E Banner 3')
      await visitor.emulateMedia({ reducedMotion: 'reduce' })
      await visitor.reload()
      await visitor.clock.fastForward(12000)
      await expect(heading()).toHaveText('E2E Banner 1')
      await expect(heading()).toHaveCSS('color', 'rgb(255, 255, 255)')
      const audit = await new AxeBuilder({ page: visitor }).include('.banner-carousel').analyze()
      expect(audit.violations.filter(issue => ['serious', 'critical'].includes(issue.impact ?? ''))).toEqual([])
      await visitor.setViewportSize({ width: 390, height: 844 })
      await banner.dispatchEvent('touchstart', { touches: [{ identifier: 1, clientX: 300, clientY: 200 }] })
      await banner.dispatchEvent('touchend', { changedTouches: [{ identifier: 1, clientX: 100, clientY: 210 }] })
      await expect(heading()).toHaveText('E2E Banner 3')
      expect(await visitor.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
      await visitor.screenshot({ path: testInfo.outputPath('banner-mobile.png') })
      await visitor.route('**/airtek-media/**', route => route.abort())
      await visitor.reload()
      await expect(banner.locator('.banner-carousel__slide:not([aria-hidden]) img')).toHaveCount(0)
      await expect(heading()).toHaveText('E2E Banner 1')
      await expect(banner.locator('.banner-carousel__gradient').first()).toBeVisible()
    } finally {
      const restored = await adminApi<CmsPrivateDraft>(page, `/published-content/${home.contentId}/drafts`, 'POST')
      const saved = await adminApi<CmsPrivateDraft>(page, `/content-drafts/${restored.draftId}`, 'PATCH', { ...original, draftVersion: restored.draftVersion }, restored.draftVersion)
      await adminApi(page, `/content-drafts/${saved.draftId}/submit`, 'POST', undefined, saved.draftVersion)
      await visitor.close()
    }
  })
})
