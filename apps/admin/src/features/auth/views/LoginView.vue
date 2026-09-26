<script setup lang="ts">
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowRight, Eye, EyeOff, KeyRound, LockKeyhole, ShieldCheck } from 'lucide-vue-next'
import BrandMark from '@/shared/components/BrandMark.vue'
import { useAuthStore } from '@/shared/stores/auth'
import type { ApiProblem } from '@/shared/types/domain'

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const email = ref('')
const password = ref('')
const otp = ref('')
const passwordVisible = ref(false)
const errorMessage = ref('')

async function submit(): Promise<void> {
  errorMessage.value = ''
  try {
    await auth.login(email.value.trim(), password.value, otp.value || undefined)
    if (auth.requiresTotpEnrollment) {
      await router.replace('/account/security')
      return
    }
    const redirect = typeof route.query.redirect === 'string' ? route.query.redirect : '/'
    await router.replace(redirect)
  } catch (error) {
    errorMessage.value = (error as ApiProblem).detail ?? (error as ApiProblem).title ?? '登录失败，请重试。'
  }
}
</script>

<template>
  <main class="auth-layout">
    <section class="auth-brand-panel">
      <BrandMark vertical />
      <div class="auth-brand-panel__body">
        <p class="eyebrow eyebrow--light">AIRTEKPOWER CONTROL CENTER</p>
        <h1>把复杂的产品信息，<br />变成可信的发布流程。</h1>
        <p>管理内容、产品主数据、客户询盘与网站表现。每次变更都有版本，每个公开事实都可追溯。</p>
      </div>
      <div class="auth-brand-panel__signal" aria-hidden="true">
        <span></span><span></span><span></span><span></span><span></span>
      </div>
      <footer><ShieldCheck :size="16" /> 管理后台与公开网站完全隔离</footer>
    </section>

    <section class="auth-form-panel">
      <form class="auth-card" @submit.prevent="submit">
        <div class="auth-card__mobile-brand"><BrandMark /></div>
        <p class="eyebrow">安全登录</p>
        <h2>欢迎回来</h2>
        <p class="auth-card__lead">使用受邀账号登录 AIRTEKPOWER 管理平台。</p>

        <label class="field">
          <span>工作邮箱</span>
          <div class="field__control"><KeyRound :size="17" /><input v-model="email" type="email" autocomplete="username" required /></div>
        </label>
        <label class="field">
          <span>密码</span>
          <div class="field__control">
            <LockKeyhole :size="17" />
            <input v-model="password" :type="passwordVisible ? 'text' : 'password'" autocomplete="current-password" required />
            <button type="button" :aria-label="passwordVisible ? '隐藏密码' : '显示密码'" @click="passwordVisible = !passwordVisible">
              <EyeOff v-if="passwordVisible" :size="17" /><Eye v-else :size="17" />
            </button>
          </div>
        </label>
        <label class="field">
          <span>TOTP 或恢复码 <small>（已启用多因素验证时填写）</small></span>
          <div class="field__control"><ShieldCheck :size="17" /><input v-model="otp" autocomplete="one-time-code" maxlength="19" placeholder="6 位验证码或恢复码" /></div>
        </label>

        <p v-if="errorMessage" class="form-error" role="alert">{{ errorMessage }}</p>
        <button class="button button--primary button--wide" type="submit" :disabled="auth.loading">
          {{ auth.loading ? '正在验证…' : '进入管理平台' }}<ArrowRight :size="17" />
        </button>

        <div class="auth-card__footer">
          <span>首次部署？</span><RouterLink to="/setup">初始化超级管理员</RouterLink>
        </div>
      </form>
    </section>
  </main>
</template>
