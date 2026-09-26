// @vitest-environment jsdom

import { createApp, nextTick } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  listSessions: vi.fn(),
  toast: vi.fn(),
}))

vi.mock('vue-router', () => ({
  useRoute: () => ({ name: 'account-security', params: {} }),
  useRouter: () => ({ replace: vi.fn() }),
}))

vi.mock('@/shared/services/adminAuthApi', () => ({ adminAuthApi: {
    listSessions: mocks.listSessions,
  } }))

vi.mock('@/shared/stores/auth', () => ({
  useAuthStore: () => ({
    user: { totpEnabled: false },
    requiresTotpEnrollment: true,
  }),
}))

vi.mock('@/shared/stores/ui', () => ({
  useUiStore: () => ({ toast: mocks.toast }),
}))

import SettingsView from '@/features/settings/views/SettingsView.vue'

beforeEach(() => {
  mocks.listSessions.mockReset().mockResolvedValue([])
  mocks.toast.mockClear()
})

describe('SettingsView account security', () => {
  it('explains why business permissions are withheld before TOTP enrollment', async () => {
    const host = document.createElement('div')
    document.body.append(host)
    const app = createApp(SettingsView)
    app.mount(host)
    await Promise.resolve()
    await nextTick()

    expect(host.textContent).toContain('完成 TOTP 后解锁管理功能')
    expect(host.textContent).toContain('服务端在完成绑定前暂不下发业务权限')

    app.unmount()
    host.remove()
  })
})
