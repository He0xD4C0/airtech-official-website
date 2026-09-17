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

  it('keeps object-storage secrets write-only and sends revision preconditions', async () => {
    const current = {
      configured: true,
      provider: 's3',
      endpoint: 'https://s3.example.test',
      region: 'test-1',
      bucket: 'media',
      accessKeyId: 'access',
      secretConfigured: true,
      keyPrefix: 'media',
      pathStyle: true,
      publicBaseUrl: 'https://media.example.test',
      legacyAssetCount: 0,
      revision: 3,
      updatedAt: '2026-09-17T00:00:00Z',
      updatedBy: 'admin@example.test',
    }
    const fetchMock = vi.fn(async (_input: string | URL | Request, init?: RequestInit) => {
      if (init?.method === 'POST') return response({ ok: true, publicUrl: `${current.publicBaseUrl}/media/.airtek-probe/test.txt` })
      if (init?.method === 'PUT') return response({ ...current, revision: 4 }, { ETag: '"revision-4"' })
      return response(current, { ETag: '"revision-3"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    const loaded = await adminApi.getObjectStorageSettings()
    expect(loaded.settings).not.toHaveProperty('secretAccessKey')
    const input = {
      endpoint: current.endpoint,
      region: current.region,
      bucket: current.bucket,
      accessKeyId: current.accessKeyId,
      secretAccessKey: '',
      keyPrefix: current.keyPrefix,
      pathStyle: current.pathStyle,
      publicBaseUrl: current.publicBaseUrl,
    }
    await expect(adminApi.testObjectStorageSettings(input)).resolves.toMatchObject({ ok: true })
    await adminApi.updateObjectStorageSettings({
      ...input,
      adoptLegacyAssets: false,
      reason: 'Rotate the public delivery configuration.',
    }, 3)

    const [, updateInit] = fetchMock.mock.calls[2] ?? []
    expect(updateInit?.method).toBe('PUT')
    expect(new Headers(updateInit?.headers).get('If-Match')).toBe('"revision-3"')
    expect(JSON.parse(String(updateInit?.body)).secretAccessKey).toBe('')
  })
})
