import AxeBuilder from '@axe-core/playwright'
import { expect, test, type Page } from '@playwright/test'
import { bannerVisualImage } from './support/banner-visual'
import {
  absolute,
  adminOrigin,
  adminStorageStatePath,
  apiOrigin,
  mediaPublicBaseUrl,
  runAdminWorkflows,
} from './support/environment'

const RASTER_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEklEQVR4nGP4z8DAAMH///8HACDtBftEyeG3AAAAAElFTkSuQmCC',
  'base64',
)

interface MediaAsset {
  id: string
  publicUrl: string
  downloadUrl: string
  originalName: string
  mediaType: string
  byteSize: number
  sha256: string
}

function severeViolations(
  violations: Awaited<ReturnType<AxeBuilder['analyze']>>['violations'],
) {
  return violations.filter(({ impact }) => impact === 'serious' || impact === 'critical')
}

function hostReadableUrl(publicUrl: string): string {
  const url = new URL(publicUrl)
  const hostname = url.hostname.toLowerCase()
  if (hostname === 'media.localhost' || hostname.endsWith('.airtek.test')) {
    url.hostname = '127.0.0.1'
  }
  return url.toString()
}

function uppercaseMediaHostname(publicBaseUrl: string): string {
  const url = new URL(publicBaseUrl)
  const authority = `${url.hostname.toUpperCase()}${url.port ? `:${url.port}` : ''}`
  return `${url.protocol}//${authority}${url.pathname.replace(/\/$/u, '')}`
}

async function uploadFromBrowser(
  page: Page,
  key: string,
  fileName: string,
  bytes: Buffer,
): Promise<{ status: number; body: unknown }> {
  await page.waitForLoadState('networkidle')
  return await page.evaluate(async ({ apiBase, idempotencyKey, name, base64 }) => {
    const session = await fetch(`${apiBase}/api/admin/v1/auth/session`, { credentials: 'include' })
    const csrf = session.headers.get('x-csrf-token')
    if (!csrf) throw new Error('Admin session did not provide a CSRF token.')
    const raw = atob(base64)
    const data = new Uint8Array(raw.length)
    for (let index = 0; index < raw.length; index += 1) data[index] = raw.charCodeAt(index)
    const form = new FormData()
    form.append('file', new File([data], name, { type: 'image/png' }))
    const response = await fetch(`${apiBase}/api/admin/v1/media/assets`, {
      method: 'POST',
      credentials: 'include',
      headers: { 'Idempotency-Key': idempotencyKey, 'X-CSRF-Token': csrf },
      body: form,
    })
    return { status: response.status, body: await response.json() as unknown }
  }, { apiBase: apiOrigin, idempotencyKey: key, name: fileName, base64: bytes.toString('base64') })
}

async function updatePublicBaseUrl(page: Page, publicBaseUrl: string): Promise<void> {
  const result = await page.evaluate(async ({ apiBase, nextPublicBaseUrl }) => {
    const session = await fetch(`${apiBase}/api/admin/v1/auth/session`, { credentials: 'include' })
    const current = await fetch(`${apiBase}/api/admin/v1/settings/object-storage`, { credentials: 'include' })
    const csrf = session.headers.get('x-csrf-token')
    const etag = current.headers.get('etag')
    const settings = await current.json() as Record<string, unknown>
    if (!session.ok || !current.ok || !csrf || !etag) throw new Error('Unable to load object storage settings and mutation headers.')
    const updated = await fetch(`${apiBase}/api/admin/v1/settings/object-storage`, {
      method: 'PUT',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json', 'If-Match': etag, 'X-CSRF-Token': csrf },
      body: JSON.stringify({
        endpoint: settings.endpoint,
        region: settings.region,
        bucket: settings.bucket,
        accessKeyId: settings.accessKeyId,
        secretAccessKey: '',
        keyPrefix: settings.keyPrefix,
        pathStyle: settings.pathStyle,
        publicBaseUrl: nextPublicBaseUrl,
        adoptLegacyAssets: false,
        reason: 'Verify immutable media URLs after an object storage settings update.',
      }),
    })
    return { status: updated.status, body: await updated.text() }
  }, { apiBase: apiOrigin, nextPublicBaseUrl: publicBaseUrl })
  expect(result.status, result.body).toBe(200)
}

