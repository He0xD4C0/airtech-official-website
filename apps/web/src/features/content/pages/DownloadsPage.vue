<script setup lang="ts">
import { computed, ref } from 'vue'
import PublicBlockRenderer from '@/features/content/components/blocks/PublicBlockRenderer.vue'
import { trackAnalyticsEvent } from '@/features/analytics'
import type { PublicPageModel } from '@/shared/types/content'

const props = defineProps<{ page: PublicPageModel }>()
const query = ref('')
const entries = computed(() => props.page.entries ?? [])
const results = computed(() => entries.value.filter((entry) => {
  const searchText = `${entry.title} ${entry.summary}`.toLowerCase()
  return searchText.includes(query.value.trim().toLowerCase())
}))

function trackSearch(): void {
  const normalizedQuery = query.value.trim()
  if (!normalizedQuery) return
  void trackAnalyticsEvent('internalSearch', {
    queryLength: normalizedQuery.length,
    resultCount: results.value.length,
  })
}

function clearFilters(): void {
  query.value = ''
}

function trackRecordOpen(): void {
  // This opens a controlled-resource record; it is not evidence that a file download started.
  void trackAnalyticsEvent('ctaClicked', {
    ctaId: 'download-record-open',
    placement: 'downloads-list',
  })
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
    <section class="section shell">
      <div class="filter-panel downloads-filter-panel">
        <label><span>Published resource</span><input v-model="query" type="search" placeholder="Search title or description" @change="trackSearch"></label>
        <p class="result-count" role="status">{{ results.length }} published {{ results.length === 1 ? 'record' : 'records' }}</p>
      </div>
      <div v-if="results.length" class="card-grid collection-grid">
        <article v-for="entry in results" :key="entry.href" class="card collection-card">
          <p class="eyebrow">Controlled resource</p>
          <h2><a :href="entry.href" @click="trackRecordOpen">{{ entry.title }}</a></h2>
          <p v-if="entry.summary">{{ entry.summary }}</p>
          <a class="text-link" :href="entry.href" @click="trackRecordOpen">View published record <span aria-hidden="true">→</span></a>
        </article>
      </div>
      <div v-else-if="page.entries?.length" class="empty-state">
        <h2>No matching published resource</h2>
        <p>Try a broader title or description.</p>
        <button class="button secondary" type="button" @click="clearFilters">Clear filters</button>
      </div>
      <div v-else class="empty-state large">
        <p class="eyebrow">No approved resources</p>
        <h2>No controlled files are published.</h2>
        <p>Drafts, placeholders and files without a current indexable record are not exposed.</p>
      </div>
    </section>
  </main>
</template>
