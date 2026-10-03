<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'

const props = defineProps<{
  provider: string
  siteKey: string
}>()

const emit = defineEmits<{ 'update:token': [value: string] }>()
const container = ref<HTMLElement | null>(null)
const widgetId = ref<string | number | null>(null)

interface CaptchaRenderer {
  render: (element: HTMLElement, options: Record<string, unknown>) => string | number
}

declare global {
  interface Window {
    turnstile?: CaptchaRenderer
    grecaptcha?: CaptchaRenderer
    hcaptcha?: CaptchaRenderer
  }
}

const scripts: Record<string, string> = {
  turnstile: 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit',
  recaptcha: 'https://www.google.com/recaptcha/api.js?render=explicit',
  hcaptcha: 'https://js.hcaptcha.com/1/api.js?render=explicit',
}

function api(name: string): CaptchaRenderer | undefined {
  switch (name) {
    case 'turnstile':
      return window.turnstile
    case 'recaptcha':
      return window.grecaptcha
    case 'hcaptcha':
      return window.hcaptcha
    default:
      return undefined
  }
}

function loadScript(source: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const existing = document.querySelector<HTMLScriptElement>(`script[src="${source}"]`)
    if (existing) {
      if (existing.dataset.loaded === 'true') resolve()
      else {
        existing.addEventListener('load', () => resolve())
        existing.addEventListener('error', () => reject(new Error('CAPTCHA script failed to load')))
      }
      return
    }
    const script = document.createElement('script')
    script.src = source
    script.async = true
    script.defer = true
    script.addEventListener('load', () => {
      script.dataset.loaded = 'true'
      resolve()
    })
    script.addEventListener('error', () => reject(new Error('CAPTCHA script failed to load')))
    document.head.append(script)
  })
}

async function render(): Promise<void> {
  if (!container.value || !props.siteKey) return
  const source = scripts[props.provider]
  if (!source) return
  await loadScript(source)
  const globalName = props.provider === 'recaptcha' ? 'grecaptcha' : props.provider
  const target = api(props.provider === 'recaptcha' ? 'recaptcha' : globalName)
  const element = container.value
  if (!target || !element) return
  element.replaceChildren()
  widgetId.value = target.render(element, {
    sitekey: props.siteKey,
    callback: (token: string) => emit('update:token', token),
    'expired-callback': () => emit('update:token', ''),
    'error-callback': () => emit('update:token', ''),
  })
}

onMounted(() => {
  void render()
})

watch(() => [props.provider, props.siteKey], () => {
  void render()
})

onBeforeUnmount(() => {
  widgetId.value = null
})
</script>

<template>
  <div ref="container" class="captcha-widget" />
</template>
