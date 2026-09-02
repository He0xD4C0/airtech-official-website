import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { adminApi } from '@/services/adminApi'
import type { Permission, SessionUser } from '@/types/domain'

export const useAuthStore = defineStore('auth', () => {
  const user = ref<SessionUser | null>(null)
  const initialized = ref(false)
  const loading = ref(false)

  const isAuthenticated = computed(() => Boolean(user.value))
  const isDevelopment = computed(() => user.value?.environment === 'development')
  const requiresTotpEnrollment = computed(() => Boolean(user.value && !user.value.totpEnabled))

  function hasPermission(permission?: Permission): boolean {
    if (!permission) return true
    return user.value?.permissions.includes(permission) ?? false
  }

  async function initialize(): Promise<void> {
    if (initialized.value) return
    loading.value = true
    try {
      user.value = await adminApi.session()
    } finally {
      initialized.value = true
      loading.value = false
    }
  }

  async function login(email: string, password: string, otp?: string): Promise<void> {
    loading.value = true
    try {
      user.value = await adminApi.login(email, password, otp)
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
    } finally {
      loading.value = false
    }
  }

  async function logout(): Promise<void> {
    await adminApi.logout()
    user.value = null
  }

  return {
    user,
    initialized,
    loading,
    isAuthenticated,
    isDevelopment,
    requiresTotpEnrollment,
    hasPermission,
    initialize,
    refresh,
    login,
    setup,
    logout,
  }
})
