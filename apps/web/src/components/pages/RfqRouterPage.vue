<script setup lang="ts">
import { computed } from 'vue'
import type { PublicPageModel } from '@/types/content'
import { trackAnalyticsEvent } from '@/lib/analytics'
import PublicBlockRenderer from '@/components/blocks/PublicBlockRenderer.vue'

const props = defineProps<{ page: PublicPageModel }>()
const paths = computed(() => (props.page.entries ?? []).flatMap((entry) => {
  const match = entry.href.match(/^\/en\/request-a-quote\/(product|selection|project|replacement)$/u)
  return match ? [{ ...entry, type: match[1] }] : []
}))

function trackRoute(journey: string): void {
  void trackAnalyticsEvent('rfqRouteSelected', { journey })
}
</script>
<template>
  <main id="main-content">
    <PublicBlockRenderer
      v-if="page.projection"
      :blocks="page.projection.composition.blocks"
      :projection="page.projection"
      :breadcrumbs="page.breadcrumbs"
    />
    <section class="section shell rfq-router-grid">
      <a v-for="(path, index) in paths" :key="path.type" :href="path.href" class="rfq-path-card" @click="trackRoute(path.type)">
        <span class="card-number">{{ String(index + 1).padStart(2, '0') }}</span><h2>{{ path.title }}</h2><p v-if="path.summary">{{ path.summary }}</p>
      </a>
    </section>
  </main>
</template>
