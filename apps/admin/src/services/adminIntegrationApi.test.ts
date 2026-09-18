import { describe, expect, it, vi } from 'vitest'
import { adminApi } from './adminApi'
import { response, setupAdminApiTestEnvironment } from './adminApi.testSupport'

setupAdminApiTestEnvironment()

const settings = {
  connectorId: '63e3d923-632a-4e47-a31b-16f3ec81690e',
  enabled: false,
  intervalMinutes: 15,
  fullReconcileEnabled: true,
  fullReconcileLocalTime: '02:00',
  timezone: 'Asia/Shanghai' as const,
  mappingVersion: 'feishu-product-v1' as const,
  sources: [{
    wikiToken: 'wiki', tableId: 'tblJjxOgBFL0FD0N', name: 'Axial Fans',
    family: 'axial' as const, application: null,
  }],
  revision: 3,
  lastIncrementalAt: null,
  lastFullAt: null,
  updatedAt: '2026-09-18T00:00:00Z',
  updatedBy: 'migration',
}

describe('Feishu integration API', () => {
  it('uses ETags and sends only editable schedule settings', async () => {
    const fetchMock = vi.fn(async (_input: string | URL | Request, init?: RequestInit) => {
      if (init?.method === 'PUT') {
        return response({ ...settings, enabled: true, intervalMinutes: 30, revision: 4 }, {
          ETag: '"revision-4"',
        })
      }
      return response(settings, { ETag: '"revision-3"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.getFeishuSettings()).resolves.toEqual({
      settings,
      etag: '"revision-3"',
    })
    const update = {
      enabled: true,
      intervalMinutes: 30,
      fullReconcileEnabled: true,
      fullReconcileLocalTime: '02:00',
    }
    await expect(adminApi.updateFeishuSettings(3, update)).resolves.toMatchObject({
      settings: { enabled: true, intervalMinutes: 30, revision: 4 },
      etag: '"revision-4"',
    })

    const [url, init] = fetchMock.mock.calls[1] ?? []
    expect(String(url)).toContain('/feishu/settings')
    expect(init?.method).toBe('PUT')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-3"')
    expect(JSON.parse(String(init?.body))).toEqual(update)
  })

  it('runs a connection test and submits only the selected sync kind', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      if (String(input).endsWith('/connection-test')) {
        return response({
          credentialsConfigured: true,
          tokenIssued: true,
          objectStorageReady: true,
          runnable: true,
          tables: [],
          checkedAt: '2026-09-18T00:00:00Z',
        })
      }
      expect(init?.method).toBe('POST')
      return response({
        id: '70000000-0000-4000-8000-000000000001',
        connectorId: settings.connectorId,
        source: 'feishu',
        dryRun: false,
        runKind: 'full',
        mappingVersion: 'feishu-product-v1',
        status: 'queued',
        resumeCursor: null,
        recordsSeen: 0,
        recordsValid: 0,
        conflictCount: 0,
        recordsApplied: 0,
        recordsFailed: 0,
        assetsSeen: 0,
        assetsCopied: 0,
        assetsReused: 0,
        assetsFailed: 0,
        startedAt: '2026-09-18T00:00:00Z',
        completedAt: null,
        error: null,
      })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.testFeishuConnection()).resolves.toMatchObject({ runnable: true })
    await expect(adminApi.startFeishuSync({ runKind: 'full' })).resolves.toMatchObject({
      status: 'queued',
      runKind: 'full',
    })

    const [url, init] = fetchMock.mock.calls[1] ?? []
    expect(String(url)).toContain('/feishu/sync-runs')
    expect(JSON.parse(String(init?.body))).toEqual({ runKind: 'full' })
  })
})
