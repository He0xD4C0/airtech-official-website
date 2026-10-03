import { adminAuthApi } from '@/shared/services/adminAuthApi'
import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

import type { Permission, SessionUser } from '@/shared/types/domain'

export const useAuthStore = defineStore('auth', () => {
  const user = ref<SessionUser | null>(null)
  const initialized = ref(false)
  const loading = ref(false)

  const isAuthenticated = computed(() => Boolean(user.value))
  const isDevelopment = computed(() => user.value?.environment === 'development')
  const isSuperAdmin = computed(() => Boolean(user.value?.roleKeys.includes('super-admin')))
  const requiresOnboarding = computed(() => Boolean(
    user.value && (user.value.mustChangePassword || user.value.mustConfirmRecoveryKey),
  ))

  function hasPermission(permission?: Permission): boolean {
    if (!permission) return true
    return user.value?.permissions.includes(permission) ?? false
  }

  async function initialize(): Promise<void> {
    if (initialized.value) return
    loading.value = true
    try {
      user.value = await adminAuthApi.session()
    } finally {
      initialized.value = true
      loading.value = false
    }
  }

  async function login(email: string, password: string, otp?: string): Promise<void> {
    loading.value = true
    try {
      user.value = await adminAuthApi.login(email, password, otp)
    } finally {
      loading.value = false
    }
  }

  function apply(next: SessionUser | null): void {
    user.value = next
  }

  async function refresh(): Promise<void> {
    user.value = await adminAuthApi.session()
  }

  async function logout(): Promise<void> {
    await adminAuthApi.logout()
    user.value = null
  }

  return {
    user,
    initialized,
    loading,
    isAuthenticated,
    isDevelopment,
    isSuperAdmin,
    requiresOnboarding,
    hasPermission,
    initialize,
    refresh,
    login,
    apply,
    logout,
  }
})
