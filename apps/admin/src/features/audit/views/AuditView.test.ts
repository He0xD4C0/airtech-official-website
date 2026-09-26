// @vitest-environment jsdom

import { createApp, nextTick } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  listAudit: vi.fn(),
  replace: vi.fn(),
  toast: vi.fn(),
}))

vi.mock('vue-router', () => ({
  useRoute: () => ({ query: {} }),
  useRouter: () => ({ replace: mocks.replace }),
}))

vi.mock('@/features/audit/services/auditApi', () => ({ auditApi: {
    listAudit: mocks.listAudit,
    exportAudit: vi.fn(),
  } }))

vi.mock('@/shared/stores/ui', () => ({
  useUiStore: () => ({ toast: mocks.toast }),
}))

import AuditView from '@/features/audit/views/AuditView.vue'

beforeEach(() => {
  mocks.replace.mockClear()
  mocks.toast.mockClear()
  mocks.listAudit.mockReset().mockResolvedValue({
    items: [{
      id: '10000000-0000-4000-8000-000000000001',
      actor: 'admin@airtek.invalid',
      action: 'settings.update',
      entityType: 'settings',
      entityId: null,
      before: { retentionDays: 180 },
      after: { retentionDays: 365 },
      reason: 'Extend retention for support cases',
      currentVersion: 2,
      requestId: '20000000-0000-4000-8000-000000000001',
      occurredAt: '2026-09-17T05:00:00Z',
    }],
    nextCursor: null,
    total: 1,
  })
})

describe('AuditView', () => {
  it('names the full-text search and can clear active filters', async () => {
    const host = document.createElement('div')
    document.body.append(host)
    const app = createApp(AuditView)
    app.mount(host)
    await nextTick()

    const search = host.querySelector<HTMLInputElement>('input[aria-label="全文搜索"]')
    const clear = [...host.querySelectorAll<HTMLButtonElement>('button')]
      .find((button) => button.textContent?.includes('清空筛选'))
    expect(search).not.toBeNull()
    expect(clear?.disabled).toBe(true)

    if (search) {
      search.value = 'auth.login'
      search.dispatchEvent(new Event('input'))
    }
    await nextTick()
    expect(clear?.disabled).toBe(false)
    clear?.click()
    await nextTick()
    expect(search?.value).toBe('')
    expect(clear?.disabled).toBe(true)

    app.unmount()
    host.remove()
  })

  it('reveals the before and after payload for an audit event', async () => {
    const host = document.createElement('div')
    document.body.append(host)
    const app = createApp(AuditView)
    app.mount(host)
    for (let index = 0; index < 5; index += 1) {
      await Promise.resolve()
      await nextTick()
    }

    const details = [...host.querySelectorAll('button')]
      .find((button) => button.textContent?.trim() === '查看详情')
    expect(details).toBeDefined()
    details?.click()
    await nextTick()

    const text = host.textContent ?? ''
    expect(text).toContain('变更前')
    expect(text).toContain('"retentionDays": 180')
    expect(text).toContain('变更后')
    expect(text).toContain('"retentionDays": 365')
    expect(text).toContain('Revision 2')
    app.unmount()
    host.remove()
  })
})
