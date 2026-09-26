// @vitest-environment jsdom

import { createApp, nextTick } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  getSettings: vi.fn(),
  connectionStatus: vi.fn(),
  listRuns: vi.fn(),
  toast: vi.fn(),
}))

vi.mock('@/features/integrations/services/adminIntegrationApi', () => ({ adminIntegrationApi: {
    getFeishuSettings: mocks.getSettings,
    connectionStatus: mocks.connectionStatus,
    listSyncRuns: mocks.listRuns,
    updateFeishuSettings: vi.fn(),
    testFeishuConnection: vi.fn(),
    startFeishuSync: vi.fn(),
    getFeishuSyncRun: vi.fn(),
  } }))

vi.mock('@/shared/stores/auth', () => ({
  useAuthStore: () => ({ hasPermission: () => true }),
}))

vi.mock('@/shared/stores/ui', () => ({
  useUiStore: () => ({ toast: mocks.toast }),
}))

import FeishuSyncView from '@/features/integrations/views/FeishuSyncView.vue'

const settings = {
  connectorId: '63e3d923-632a-4e47-a31b-16f3ec81690e',
  appId: 'cli_airtek',
  secretConfigured: true,
  enabled: false,
  intervalEnabled: true,
  intervalMinutes: 15,
  dailyEnabled: false,
  dailyLocalTime: '02:00',
  timezone: 'Asia/Shanghai' as const,
  mappingVersion: 'feishu-product-v1',
  sources: [{
    enabled: true,
    wikiToken: 'wiki',
    tableId: 'table',
    name: 'Axial Fans',
    family: 'axial' as const,
    application: null,
  }],
  revision: 1,
  connectionRevision: 1,
  testedConnectionRevision: 1,
  lastConnectionTestAt: '2026-09-19T00:00:00Z',
  lastIntervalAt: null,
  lastDailyAt: null,
  updatedAt: '2026-09-19T00:00:00Z',
  updatedBy: 'migration',
}

beforeEach(() => {
  mocks.getSettings.mockReset().mockResolvedValue({ settings, etag: '"revision-1"' })
  mocks.connectionStatus.mockReset().mockResolvedValue({
    connectorId: settings.connectorId,
    displayName: 'AIRTEKPOWER Product Master',
    configured: true,
    enabled: false,
    runnable: false,
    unavailableReason: 'Automatic synchronization is disabled.',
    updatedAt: settings.updatedAt,
    latestSync: null,
  })
  mocks.listRuns.mockReset().mockResolvedValue({ items: [], nextCursor: null })
  mocks.toast.mockClear()
})

describe('FeishuSyncView', () => {
  it('shows write-only credentials, selectable sources, and independent full-scan schedules', async () => {
    const host = document.createElement('div')
    document.body.append(host)
    const app = createApp(FeishuSyncView)
    app.mount(host)
    await Promise.resolve()
    await Promise.resolve()
    await nextTick()

    expect(host.textContent).toContain('选择性同步来源表')
    expect(host.textContent).toContain('启用间隔同步')
    expect(host.textContent).toContain('启用每日同步')
    expect(host.textContent).toContain('立即完整同步')
    expect(host.textContent).toContain('App Secret 已配置')
    expect(host.textContent).not.toContain('增量同步')
    expect(host.textContent).not.toContain('回滚')
    expect(host.textContent).not.toContain('导出按钮')
    const password = host.querySelector<HTMLInputElement>('input[type="password"]')
    expect(password?.value).toBe('')
    expect(host.querySelectorAll('.source-card')).toHaveLength(1)

    const add = [...host.querySelectorAll<HTMLButtonElement>('button')]
      .find(button => button.textContent?.includes('添加来源'))
    add?.click()
    await nextTick()
    expect(host.querySelectorAll('.source-card')).toHaveLength(2)

    app.unmount()
    host.remove()
  })
})