test.describe('direct public media upload', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack so uploads use a disposable stack.')
  test.describe.configure({ mode: 'serial' })
  test.use({ storageState: adminStorageStatePath })

  test('returns 201 and serves the immutable external object URL immediately', async ({ page }, testInfo) => {
    const fileName = `e2e-direct-${testInfo.workerIndex}-${Date.now().toString(36)}.png`
    await page.goto(absolute(adminOrigin, '/media'))
    await expect(page.getByRole('heading', { name: '媒体库', exact: true })).toBeVisible()

    const responsePromise = page.waitForResponse((response) => (
      new URL(response.url()).pathname === '/api/admin/v1/media/assets'
      && response.request().method() === 'POST'
    ))
    await page.setInputFiles('input[type="file"]', {
      name: fileName,
      mimeType: 'image/png',
      buffer: RASTER_PNG,
    })
    const uploadResponse = await responsePromise
    expect(uploadResponse.status()).toBe(201)
    const asset = await uploadResponse.json() as MediaAsset
    expect(asset).toMatchObject({ originalName: fileName, mediaType: 'image/png', byteSize: RASTER_PNG.byteLength })
    expect(asset.sha256).toMatch(/^[a-f0-9]{64}$/u)

    expect(asset.publicUrl.startsWith(`${mediaPublicBaseUrl}/media/`)).toBe(true)
    const publicResponse = await page.request.get(hostReadableUrl(asset.publicUrl))
    expect(publicResponse.status()).toBe(200)
    expect(publicResponse.headers()['content-type']).toBe('image/png')
    expect((await publicResponse.body()).equals(RASTER_PNG)).toBe(true)
    const downloadResponse = await page.request.get(absolute(apiOrigin, asset.downloadUrl), { maxRedirects: 0 })
    expect(downloadResponse.status()).toBe(308)
    expect(downloadResponse.headers().location).toBe(asset.publicUrl)

    const row = page.locator('table.data-table tbody tr', { hasText: fileName })
    await expect(row).toBeVisible()
    const detail = page.getByRole('dialog', { name: fileName })
    if (!await detail.isVisible()) {
      await row.getByRole('button', { name: new RegExp(fileName, 'u') }).click()
    }
    await expect.poll(() => detail.locator('img').evaluate((image) => (image as HTMLImageElement).naturalWidth)).toBeGreaterThan(0)
    await expect(detail.getByText(asset.sha256)).toBeVisible()
    await expect(detail.getByText('暂无已发布内容引用。')).toBeVisible()

    const accessibility = await new AxeBuilder({ page }).analyze()
    expect(severeViolations(accessibility.violations)).toEqual([])
  })

  test('keeps an existing URL unchanged when the public base URL changes', async ({ page }, testInfo) => {
    await page.goto(absolute(adminOrigin, '/media'))
    const suffix = `${testInfo.workerIndex}-${Date.now().toString(36)}`
    const first = await uploadFromBrowser(page, `e2e-url-a-${suffix}`, `url-a-${suffix}.png`, RASTER_PNG)
    expect(first.status).toBe(201)
    const assetA = first.body as MediaAsset

    const changedPublicBaseUrl = uppercaseMediaHostname(mediaPublicBaseUrl)
    await updatePublicBaseUrl(page, changedPublicBaseUrl)
    const second = await uploadFromBrowser(page, `e2e-url-b-${suffix}`, `url-b-${suffix}.png`, RASTER_PNG)
    expect(second.status).toBe(201)
    const assetB = second.body as MediaAsset

    expect(assetA.publicUrl.startsWith(`${mediaPublicBaseUrl}/media/`)).toBe(true)
    expect(assetB.publicUrl.startsWith(`${changedPublicBaseUrl}/media/`)).toBe(true)
    expect(assetB.publicUrl).not.toBe(assetA.publicUrl)
    for (const publicUrl of [assetA.publicUrl, assetB.publicUrl]) {
      const response = await page.request.get(hostReadableUrl(publicUrl))
      expect(response.status()).toBe(200)
      expect((await response.body()).equals(RASTER_PNG)).toBe(true)
    }
  })

  test('replays the same actor/key/file and rejects key reuse for another file', async ({ page }, testInfo) => {
    await page.goto(absolute(adminOrigin, '/media'))
    const key = `e2e-idempotent-${testInfo.workerIndex}-${Date.now().toString(36)}`
    const fileName = `${key}.png`
    const first = await uploadFromBrowser(page, key, fileName, RASTER_PNG)
    const replay = await uploadFromBrowser(page, key, fileName, RASTER_PNG)
    const conflictBytes = Buffer.concat([RASTER_PNG, Buffer.from([0])])
    const conflict = await uploadFromBrowser(page, key, `different-${fileName}`, conflictBytes)

    expect(first.status).toBe(201)
    expect(replay.status).toBe(201)
    expect((replay.body as MediaAsset).id).toBe((first.body as MediaAsset).id)
    expect(conflict.status).toBe(409)
    expect(conflict.body).toMatchObject({
      status: 409,
      type: 'https://api.airtekpower.example/problems/media_idempotency_conflict',
    })
  })

  test('uploads a full-resolution image larger than the gateway default limit', async ({ page, request }, testInfo) => {
    await page.goto(absolute(adminOrigin, '/media'))
    // A valid uncompressed raster exercises real multipart transport above 1 MiB.
    const bytes = bannerVisualImage(0)
    expect(bytes.length).toBeGreaterThan(1024 * 1024)
    const key = `e2e-large-image-${testInfo.workerIndex}-${Date.now().toString(36)}`
    const result = await uploadFromBrowser(page, key, `${key}.png`, bytes)
    expect(result.status).toBe(201)
    const asset = result.body as MediaAsset
    expect(asset.byteSize).toBe(bytes.length)
    const original = await request.get(hostReadableUrl(asset.publicUrl))
    expect(original.status()).toBe(200)
    expect((await original.body()).equals(bytes)).toBe(true)
  })

  test('rejects forged image bytes and keeps the media page usable', async ({ page }) => {
    await page.goto(absolute(adminOrigin, '/media'))
    const responsePromise = page.waitForResponse((response) => (
      new URL(response.url()).pathname === '/api/admin/v1/media/assets'
      && response.request().method() === 'POST'
    ))
    await page.setInputFiles('input[type="file"]', {
      name: 'forged.png',
      mimeType: 'image/png',
      buffer: Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"/>'),
    })
    expect((await responsePromise).status()).toBe(415)
    await expect(page.getByRole('status')).toContainText('PNG')
    await expect(page.getByRole('heading', { name: '媒体库', exact: true })).toBeVisible()
  })
})
