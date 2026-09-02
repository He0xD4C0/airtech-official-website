<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowRight, Check, KeyRound, LockKeyhole, ShieldCheck } from 'lucide-vue-next'
import BrandMark from '@/components/BrandMark.vue'
import { adminApi, type InvitationAcceptance } from '@/services/adminApi'
import { apiErrorMessage, apiProblemStatus } from '@/services/cursorPagination'
import { readAndClearInvitationToken } from '@/services/invitationToken'

const route = useRoute()
const router = useRouter()
const token = ref('')
const password = ref('')
const confirmation = ref('')
const submitting = ref(false)
const result = ref<InvitationAcceptance>()
const errorMessage = ref('')
const tokenValid = computed(() => /^[A-Za-z0-9_-]{43}$/.test(token.value))
const passwordChecks = computed(() => [
  { label: '12–256 个字符', pass: password.value.length >= 12 && password.value.length <= 256 },
  { label: '包含 ASCII 字母和数字', pass: /[A-Za-z]/.test(password.value) && /\d/.test(password.value) },
  { label: '两次输入一致', pass: password.value.length > 0 && password.value === confirmation.value },
])

async function submit(): Promise<void> {
  errorMessage.value = ''
  if (!tokenValid.value) {
    errorMessage.value = '邀请链接无效或缺少一次性令牌，请联系管理员重新邀请。'
    return
  }
  if (passwordChecks.value.some((check) => !check.pass)) {
    errorMessage.value = '请完成全部密码安全要求。'
    return
  }
  submitting.value = true
  try {
    result.value = await adminApi.acceptInvitation({ token: token.value, password: password.value })
    password.value = ''
    confirmation.value = ''
    token.value = ''
  } catch (error) {
    const status = apiProblemStatus(error)
    errorMessage.value = status === 401
      ? '邀请已失效、过期、撤销或使用过，请联系管理员重新邀请。'
      : status === 409
        ? '该邮箱已存在账号，请直接登录或联系管理员。'
        : status === 429
          ? '尝试次数过多，请稍后再试。'
          : apiErrorMessage(error, '激活失败，请稍后重试。')
  } finally {
    submitting.value = false
  }
}

onMounted(async () => {
  token.value = await readAndClearInvitationToken(
    route.query.token,
    () => router.replace({ name: 'accept-invitation' }),
  )
})
</script>

<template>
  <main class="auth-layout">
    <section class="auth-brand-panel">
      <BrandMark inverse />
      <div class="auth-brand-panel__body"><p class="eyebrow eyebrow--light">INVITATION ACTIVATION</p><h1>建立受邀账号，<br />进入可信发布流程。</h1><p>邀请令牌仅用于一次激活，不会写入浏览器存储；成功后需在登录页建立独立会话。</p></div>
      <footer><ShieldCheck :size="16" /> 管理后台与公开网站完全隔离</footer>
    </section>
    <section class="auth-form-panel">
      <div v-if="result" class="auth-card" role="status">
        <div class="auth-card__mobile-brand"><BrandMark /></div>
        <p class="eyebrow">ACCOUNT ACTIVE</p><h2>账号已激活</h2>
        <p class="auth-card__lead">{{ result.displayName }}（{{ result.email }}）已加入 {{ result.roleKeys.join(', ') || '后台平台' }}。邀请令牌已从地址栏与页面内存清除。</p>
        <RouterLink class="button button--primary button--wide" to="/login">前往安全登录<ArrowRight :size="17" /></RouterLink>
      </div>
      <form v-else class="auth-card" @submit.prevent="submit">
        <div class="auth-card__mobile-brand"><BrandMark /></div>
        <p class="eyebrow">接受邀请</p><h2>设置后台密码</h2>
        <p class="auth-card__lead">令牌仅保留在当前页面内存中；提交成功后会立即从 URL 清除，且不会自动创建登录会话。</p>
        <div :class="['token-notice', { 'token-notice--danger': !tokenValid }]">
          <span><KeyRound :size="15" /></span><p><strong>{{ tokenValid ? '一次性邀请令牌已读取' : '邀请链接不可用' }}</strong>{{ tokenValid ? '完成密码设置即可激活账号。' : '请使用管理员提供的完整邀请链接。' }}</p>
        </div>
        <label class="field"><span>新密码</span><div class="field__control"><LockKeyhole :size="17" /><input v-model="password" required type="password" autocomplete="new-password" maxlength="256" /></div></label>
        <label class="field"><span>确认密码</span><div class="field__control"><LockKeyhole :size="17" /><input v-model="confirmation" required type="password" autocomplete="new-password" maxlength="256" /></div></label>
        <div class="password-checks"><span v-for="check in passwordChecks" :key="check.label" :class="{ 'is-pass': check.pass }"><Check :size="13" />{{ check.label }}</span></div>
        <p v-if="errorMessage" class="form-error" role="alert">{{ errorMessage }}</p>
        <button class="button button--primary button--wide" type="submit" :disabled="submitting || !tokenValid">{{ submitting ? '正在激活…' : '激活账号' }}<ArrowRight :size="17" /></button>
        <div class="auth-card__footer"><span>已经激活？</span><RouterLink to="/login">返回登录</RouterLink></div>
      </form>
    </section>
  </main>
</template>
