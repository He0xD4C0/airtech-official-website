<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowLeft, Check, KeyRound, LockKeyhole, ShieldCheck, UserRound } from 'lucide-vue-next'
import BrandMark from '@/shared/components/BrandMark.vue'
import { useAuthStore } from '@/shared/stores/auth'
import type { ApiProblem } from '@/shared/types/domain'

const router = useRouter()
const auth = useAuthStore()
const displayName = ref('')
const email = ref('')
const password = ref('')
const confirmation = ref('')
const bootstrapToken = ref('')
const errorMessage = ref('')

const passwordChecks = computed(() => [
  { label: '至少 12 个字符', pass: password.value.length >= 12 },
  { label: '包含字母与数字', pass: /[A-Za-z]/.test(password.value) && /\d/.test(password.value) },
  { label: '两次输入一致', pass: password.value.length > 0 && password.value === confirmation.value },
])

async function submit(): Promise<void> {
  errorMessage.value = ''
  if (passwordChecks.value.some((check) => !check.pass)) {
    errorMessage.value = '请完成全部密码安全要求。'
    return
  }

  try {
    await auth.setup(displayName.value.trim(), email.value.trim(), password.value, bootstrapToken.value.trim())
    await router.replace('/account/security')
  } catch (error) {
    errorMessage.value = (error as ApiProblem).detail ?? (error as ApiProblem).title ?? '初始化失败。'
  }
}
</script>

<template>
  <main class="setup-layout">
    <header class="setup-topbar">
      <BrandMark />
      <RouterLink to="/login"><ArrowLeft :size="15" />返回登录</RouterLink>
    </header>
    <section class="setup-grid">
      <div class="setup-copy">
        <p class="eyebrow">首次部署 · ACCOUNT SETUP</p>
        <h1>建立第一个<br />超级管理员</h1>
        <p>此入口只在用户表为空时有效。初始化完成后，Bootstrap Token 将立即失效。</p>
        <ol>
          <li><span>1</span><div><strong>创建本地管理员</strong><p>邀请制账号，不依赖外部身份供应商。</p></div></li>
          <li><span>2</span><div><strong>启用多因素验证</strong><p>创建账号后必须绑定 TOTP 并保存一次性恢复码，之后才能访问业务功能。</p></div></li>
          <li><span>3</span><div><strong>留下审计记录</strong><p>首次初始化也会写入不可变审计日志。</p></div></li>
        </ol>
      </div>

      <form class="setup-card" @submit.prevent="submit">
        <div class="setup-card__title"><span><ShieldCheck :size="20" /></span><div><h2>账号信息</h2><p>所有字段都必须填写</p></div></div>
        <label class="field"><span>显示名称</span><div class="field__control"><UserRound :size="17" /><input v-model="displayName" required autocomplete="name" placeholder="例如：网站管理员" /></div></label>
        <label class="field"><span>工作邮箱</span><div class="field__control"><KeyRound :size="17" /><input v-model="email" required type="email" autocomplete="email" placeholder="name@company.com" /></div></label>
        <label class="field"><span>密码</span><div class="field__control"><LockKeyhole :size="17" /><input v-model="password" required type="password" autocomplete="new-password" /></div></label>
        <label class="field"><span>确认密码</span><div class="field__control"><LockKeyhole :size="17" /><input v-model="confirmation" required type="password" autocomplete="new-password" /></div></label>
        <div class="password-checks">
          <span v-for="check in passwordChecks" :key="check.label" :class="{ 'is-pass': check.pass }"><Check :size="13" />{{ check.label }}</span>
        </div>
        <label class="field"><span>一次性 Bootstrap Token</span><div class="field__control"><KeyRound :size="17" /><input v-model="bootstrapToken" required type="password" autocomplete="off" /></div></label>
        <p v-if="errorMessage" class="form-error" role="alert">{{ errorMessage }}</p>
        <button class="button button--primary button--wide" type="submit" :disabled="auth.loading">{{ auth.loading ? '正在初始化…' : '创建管理员' }}</button>
      </form>
    </section>
  </main>
</template>
