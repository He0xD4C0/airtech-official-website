import { expect, request, test } from '@playwright/test'
import {
  absolute,
  administrator,
  apiControlHeaders,
  apiControlOrigin,
  adminOrigin,
  adminStorageStatePath,
  apiOrigin,
  browserCookiesForLocalGateway,
  restrictedUser,
  runAdminWorkflows,
} from './support/environment'
import { totp } from './support/totp'
import { secureHostOnlyCookies } from './support/secure-cookie'

test.describe('Authenticated Admin workflows against Rust and PostgreSQL', () => {
  test.skip(!runAdminWorkflows, 'Run through pnpm test:e2e:stack so mutations use a disposable PostgreSQL volume.')
  test.describe.configure({ mode: 'serial' })
  test.use({ storageState: adminStorageStatePath })

  test('creates and publishes News through the unified content editor', async ({ page }) => {
    await page.goto(absolute(adminOrigin, '/content/drafts/new'))
    const templateCards = page.locator('.create-form__cards')
    await templateCards.getByRole('button', { name: /^新闻 /u }).click()
    await templateCards.getByRole('button').filter({ hasText: 'newsDetail' }).click()
    await page.getByLabel('标题', { exact: true }).fill('AIRTEK E2E publication')
    await page.getByLabel(/Slug/u).fill('airtek-e2e-publication')
    await page.getByRole('button', { name: '创建草稿' }).click()

    await expect(page.getByRole('heading', { name: '基本信息' })).toBeVisible()
    await expect(
      page.getByRole('button', { name: new RegExp(administrator.displayName, 'u') }),
    ).toBeVisible({ timeout: 15_000 })
    await page.getByLabel('分类').fill('Acceptance')
    await page.getByLabel('作者显示名').fill('AIRTEKPOWER E2E')

    const body = page.locator('[contenteditable="true"]').first()
    await body.click()
    await body.fill('Disposable browser acceptance record body.')

    const placeholder = page.getByRole('checkbox', { name: /^占位内容/u })
    if (await placeholder.isChecked()) await placeholder.uncheck()
    await expect(page.locator('.save-state')).toContainText('有未保存更改')
    await page.getByRole('button', { name: '保存', exact: true }).click()
    await expect(page.locator('.save-state')).toContainText('已保存')
    await page.getByRole('button', { name: '提交审核' }).click()
    await expect(page.getByText('已发布', { exact: true })).toBeVisible()
  })

  test('updates, explicitly saves and publishes General Information', async ({ page }) => {
    await page.goto(absolute(adminOrigin, '/site/general-information'))
    const createDraft = page.getByRole('button', { name: '创建编辑草稿' })
    const editorHeading = page.getByRole('heading', { name: '基本信息', exact: true })
    await expect(createDraft.or(editorHeading)).toBeVisible()
    if (await createDraft.isVisible()) await createDraft.click()

    await expect(editorHeading).toBeVisible()
    await page.getByLabel('品牌标语').fill('AIRTEK E2E database-backed identity')
    await expect(page.locator('.save-state')).toContainText('有未保存更改')
    await page.getByRole('button', { name: '保存', exact: true }).click()
    await expect(page.locator('.save-state')).toContainText('已保存')
    await page.getByRole('button', { name: '提交审核' }).click()
    await expect(page.getByText('已发布', { exact: true })).toBeVisible()
  })

  test('imports a Product Master row and edits portal-owned product fields', async ({ page }) => {
    test.setTimeout(180_000)
    const csv = process.env.E2E_PRODUCT_MASTER_CSV_BASE64
      ? Buffer.from(process.env.E2E_PRODUCT_MASTER_CSV_BASE64, 'base64').toString('utf8')
      : [
          'stable_id,model,family,title,样品报价（sample）',
          'E2E-PRODUCT-001,E2E-MODEL-001,Axial,E2E imported product,100.00',
          '',
        ].join('\n')
    await page.goto(absolute(adminOrigin, '/products/imports'))
    await page.locator('input[type="file"]').setInputFiles({
      name: 'airtek-e2e-product-master.csv',
      mimeType: 'text/csv',
      buffer: Buffer.from(csv),
    })
    await page.getByRole('button', { name: '开始导入' }).click()
    await expect(page.getByText('Product Master 导入完成')).toBeVisible({ timeout: 120_000 })
    await expect(page.getByText(/1 条有效记录/u)).toBeVisible()

    await page.goto(absolute(adminOrigin, '/products'))
    const productSearch = page.waitForResponse((response) => {
      const url = new URL(response.url())
      return url.pathname === '/api/admin/v1/products'
        && url.searchParams.get('q') === 'E2E-MODEL-001'
    })
    await page.getByPlaceholder('搜索稳定 ID、型号或家族').fill('E2E-MODEL-001')
    expect((await productSearch).status()).toBe(200)
    await page.getByRole('link', { name: '查看产品 E2E-MODEL-001' }).click()
    await expect(page.getByRole('heading', { name: '网站展示与 SEO' })).toBeVisible()
    await page.getByLabel('公开标题').fill('E2E portal product title')
    await page.getByLabel('SEO Title').fill('E2E product SEO title')
    await page.getByLabel('排序值').fill('73')
    await page.getByRole('button', { name: '保存网站字段' }).click()
    await expect(page.getByText('网站展示字段已保存')).toBeVisible()

    await page.getByRole('button', { name: '按需查看私有报价' }).click()
    await expect(page.getByText('100.00', { exact: true })).toBeVisible()
  })

  test('renders PostgreSQL source aggregates without visitor profiles', async ({ page }) => {
    await page.goto(absolute(adminOrigin, '/analytics/sources'))
    await expect(page.locator('.source-dashboard')).toContainText('e2e.example.test')
    await expect(page.getByText(/e2e-source \/ integration-test/u)).toBeVisible()
  })

  test('updates role authorization, creates an invitation, and enforces UI and API denial', async ({ page, browser }) => {
    test.setTimeout(120_000)
    await page.goto(absolute(adminOrigin, '/roles'))
    await page.getByRole('link').filter({ hasText: 'content-editor' }).click()
    await page.getByLabel('角色名称').fill('Content Editor E2E')
    const permissions = page.getByRole('group', { name: '权限矩阵' })
    for (const permission of ['content.write', 'media.write']) {
      const checkbox = permissions.locator('label').filter({ hasText: permission }).getByRole('checkbox')
      if (await checkbox.isChecked()) await checkbox.uncheck()
    }
    await page.getByLabel('变更原因').fill('E2E restricts this role for authorization acceptance')
    await page.getByRole('button', { name: '保存角色' }).click()
    await expect(page.getByText('角色已更新')).toBeVisible()

    await page.goto(absolute(adminOrigin, '/users'))
    await page.getByRole('button', { name: '邀请用户' }).click()
    await page.getByLabel('显示名称').fill(restrictedUser.displayName)
    await page.getByLabel('邮箱').fill(restrictedUser.email)
    await page.locator('.modal-card .role-options label').filter({ hasText: 'Content Editor E2E' }).getByRole('checkbox').check()
    await page.getByRole('button', { name: '发送邀请' }).click()
    await expect(page.getByText('邀请已创建')).toBeVisible()
    const token = (await page.locator('section').filter({ hasText: '一次性邀请令牌' }).locator('code.checksum').textContent())?.trim()
    expect(token).toMatch(/^[A-Za-z0-9_-]{43}$/u)

    const activationApi = await request.newContext({
      baseURL: apiControlOrigin,
      extraHTTPHeaders: apiControlHeaders(),
    })
    try {
      const accepted = await activationApi.post('/api/admin/v1/auth/invitations/accept', {
        data: { token, password: restrictedUser.password },
      })
      expect(accepted.status()).toBe(201)
    } finally {
      await activationApi.dispose()
    }

    const restrictedContext = await browser.newContext()
    try {
      const restrictedAuthApi = await request.newContext({
        baseURL: apiControlOrigin,
        extraHTTPHeaders: apiControlHeaders(),
      })
      const loginResponse = await restrictedAuthApi.post('/api/admin/v1/auth/login', {
        data: { email: restrictedUser.email, password: restrictedUser.password },
      })
      expect(loginResponse.status()).toBe(200)
      const restrictedCsrf = loginResponse.headers()['x-csrf-token']
      expect(restrictedCsrf).toBeTruthy()
      const restrictedCookies = secureHostOnlyCookies(loginResponse, new URL(apiOrigin).hostname)
      await restrictedContext.addCookies(browserCookiesForLocalGateway(restrictedCookies))
      await restrictedAuthApi.dispose()

      const restrictedPage = await restrictedContext.newPage()
      await restrictedPage.goto(absolute(adminOrigin, '/account/security'))
      await expect(restrictedPage).toHaveURL(absolute(adminOrigin, '/account/security'))

      // App startup refreshes the session and rotates CSRF. Read the latest
      // response header rather than reusing the pre-navigation login token.
      const refreshedCsrf = await restrictedPage.evaluate(async (apiUrl) => {
        const response = await fetch(`${apiUrl}/api/admin/v1/auth/session`, { credentials: 'include' })
        return { status: response.status, csrf: response.headers.get('x-csrf-token') }
      }, apiOrigin)
      expect(refreshedCsrf.status).toBe(200)
      expect(refreshedCsrf.csrf).toBeTruthy()

      const enrollment = await restrictedPage.evaluate(async ({ apiUrl, csrf }) => {
        const response = await fetch(`${apiUrl}/api/admin/v1/auth/totp/enrollment`, {
          method: 'POST', credentials: 'include', headers: { 'X-CSRF-Token': csrf },
        })
        return { status: response.status, body: await response.json() as { secret: string } }
      }, { apiUrl: apiOrigin, csrf: refreshedCsrf.csrf! })
      expect(enrollment.status).toBe(200)
      const confirmation = await restrictedPage.evaluate(async ({ apiUrl, csrf, code }) => {
        const response = await fetch(`${apiUrl}/api/admin/v1/auth/totp/confirm`, {
          method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf },
          body: JSON.stringify({ code }),
        })
        return response.status
      }, { apiUrl: apiOrigin, csrf: refreshedCsrf.csrf!, code: totp(enrollment.body.secret) })
      expect(confirmation).toBe(200)
      const session = await restrictedPage.evaluate(async (apiUrl) => {
        const response = await fetch(`${apiUrl}/api/admin/v1/auth/session`, { credentials: 'include' })
        return { status: response.status, body: await response.json() as { permissions: string[]; totpEnabled: boolean } }
      }, apiOrigin)
      expect(session.status).toBe(200)
      const restrictedSession = session.body
      expect(restrictedSession.totpEnabled).toBe(true)
      expect(restrictedSession.permissions).toContain('content.read')
      expect(restrictedSession.permissions).not.toContain('analytics.read')

      await restrictedPage.goto(absolute(adminOrigin, '/analytics/sources'))
      await expect(restrictedPage).toHaveURL(/\/forbidden\?/u)
      await expect(restrictedPage.getByRole('heading', { name: '没有访问权限' })).toBeVisible()
      await expect(restrictedPage.getByText('analytics.read', { exact: true })).toBeVisible()

      const denied = await restrictedPage.evaluate(async (apiUrl) => {
        const response = await fetch(`${apiUrl}/api/admin/v1/analytics/sources`, { credentials: 'include' })
        return response.status
      }, apiOrigin)
      expect(denied).toBe(403)

      await page.goto(absolute(adminOrigin, '/users'))
      await page.getByRole('link').filter({ hasText: restrictedUser.email }).click()
      await page.getByLabel('显示名称', { exact: true }).fill(`${restrictedUser.displayName} Updated`)
      await page.getByLabel('变更原因').fill('E2E verifies user revision ETag updates')
      await page.getByRole('button', { name: '保存', exact: true }).click()
      await expect(page.getByText('用户已更新')).toBeVisible()
      await page.getByRole('button', { name: '撤销会话' }).click()
      await expect(page.getByText('会话已撤销')).toBeVisible()

      const revoked = await restrictedPage.evaluate(async (apiUrl) => {
        const response = await fetch(`${apiUrl}/api/admin/v1/auth/session`, { credentials: 'include' })
        return response.status
      }, apiOrigin)
      expect(revoked).toBe(401)
    } finally {
      await restrictedContext.close()
    }
  })
})
