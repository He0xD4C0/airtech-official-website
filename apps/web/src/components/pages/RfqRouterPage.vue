<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import type { PublicPageModel } from '@/types/content'
import { trackAnalyticsEvent } from '@/lib/analytics'
defineProps<{ page: PublicPageModel }>()
const paths = [
  { type: 'product', title: 'Known product', summary: 'Begin with an existing published model or a specific product question.', prompt: 'I know the product' },
  { type: 'selection', title: 'Fan selection', summary: 'Begin with airflow, pressure, operating conditions and hard constraints.', prompt: 'I need help selecting' },
  { type: 'project', title: 'Project inquiry', summary: 'Begin with application, project stage, system context and engineering needs.', prompt: 'I have a project' },
  { type: 'replacement', title: 'Replacement', summary: 'Begin with the existing model, installation, duty point and replacement goal.', prompt: 'I need a replacement' },
]

function trackRoute(journey: string): void {
  void trackAnalyticsEvent('rfqRouteSelected', { journey })
}
</script>
<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <section class="section shell rfq-router-grid">
      <a v-for="(path, index) in paths" :key="path.type" :href="`/en/request-a-quote/${path.type}`" class="rfq-path-card" @click="trackRoute(path.type)">
        <span class="card-number">0{{ index + 1 }}</span><h2>{{ path.title }}</h2><p>{{ path.summary }}</p><strong>{{ path.prompt }} <span aria-hidden="true">→</span></strong>
      </a>
    </section>
    <section class="section shell rfq-assurance"><div><h2>What happens to your context</h2><p>Your selected route, source page and submitted business context are stored with the inquiry—not sent as unrestricted analytics properties.</p></div><div><h2>No automatic promises</h2><p>Submission acknowledges receipt only. Suitability, availability, price, lead time and commercial terms require follow-up.</p></div><div><h2>No public uploads</h2><p>Technical files can move through an agreed channel after the inquiry is received.</p></div></section>
  </main>
</template>
