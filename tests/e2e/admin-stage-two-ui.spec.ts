import AxeBuilder from '@axe-core/playwright'
import { expect, test, type Page } from '@playwright/test'
import {
  absolute,
  adminOrigin,
  adminStorageStatePath,
  runAdminWorkflows,
} from './support/environment'

const routes = [
  '/',
  '/content/drafts',
  '/content/drafts/new',
  '/content/reviews',
  '/content/published',
  '/site/general-information',
  '/site/navigation',
  '/site/footer',
  '/products',
  '/products/imports',
  '/integrations/feishu',
  '/media',
  '/rfqs',
  '/contacts',
  '/analytics',
  '/analytics/sources',
  '/users',
  '/roles',
  '/audit',
  '/settings/general',
  '/settings/security',
  '/settings/consent',
  '/settings/retention',
  '/settings/domains',
] as const

function seriousViolations(
  violations: Awaited<ReturnType<AxeBuilder['analyze']>>['violations'],
) {
  return violations.filter(({ impact }) => impact === 'serious' || impact === 'critical')
}

async function settle(page: Page, path: string): Promise<void> {
  await page.goto(absolute(adminOrigin, path))
  await expect(page.locator('#main-content')).toBeVisible()
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.waitForTimeout(250)
}

test.describe('stage two Admin workspace acceptance', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack against the disposable full stack.')
  test.describe.configure({ mode: 'serial' })
  test.use({ storageState: adminStorageStatePath })

  test('covers every main navigation route without shell controls or serious axe violations', async ({ page }) => {
    test.setTimeout(240_000)
    const browserErrors: string[] = []
    page.on('console', (message) => {
      if (message.type() === 'error') browserErrors.push(message.text())
    })
    page.on('pageerror', (error) => browserErrors.push(error.message))

    for (const path of routes) {
      await settle(page, path)
      await expect(page.locator('body')).not.toContainText('切换工作空间')
      await expect(page.getByRole('button', { name: '通知' })).toHaveCount(0)
      await expect(page.getByRole('button', { name: '导出校验报告' })).toHaveCount(0)
      const accessibility = await new AxeBuilder({ page }).analyze()
      expect(seriousViolations(accessibility.violations), `axe violations at ${path}`).toEqual([])
    }
    expect(browserErrors).toEqual([])
  })

  test('keeps every main route inside 390, 768 and 1280 pixel viewports', async ({ page }) => {
    test.setTimeout(240_000)
    for (const width of [390, 768, 1280]) {
      await page.setViewportSize({ width, height: 900 })
      for (const path of routes) {
        await settle(page, path)
        const layout = await page.evaluate(() => ({
          viewport: window.innerWidth,
          documentWidth: document.documentElement.scrollWidth,
          mainWidth: document.querySelector<HTMLElement>('#main-content')?.getBoundingClientRect().width ?? 0,
          overflowers: Array.from(document.querySelectorAll<HTMLElement>('body *'))
            .map((element) => {
              const bounds = element.getBoundingClientRect()
              return {
                element: `${element.tagName.toLowerCase()}.${element.className}`,
                left: Math.round(bounds.left),
                right: Math.round(bounds.right),
                width: Math.round(bounds.width),
                scrollWidth: element.scrollWidth,
              }
            })
            .filter(({ right }) => right > window.innerWidth + 1)
            .slice(0, 8),
        }))
        expect(layout.mainWidth, `main width at ${width}px on ${path}`).toBeGreaterThan(0)
        expect(
          layout.documentWidth,
          `page overflow at ${width}px on ${path}: ${JSON.stringify(layout.overflowers)}`,
        ).toBeLessThanOrEqual(layout.viewport + 1)
      }
    }
  })

  test('moves focus into and back out of quick navigation', async ({ page }) => {
    await settle(page, '/')
    const trigger = page.getByRole('button', { name: /快速导航/u })
    await trigger.focus()
    await page.keyboard.press(process.platform === 'darwin' ? 'Meta+K' : 'Control+K')
    const filter = page.getByRole('textbox', { name: '筛选快速导航' })
    await expect(filter).toBeFocused()
    await filter.fill('新建')
    const quickNavigation = page.getByRole('dialog', { name: '快速导航' })
    await expect(quickNavigation.getByRole('link', { name: /新建私人草稿/u })).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(trigger).toBeFocused()
  })

  test('resets reused view state when navigating between named routes', async ({ page }) => {
    await settle(page, '/site/navigation')
    const initializeNavigation = page.getByRole('button', { name: /初始化 Navigation/u })
    if (await initializeNavigation.isVisible().catch(() => false)) await initializeNavigation.click()
    await expect(page.getByText('内容类型锁定为 导航', { exact: false })).toBeVisible()

    await page.getByRole('link', { name: 'Footer', exact: true }).click()
    await expect(page).toHaveURL(absolute(adminOrigin, '/site/footer'))
    await expect(page.locator('#main-content')).not.toContainText('navigation · navigation')
    await expect(page.locator('#main-content')).toContainText(/尚未初始化 Footer|内容类型锁定为 页脚/u)

    await page.getByRole('link', { name: '系统设置', exact: true }).click()
    await expect(page.getByRole('heading', { name: '系统设置', exact: true })).toBeVisible()
    await page.getByRole('link', { name: '安全会话', exact: true }).click()
    await expect(page).toHaveURL(absolute(adminOrigin, '/account/security'))
    await expect(page.getByRole('heading', { name: '账号安全', exact: true })).toBeVisible()
  })
})
