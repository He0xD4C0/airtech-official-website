<script setup lang="ts">
import { computed } from 'vue'
import PageHero from '@/components/common/PageHero.vue'
import type { PublicPageModel } from '@/types/content'
import { trackAnalyticsEvent } from '@/lib/analytics'
import PublishedEditorialSection from '@/components/content/PublishedEditorialSection.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import PageSlotSections from './PageSlotSections.vue'

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
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <PublishedEditorialSection :content="page.publishedContent" />
    <section class="section shell rfq-router-grid">
      <a v-for="(path, index) in paths" :key="path.type" :href="path.href" class="rfq-path-card" @click="trackRoute(path.type)">
        <span class="card-number">{{ String(index + 1).padStart(2, '0') }}</span><h2>{{ path.title }}</h2><p v-if="path.summary">{{ path.summary }}</p>
      </a>
    </section>
    <PageSlotSections :sections="page.sections" />
    <CallToAction
      v-if="page.primaryCta"
      :eyebrow="page.primaryCta.eyebrow"
      :title="page.primaryCta.title"
      :description="page.primaryCta.description"
      :href="page.primaryCta.href"
      :label="page.primaryCta.label"
    />
  </main>
</template>
