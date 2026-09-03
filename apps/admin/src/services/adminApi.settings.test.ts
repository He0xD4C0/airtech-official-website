import { describe, expect, it, vi } from 'vitest'
import { adminApi } from './adminApi'
import { response, setupAdminApiTestEnvironment } from './adminApi.testSupport'

setupAdminApiTestEnvironment()

describe('platform settings API', () => {
  it('reads the settings ETag and updates only the allow-listed policy fields', async () => {
    const current = {
      rfqRetentionDays: 365,
      retentionDeletionGraceDays: 30,
      temporaryOverrideDefaultDays: 30,
      publicLocale: 'en' as const,
      revision: 6,
    }
    const fetchMock = vi.fn(async (_input: string | URL | Request, init?: RequestInit) => {
      if (init?.method === 'PATCH') {
        return response({ ...current, rfqRetentionDays: 540, revision: 7 }, { ETag: '"revision-7"' })
      }
      return response(current, { ETag: '"revision-6"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.getSettings()).resolves.toEqual({ settings: current, etag: '"revision-6"' })
    await expect(adminApi.updateSettings({
      rfqRetentionDays: 540,
      reason: 'Align retention with approved policy',
    }, 6)).resolves.toMatchObject({
      settings: { rfqRetentionDays: 540, publicLocale: 'en', revision: 7 },
      etag: '"revision-7"',
    })

    const [url, init] = fetchMock.mock.calls[1] ?? []
    expect(String(url)).toContain('/settings')
    expect(init?.method).toBe('PATCH')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-6"')
    expect(JSON.parse(String(init?.body))).toEqual({
      rfqRetentionDays: 540,
      reason: 'Align retention with approved policy',
    })
  })
})
