import { adminIntegrationApi } from '@/features/integrations/services/adminIntegrationApi'
import { describe, expect, it, vi } from 'vitest'

import { response, setupAdminApiTestEnvironment } from '@/shared/services/adminApi.testSupport'

setupAdminApiTestEnvironment()

const settings = {
  connectorId: '63e3d923-632a-4e47-a31b-16f3ec81690e',
  appId: 'cli_airtek',
  secretConfigured: true,
  enabled: false,
  intervalEnabled: true,
  intervalMinutes: 15,
  dailyEnabled: true,
  dailyLocalTime: '02:00',
  timezone: 'Asia/Shanghai' as const,
  mappingVersion: 'feishu-product-v1' as const,
  sources: [{
    enabled: true,
    wikiToken: 'wiki', tableId: 'tblJjxOgBFL0FD0N', name: 'Axial Fans',
    family: 'axial' as const, application: null,
  }],
  revision: 3,
  connectionRevision: 2,
  testedConnectionRevision: 2,
  lastConnectionTestAt: '2026-09-18T00:00:00Z',
  lastIntervalAt: null,
  lastDailyAt: null,
  updatedAt: '2026-09-18T00:00:00Z',
  updatedBy: 'migration',
}

describe('Feishu integration API', () => {
  it('uses ETags and sends GUI-managed credentials, sources, and schedule settings', async () => {
    const fetchMock = vi.fn(async (_input: string | URL | Request, init?: RequestInit) => {
      if (init?.method === 'PUT') {
        return response({ ...settings, enabled: true, intervalMinutes: 30, revision: 4 }, {
          ETag: '"revision-4"',
        })
      }
      return response(settings, { ETag: '"revision-3"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminIntegrationApi.getFeishuSettings()).resolves.toEqual({
      settings,
      etag: '"revision-3"',
    })
    const update = {
      appId: 'cli_airtek',
      appSecret: 'rotated-secret',
      clearCredentials: false,
      sources: settings.sources,
      enabled: true,
      intervalEnabled: true,
      intervalMinutes: 30,
      dailyEnabled: true,
      dailyLocalTime: '02:00',
    }
    await expect(adminIntegrationApi.updateFeishuSettings(3, update)).resolves.toMatchObject({
      settings: { enabled: true, intervalMinutes: 30, revision: 4 },
      etag: '"revision-4"',
    })

    const [url, init] = fetchMock.mock.calls[1] ?? []
    expect(String(url)).toContain('/feishu/settings')
    expect(init?.method).toBe('PUT')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-3"')
    expect(JSON.parse(String(init?.body))).toEqual(update)
  })

  it('runs a connection test and starts a full sync without a request body', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      if (String(input).endsWith('/connection-test')) {
        return response({
          credentialsConfigured: true,
          tokenIssued: true,
          objectStorageReady: true,
          privateStagingReady: true,
          runnable: true,
          connectionRevision: 2,
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
        trigger: 'manual',
        settingsRevision: 3,
        sources: settings.sources,
        mappingVersion: 'feishu-product-v1',
        status: 'queued',
        resumeCursor: null,
        recordsSeen: 0,
        recordsValid: 0,
        recordsApplied: 0,
        recordsFailed: 0,
        recordsDeleted: 0,
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

    await expect(adminIntegrationApi.testFeishuConnection()).resolves.toMatchObject({ runnable: true })
    await expect(adminIntegrationApi.startFeishuSync()).resolves.toMatchObject({
      status: 'queued',
      trigger: 'manual',
    })

    const [url, init] = fetchMock.mock.calls[1] ?? []
    expect(String(url)).toContain('/feishu/sync-runs')
    expect(init?.body).toBeUndefined()
  })
})
