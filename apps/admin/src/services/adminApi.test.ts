import { describe, expect, it, vi } from 'vitest'
import { adminApi } from './adminApi'
import { response, setupAdminApiTestEnvironment } from './adminApi.testSupport'

setupAdminApiTestEnvironment()

describe('contract transport compatibility', () => {
  it('filters development-only and unknown permissions when the virtual boundary is disabled', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => response({
      id: '92000000-0000-4000-8000-000000000001',
      displayName: 'Production Admin',
      email: 'admin@example.test',
      role: 'Developer',
      permissions: ['dashboard.read', 'devtools.shell', 'future.permission'],
      environment: 'production',
      totpEnabled: true,
    })))

    await expect(adminApi.session()).resolves.toMatchObject({
      permissions: ['dashboard.read'],
      environment: 'production',
    })
  })

  it('rotates CSRF from a safe response and sends it only with the following mutation', async () => {
    const session = {
      id: '93000000-0000-4000-8000-000000000001',
      displayName: 'Contract Admin',
      email: 'admin@example.test',
      role: 'Super Admin',
      permissions: ['settings.manage'],
      environment: 'development',
      totpEnabled: true,
    }
    const settings = {
      rfqRetentionDays: 365,
      retentionDeletionGraceDays: 30,
      temporaryOverrideDefaultDays: 30,
      publicLocale: 'en',
      revision: 2,
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void init
      const url = String(input)
      if (url.endsWith('/auth/logout')) return new Response(null, { status: 204 })
      if (url.endsWith('/auth/session')) {
        return response(session, { 'X-CSRF-Token': 'rotated-contract-token' })
      }
      return response(settings, { ETag: '"revision-2"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.logout()
    fetchMock.mockClear()
    await adminApi.session()
    await adminApi.updateSettings({ rfqRetentionDays: 365, reason: 'Verify CSRF transport' }, 1)

    const [, sessionInit] = fetchMock.mock.calls[0] ?? []
    const [, mutationInit] = fetchMock.mock.calls[1] ?? []
    expect(new Headers(sessionInit?.headers).has('X-CSRF-Token')).toBe(false)
    expect(new Headers(mutationInit?.headers).get('X-CSRF-Token')).toBe('rotated-contract-token')
    await adminApi.logout()
  })

  it('preserves direct Problem Details fields for existing Admin error consumers', async () => {
    const problem = {
      type: 'https://airtek.example/problems/validation',
      title: 'Validation failed',
      status: 422,
      detail: 'Credentials were rejected by the contract endpoint.',
      requestId: '94000000-0000-4000-8000-000000000001',
    }
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify(problem), {
      status: 422,
      headers: { 'Content-Type': 'application/problem+json' },
    })))

    await expect(adminApi.login('admin@example.test', 'invalid-password')).rejects.toMatchObject({
      status: 422,
      detail: problem.detail,
      problem,
    })
  })
})
