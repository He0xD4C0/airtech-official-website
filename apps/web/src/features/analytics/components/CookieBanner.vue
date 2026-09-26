<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import {
  analyticsRuntimeDisabled,
  currentAnalyticsConsent,
  setAnalyticsConsent,
  trackCurrentPageView,
} from '@/features/analytics/lib/analytics'
import { useI18n } from 'vue-i18n'

type Consent = 'accepted' | 'declined'
const visible = ref(false)
const { locale, t } = useI18n({ useScope: 'global' })
const current = ref<Consent | null>(null)

function load() {
  if (typeof window === 'undefined') return
  if (analyticsRuntimeDisabled()) {
    visible.value = false
    return
  }
  current.value = currentAnalyticsConsent()
  visible.value = current.value === null
  if (current.value === 'accepted') void trackCurrentPageView()
}

async function save(value: Consent) {
  current.value = value
  visible.value = false
  const recorded = await setAnalyticsConsent(value)
  if (value === 'accepted' && recorded) void trackCurrentPageView()
}

function openSettings() {
  visible.value = true
}

onMounted(() => {
  load()
  window.addEventListener('airtek:open-cookie-settings', openSettings)
})
onUnmounted(() => window.removeEventListener('airtek:open-cookie-settings', openSettings))
</script>

<template>
  <aside v-if="visible" class="cookie-banner" :lang="locale" :aria-label="t('cookie.label')" aria-live="polite">
    <div>
      <strong>{{ t('cookie.title') }}</strong>
      <p>{{ t('cookie.description') }}</p>
    </div>
    <div class="cookie-actions">
      <button class="button secondary" type="button" @click="save('declined')">{{ t('cookie.decline') }}</button>
      <button class="button" type="button" @click="save('accepted')">{{ t('cookie.accept') }}</button>
    </div>
  </aside>
</template>
