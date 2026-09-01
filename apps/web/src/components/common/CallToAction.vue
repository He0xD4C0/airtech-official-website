<script setup lang="ts">
import { trackAnalyticsEvent } from '@/lib/analytics'

const props = withDefaults(defineProps<{
  eyebrow?: string
  title: string
  description: string
  href?: string
  label?: string
}>(), {
  eyebrow: 'Engineering handoff',
  href: '/en/request-a-quote',
  label: 'Start a request',
})

function trackClick(): void {
  void trackAnalyticsEvent('ctaClicked', {
    ctaId: props.label.toLowerCase().replace(/[^a-z0-9]+/gu, '-').replace(/(^-|-$)/gu, ''),
    destinationPath: props.href,
    placement: 'content-panel',
  })
}
</script>

<template>
  <section class="cta-panel shell">
    <div>
      <p class="eyebrow light">{{ eyebrow }}</p>
      <h2>{{ title }}</h2>
      <p>{{ description }}</p>
    </div>
    <a class="button inverted" :href="href" @click="trackClick">{{ label }}</a>
  </section>
</template>
