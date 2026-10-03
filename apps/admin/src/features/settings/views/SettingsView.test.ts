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
    user: { id: '90000000-0000-4000-8000-000000000001', totpEnabled: false, roleKeys: [] },
    requiresOnboarding: true,
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
  it('explains the first-login security onboarding gate', async () => {
    const host = document.createElement('div')
    document.body.append(host)
    const app = createApp(SettingsView)
    app.mount(host)
    await Promise.resolve()
    await nextTick()

    expect(host.textContent).toContain('先完成首次安全设置')
    expect(host.textContent).toContain('管理员恢复密钥确认完成后即可进入管理功能')

    app.unmount()
    host.remove()
  })
})
