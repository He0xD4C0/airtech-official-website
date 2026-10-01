import { expect, type Page } from '@playwright/test'

/** Sample actual painted slide positions, including both circular boundaries. */
export async function verifyBannerMotion(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1280, height: 1000 })
  await page.emulateMedia({ reducedMotion: 'no-preference' })
  await page.reload()
  const banner = page.locator('.banner-carousel')
  await banner.getByRole('button', { name: 'Pause banners' }).click()
  const settle = () => page.waitForFunction(() => !document.querySelector('.banner-carousel.is-moving, .banner-carousel.is-preparing'))
  await banner.getByRole('button', { name: 'Show banner 3' }).click()
  await settle()

  async function sample(button: string, direction: number, target: string): Promise<void> {
    await banner.getByRole('button', { name: button, exact: true }).click()
    const frames = await banner.evaluate(root => new Promise<Array<{ incoming: number; outgoing: number; width: number; duration: string; visible: number }>>(resolve => {
      const result: Array<{ incoming: number; outgoing: number; width: number; duration: string; visible: number }> = []
      function tick() {
        const current = root.querySelector('.is-active')!
        const old = root.querySelector('.is-outgoing')
        if (!old) { resolve(result); return }
        const bounds = root.getBoundingClientRect()
        result.push({ incoming: current.getBoundingClientRect().x - bounds.x,
          outgoing: old.getBoundingClientRect().x - bounds.x, width: bounds.width,
          duration: getComputedStyle(current).transitionDuration,
          visible: Array.from(root.querySelectorAll('.banner-carousel__slide')).filter(slide => getComputedStyle(slide).visibility === 'visible').length })
        if (result.length > 120) { resolve(result); return }
        requestAnimationFrame(tick)
      }
      requestAnimationFrame(tick)
    }))
    expect(frames.length).toBeGreaterThan(3)
    expect(frames.some(frame => frame.duration === '0.35s')).toBe(true)
    expect(frames.some(frame => frame.incoming * direction > 1 && frame.incoming * direction < frame.width - 1)).toBe(true)
    for (const frame of frames) {
      expect(frame.visible).toBe(2)
      expect(frame.incoming * direction).toBeGreaterThanOrEqual(-1)
      expect(frame.incoming * direction).toBeLessThanOrEqual(frame.width + 1)
      expect(frame.outgoing * direction).toBeGreaterThanOrEqual(-frame.width - 1)
      expect(frame.outgoing * direction).toBeLessThanOrEqual(1)
    }
    await settle()
    await expect(banner.locator('[aria-current=true]')).toHaveAttribute('aria-label', target)
    expect(await banner.evaluate(root => root.style.getPropertyValue('--banner-direction'))).toBe(String(direction))
  }
  await sample('Next banner', 1, 'Show banner 1')
  await sample('Previous banner', -1, 'Show banner 3')
  await sample('Show banner 1', -1, 'Show banner 1')
  for (let index = 0; index < 3; index += 1) await banner.getByRole('button', { name: 'Next banner' }).click()
  await settle()
  await expect(banner.locator('[aria-current=true]')).toHaveAttribute('aria-label', 'Show banner 1')
}
