<script setup lang="ts">
import { trackAnalyticsEvent } from '@/lib/analytics'

defineProps<{
  eyebrow?: string
  title: string
  description: string
  href: string
  label: string
}>()

function trackClick(): void {
  void trackAnalyticsEvent('ctaClicked', {
    ctaId: 'content-primary',
    placement: 'content-panel',
  })
}
</script>

<template>
  <section class="cta-panel shell">
    <div>
      <p v-if="eyebrow" class="eyebrow light">{{ eyebrow }}</p>
      <h2>{{ title }}</h2>
      <p>{{ description }}</p>
    </div>
    <a class="button inverted" :href="href" @click="trackClick">{{ label }}</a>
  </section>
</template>
