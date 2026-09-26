<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import {
  analyticsRuntimeDisabled,
  currentAnalyticsConsent,
  setAnalyticsConsent,
  trackCurrentPageView,
} from '@/features/analytics/lib/analytics'

type Consent = 'accepted' | 'declined'
const visible = ref(false)
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
  <aside v-if="visible" class="cookie-banner" aria-label="Analytics cookie preferences" aria-live="polite">
    <div>
      <strong>Privacy choices</strong>
      <p>Essential storage keeps this preference. Optional analytics stays off unless you accept it.</p>
    </div>
    <div class="cookie-actions">
      <button class="button secondary" type="button" @click="save('declined')">Keep analytics off</button>
      <button class="button" type="button" @click="save('accepted')">Allow analytics</button>
    </div>
  </aside>
</template>
