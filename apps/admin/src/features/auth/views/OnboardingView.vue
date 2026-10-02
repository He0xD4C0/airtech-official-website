<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { Copy, KeyRound, LockKeyhole, ShieldCheck } from 'lucide-vue-next'
import BrandMark from '@/shared/components/BrandMark.vue'
import { adminAuthApi } from '@/shared/services/adminAuthApi'
import { useAuthStore } from '@/shared/stores/auth'
import type { ApiProblem } from '@/shared/types/domain'

const router = useRouter()
const auth = useAuthStore()

const currentPassword = ref('')
const newPassword = ref('')
const confirmPassword = ref('')
const recoveryKey = ref<string | null>(null)
const acknowledged = ref(false)
const busy = ref(false)
const errorMessage = ref('')

const needsPassword = computed(() => Boolean(auth.user?.mustChangePassword))
const needsRecoveryKey = computed(() => Boolean(auth.user?.mustConfirmRecoveryKey))

onMounted(async () => {
  if (!auth.isAuthenticated) {
    await router.replace({ name: 'login' })
    return
  }
  if (!auth.requiresOnboarding) await router.replace('/')
})

function message(error: unknown): string {
  return (error as ApiProblem).detail ?? (error as ApiProblem).title ?? '操作失败，请重试。'
}

async function submitPassword(): Promise<void> {
  errorMessage.value = ''
  if (newPassword.value !== confirmPassword.value) {
    errorMessage.value = '两次输入的新密码不一致。'
    return
  }
  busy.value = true
  try {
    await adminAuthApi.changePassword(currentPassword.value, newPassword.value)
    await auth.refresh()
    currentPassword.value = ''
    newPassword.value = ''
    confirmPassword.value = ''
    if (!auth.requiresOnboarding) await router.replace('/')
  } catch (error) {
    errorMessage.value = message(error)
  } finally {
    busy.value = false
  }
}

async function loadRecoveryKey(): Promise<void> {
  errorMessage.value = ''
  busy.value = true
  try {
    const state = await adminAuthApi.recoveryKey()
    recoveryKey.value = state.recoveryKey ?? ''
  } catch (error) {
    errorMessage.value = message(error)
  } finally {
    busy.value = false
  }
}

async function copyRecoveryKey(): Promise<void> {
  if (recoveryKey.value) await navigator.clipboard.writeText(recoveryKey.value)
}

async function confirmRecoveryKey(): Promise<void> {
  errorMessage.value = ''
  busy.value = true
  try {
    await adminAuthApi.confirmRecoveryKey()
    await auth.refresh()
    acknowledged.value = false
    if (!auth.requiresOnboarding) await router.replace('/')
  } catch (error) {
    errorMessage.value = message(error)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <main class="auth-layout">
    <section class="auth-brand-panel">
      <BrandMark vertical />
      <div class="auth-brand-panel__body">
        <p class="eyebrow eyebrow--light">AIRTEKPOWER CONTROL CENTER</p>
        <h1>首次登录安全设置</h1>
        <p>设置自己的密码，并把管理员恢复密钥保存到离线位置。</p>
      </div>
      <footer><ShieldCheck :size="16" /> 完成设置前管理接口保持锁定</footer>
    </section>

    <section class="auth-form-panel">
      <div class="auth-card">
        <div class="auth-card__mobile-brand"><BrandMark /></div>
        <p class="eyebrow">ONBOARDING</p>
        <h2>完成首次安全设置</h2>

        <form v-if="needsPassword" @submit.prevent="submitPassword">
          <label class="field">
            <span>当前初始密码</span>
            <div class="field__control"><LockKeyhole :size="17" /><input v-model="currentPassword" type="password" autocomplete="current-password" required /></div>
          </label>
          <label class="field">
            <span>新密码</span>
            <div class="field__control"><LockKeyhole :size="17" /><input v-model="newPassword" type="password" autocomplete="new-password" minlength="12" required /></div>
          </label>
          <label class="field">
            <span>确认新密码</span>
            <div class="field__control"><LockKeyhole :size="17" /><input v-model="confirmPassword" type="password" autocomplete="new-password" minlength="12" required /></div>
          </label>
          <button class="button button--primary button--wide" type="submit" :disabled="busy">保存新密码</button>
        </form>

        <section v-if="needsRecoveryKey" class="settings-section">
          <p><KeyRound :size="16" /> 管理员恢复密钥只在未确认前显示一次；丢失后只能用已保存的副本恢复账号。</p>
          <button v-if="recoveryKey === null" class="button button--secondary" type="button" :disabled="busy" @click="loadRecoveryKey">获取恢复密钥</button>
          <template v-else>
            <code v-if="recoveryKey" class="recovery-code-panel">{{ recoveryKey }}</code>
            <p v-else>恢复密钥由运维预先提供，请使用部署环境中保存的副本。</p>
            <div class="settings-section--actions">
              <button v-if="recoveryKey" class="button button--secondary" type="button" @click="copyRecoveryKey"><Copy :size="15" />复制</button>
              <label class="field field--compact"><input v-model="acknowledged" type="checkbox" /> 我已将恢复密钥保存到离线位置</label>
              <button class="button button--primary" type="button" :disabled="!acknowledged || busy" @click="confirmRecoveryKey">确认并继续</button>
            </div>
          </template>
        </section>

        <p v-if="errorMessage" class="form-error" role="alert">{{ errorMessage }}</p>
      </div>
    </section>
  </main>
</template>
