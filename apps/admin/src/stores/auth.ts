import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { adminApi, mockApiEnabled } from '@/services/adminApi'
import type { Permission, SessionUser } from '@/types/domain'

const DEMO_SESSION_KEY = 'airtek.admin.demo-session'

export const useAuthStore = defineStore('auth', () => {
  const user = ref<SessionUser | null>(null)
  const initialized = ref(false)
  const loading = ref(false)

  const isAuthenticated = computed(() => Boolean(user.value))
  const isDevelopment = computed(() => user.value?.environment === 'development')

  function hasPermission(permission?: Permission): boolean {
    if (!permission) return true
    return user.value?.permissions.includes(permission) ?? false
  }

  async function initialize(): Promise<void> {
    if (initialized.value) return
    loading.value = true
    try {
      if (mockApiEnabled && sessionStorage.getItem(DEMO_SESSION_KEY) === 'active') {
        user.value = await adminApi.login('demo@localhost.invalid', 'development-only')
      } else {
        user.value = await adminApi.session()
      }
    } finally {
      initialized.value = true
      loading.value = false
    }
  }

  async function login(email: string, password: string, otp?: string): Promise<void> {
    loading.value = true
    try {
      user.value = await adminApi.login(email, password, otp)
      if (mockApiEnabled) sessionStorage.setItem(DEMO_SESSION_KEY, 'active')
    } finally {
      loading.value = false
    }
  }

  async function refresh(): Promise<void> {
    user.value = await adminApi.session()
  }

  async function setup(displayName: string, email: string, password: string, bootstrapToken: string): Promise<void> {
    loading.value = true
    try {
      user.value = await adminApi.setup(displayName, email, password, bootstrapToken)
      if (mockApiEnabled) sessionStorage.setItem(DEMO_SESSION_KEY, 'active')
    } finally {
      loading.value = false
    }
  }

  async function logout(): Promise<void> {
    await adminApi.logout()
    user.value = null
    sessionStorage.removeItem(DEMO_SESSION_KEY)
  }

  return {
    user,
    initialized,
    loading,
    isAuthenticated,
    isDevelopment,
    hasPermission,
    initialize,
    refresh,
    login,
    setup,
    logout,
  }
})
