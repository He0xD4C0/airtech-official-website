import { deflateSync } from 'node:zlib'
import { expect, type Page, type TestInfo } from '@playwright/test'

function crc32(bytes: Buffer): number {
  let crc = 0xffffffff
  for (const byte of bytes) {
    crc ^= byte
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0)
  }
  return (crc ^ 0xffffffff) >>> 0
}
function chunk(type: string, data: Buffer): Buffer {
  const name = Buffer.from(type)
  const size = Buffer.alloc(4); size.writeUInt32BE(data.length)
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(Buffer.concat([name, data])))
  return Buffer.concat([size, name, data, crc])
}

/** Synthetic bright field and machine-like grid; deliberately not a company asset. */
export function bannerVisualImage(compressionLevel = 6): Buffer {
  const width = 1920, height = 900
  const header = Buffer.alloc(13)
  header.writeUInt32BE(width, 0); header.writeUInt32BE(height, 4)
  header[8] = 8; header[9] = 2
  const stride = width * 3 + 1
  const pixels = Buffer.alloc(stride * height)
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const grid = x > 1100 && (x % 120 < 3 || y % 120 < 3)
      const accent = x > 1240 && x < 1460 && y > 220 && y < 680
      const rgb = accent ? [72, 128, 137] : grid ? [172, 190, 196] : [232, 238, 240]
      const offset = y * stride + 1 + x * 3
      pixels[offset] = rgb[0]!; pixels[offset + 1] = rgb[1]!; pixels[offset + 2] = rgb[2]!
    }
  }
  return Buffer.concat([Buffer.from('89504e470d0a1a0a', 'hex'), chunk('IHDR', header), chunk('IDAT', deflateSync(pixels, { level: compressionLevel })), chunk('IEND', Buffer.alloc(0))])
}

function luminance(rgb: number[]): number {
  const values = rgb.map(value => { const n = value / 255; return n <= .04045 ? n / 12.92 : ((n + .055) / 1.055) ** 2.4 })
  return values[0]! * .2126 + values[1]! * .7152 + values[2]! * .0722
}

export async function verifyBannerVisuals(page: Page, info: TestInfo): Promise<void> {
  const banner = page.locator('.banner-carousel')
  await page.emulateMedia({ reducedMotion: 'reduce' })
  for (const width of [320, 390, 768, 1280, 1920]) {
    await page.setViewportSize({ width, height: 1000 })
    await page.reload()
    await expect(banner.locator('.is-active img')).toHaveJSProperty('naturalWidth', 1600)
    const pause = banner.getByRole('button', { name: 'Play banners' })
    await expect(pause).toBeVisible()
    const heading = banner.locator('.is-active h1')
    const originalHeight = (await banner.boundingBox())!.height
    await banner.getByRole('button', { name: 'Show banner 3' }).click()
    await expect(banner.locator('[aria-current=true]')).toHaveAttribute('aria-label', 'Show banner 3')
    expect((await banner.boundingBox())!.height).toBeCloseTo(originalHeight, 0)
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    const copy = (await heading.boundingBox())!
    const actions = (await banner.locator('.is-active .banner-carousel__actions').boundingBox())!
    const controls = (await banner.locator('.banner-carousel__controls').boundingBox())!
    expect(actions.y + actions.height + 24).toBeLessThanOrEqual(controls.y)
    expect(copy.width).toBeGreaterThan(width < 900 ? Math.min(width - 60, 600) : 500)
    const previous = banner.getByRole('button', { name: 'Previous banner' })
    const next = banner.getByRole('button', { name: 'Next banner' })
    if (width < 900) {
      await expect(previous).toBeHidden(); await expect(next).toBeHidden()
    } else {
      const left = (await previous.boundingBox())!, right = (await next.boundingBox())!
      const root = (await banner.boundingBox())!
      expect(left.x + left.width + 16).toBeLessThanOrEqual(copy.x)
      expect(right.x).toBeGreaterThan(root.x + root.width - 80)
      await expect(heading).toHaveCSS('font-size', `${Math.min(width * .038, 56)}px`)
    }
    await expect(banner.locator('.banner-carousel__dot').first()).toHaveText('')
    const screenshot = await banner.screenshot({ path: info.outputPath(`banner-image-${width}.png`), animations: 'disabled' })
    const samples = await banner.evaluate(async (root, encoded) => {
      const bytes = Uint8Array.from(atob(encoded), char => char.charCodeAt(0))
      const bitmap = await createImageBitmap(new Blob([bytes], { type: 'image/png' }))
      const canvas = document.createElement('canvas')
      canvas.width = bitmap.width; canvas.height = bitmap.height
      const context = canvas.getContext('2d')!
      context.drawImage(bitmap, 0, 0)
      const lead = root.querySelector('.is-active .banner-carousel__lead')!.getBoundingClientRect()
      const bounds = root.getBoundingClientRect()
      const pixel = (x: number, y: number) => [...context.getImageData(Math.floor(x), Math.floor(y), 1, 1).data].slice(0, 3)
      return { right: pixel(bounds.width * .9, bounds.height * .2), lead: pixel(lead.right - bounds.x - 1, lead.bottom - bounds.y - 1) }
    }, screenshot.toString('base64'))
    expect(1.05 / (luminance(samples.lead) + .05)).toBeGreaterThanOrEqual(4.5)
    if (width >= 900) expect(Math.min(...samples.right)).toBeGreaterThanOrEqual(170)
  }
}
