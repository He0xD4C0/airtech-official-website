<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, ArrowRight, Eye, EyeOff, KeyRound, LockKeyhole, Mail, ShieldCheck, Smartphone } from 'lucide-vue-next'
import BrandMark from '@/shared/components/BrandMark.vue'
import CaptchaWidget from '@/features/auth/components/CaptchaWidget.vue'
import { adminAuthApi } from '@/shared/services/adminAuthApi'
import { useAuthStore } from '@/shared/stores/auth'
import type { LoginStep } from '@/shared/services/adminApiTypes'
import type { ApiProblem, SessionUser } from '@/shared/types/domain'

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()

type Stage = 'identify' | 'method' | 'code'
const stage = ref<Stage>('identify')
const email = ref('')
const password = ref('')
const passwordVisible = ref(false)
const captchaToken = ref('')
const captchaRequired = ref(false)
const captchaSiteKey = ref('')
const captchaProvider = ref('turnstile')
const flowToken = ref('')
const pendingFactor = ref<string | null>(null)
const code = ref('')
const errorMessage = ref('')
const busy = ref(false)

const codeLabel = computed(() => {
  switch (pendingFactor.value) {
    case 'totp':
      return '验证器 6 位验证码'
    case 'riskSms':
      return '风控短信验证码'
    case 'smsCode':
      return '短信验证码'
    default:
      return '邮箱验证码'
  }
})

function message(error: unknown): string {
  return (error as ApiProblem).detail ?? (error as ApiProblem).title ?? '登录失败，请重试。'
}

async function finish(user: SessionUser): Promise<void> {
  auth.apply(user)
  if (auth.requiresOnboarding) {
    await router.replace('/onboarding')
    return
  }
  const redirect = typeof route.query.redirect === 'string' ? route.query.redirect : '/'
  await router.replace(redirect)
}

async function handleResult(result: LoginStep | SessionUser): Promise<void> {
  if ('status' in result) {
    pendingFactor.value = result.factor ?? null
    code.value = ''
    captchaToken.value = ''
    stage.value = 'code'
    return
  }
  await finish(result)
}

async function submitIdentify(): Promise<void> {
  errorMessage.value = ''
  busy.value = true
  try {
    const identity = await adminAuthApi.identify(email.value.trim())
    flowToken.value = identity.flowToken
    captchaRequired.value = identity.captchaRequired
    captchaSiteKey.value = identity.captchaSiteKey ?? ''
    captchaProvider.value = identity.captchaProvider ?? 'turnstile'
    captchaToken.value = ''
    stage.value = 'method'
  } catch (error) {
    errorMessage.value = message(error)
  } finally {
    busy.value = false
  }
}

async function attempt(method: 'password' | 'emailCode' | 'smsCode'): Promise<void> {
  errorMessage.value = ''
  if (captchaRequired.value && !captchaToken.value) {
    errorMessage.value = '请先完成人机验证。'
    return
  }
  busy.value = true
  try {
    const result = await adminAuthApi.attempt({
      flowToken: flowToken.value,
      method,
      ...(method === 'password' ? { password: password.value } : {}),
      ...(captchaToken.value ? { captchaToken: captchaToken.value } : {}),
    })
    await handleResult(result)
  } catch (error) {
    errorMessage.value = message(error)
  } finally {
    busy.value = false
  }
}

async function submitCode(): Promise<void> {
  errorMessage.value = ''
  busy.value = true
  try {
    const result = await adminAuthApi.verify(flowToken.value, code.value.trim())
    await handleResult(result)
  } catch (error) {
    errorMessage.value = message(error)
  } finally {
    busy.value = false
  }
}

function restart(): void {
  stage.value = 'identify'
  flowToken.value = ''
  pendingFactor.value = null
  code.value = ''
  captchaToken.value = ''
  password.value = ''
  errorMessage.value = ''
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
      <form v-if="stage === 'identify'" class="auth-card" @submit.prevent="submitIdentify">
        <div class="auth-card__mobile-brand"><BrandMark /></div>
        <p class="eyebrow">安全登录</p>
        <h2>欢迎回来</h2>
        <p class="auth-card__lead">第一步：输入工作邮箱。</p>
        <label class="field">
          <span>工作邮箱</span>
          <div class="field__control"><KeyRound :size="17" /><input v-model="email" type="email" autocomplete="username" required /></div>
        </label>
        <p v-if="errorMessage" class="form-error" role="alert">{{ errorMessage }}</p>
        <button class="button button--primary button--wide" type="submit" :disabled="busy">
          {{ busy ? '正在识别…' : '下一步' }}<ArrowRight :size="17" />
        </button>
      </form>

      <form v-else-if="stage === 'method'" class="auth-card" @submit.prevent="attempt('password')">
        <div class="auth-card__mobile-brand"><BrandMark /></div>
        <p class="eyebrow">选择登录方式</p>
        <h2>{{ email }}</h2>
        <button class="button button--secondary" type="button" @click="restart"><ArrowLeft :size="15" />换个账号</button>

        <CaptchaWidget
          v-if="captchaRequired"
          :provider="captchaProvider"
          :site-key="captchaSiteKey"
          @update:token="captchaToken = $event"
        />

        <label class="field">
          <span>密码</span>
          <div class="field__control">
            <LockKeyhole :size="17" />
            <input v-model="password" :type="passwordVisible ? 'text' : 'password'" aria-label="登录密码" autocomplete="current-password" />
            <button type="button" :aria-label="passwordVisible ? '隐藏密码' : '显示密码'" @click="passwordVisible = !passwordVisible">
              <EyeOff v-if="passwordVisible" :size="17" /><Eye v-else :size="17" />
            </button>
          </div>
        </label>
        <button class="button button--primary button--wide" type="submit" :disabled="busy || !password">
          {{ busy ? '正在验证…' : '使用密码登录' }}
        </button>
        <div class="auth-card__footer">
          <button class="button button--secondary" type="button" :disabled="busy" @click="attempt('emailCode')"><Mail :size="15" />发送邮箱验证码</button>
          <button class="button button--secondary" type="button" :disabled="busy" @click="attempt('smsCode')"><Smartphone :size="15" />发送短信验证码</button>
        </div>
        <p class="inline-note">若该账号未绑定手机，短信验证码不会发送。</p>
        <p v-if="errorMessage" class="form-error" role="alert">{{ errorMessage }}</p>
      </form>

      <form v-else class="auth-card" @submit.prevent="submitCode">
        <div class="auth-card__mobile-brand"><BrandMark /></div>
        <p class="eyebrow">多因素验证</p>
        <h2>{{ codeLabel }}</h2>
        <label class="field">
          <span>{{ codeLabel }}</span>
          <div class="field__control"><ShieldCheck :size="17" /><input v-model="code" autocomplete="one-time-code" maxlength="19" required /></div>
        </label>
        <p v-if="errorMessage" class="form-error" role="alert">{{ errorMessage }}</p>
        <button class="button button--primary button--wide" type="submit" :disabled="busy || !code">
          {{ busy ? '正在验证…' : '继续' }}
        </button>
        <button class="button button--secondary" type="button" @click="restart">重新开始</button>
      </form>
    </section>
  </main>
</template>
