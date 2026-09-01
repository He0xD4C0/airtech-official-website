<script setup lang="ts">
import { computed, ref } from 'vue'
import PageHero from '@/components/common/PageHero.vue'
import { trackAnalyticsEvent } from '@/lib/analytics'
import type { PublicPageModel } from '@/types/content'
const props = defineProps<{ page: PublicPageModel }>()
const query = ref('')
const type = ref('all')
const entries = computed(() => (props.page.entries ?? []).map((entry) => ({ ...entry, type: entry.eyebrow || 'Page' })))
const types = computed(() => [...new Set(entries.value.map((entry) => entry.type))].sort())
const results = computed(() => entries.value.filter((entry) => {
  const matchesQuery = `${entry.title} ${entry.summary}`.toLowerCase().includes(query.value.trim().toLowerCase())
  return matchesQuery && (type.value === 'all' || entry.type === type.value)
}))

function trackSearch(): void {
  const normalizedQuery = query.value.trim()
  if (!normalizedQuery) return
  void trackAnalyticsEvent('internalSearch', {
    queryLength: normalizedQuery.length,
    resultCount: results.value.length,
  })
}

function trackTypeFilter(): void {
  void trackAnalyticsEvent('filterApplied', {
    filterName: 'contentType',
    filterValue: type.value,
    resultCount: results.value.length,
  })
}
</script>
<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" compact />
    <section class="section shell search-layout"><div class="search-controls"><label><span>Search the public site</span><input v-model="query" autofocus type="search" placeholder="Search products, solutions or resources" @change="trackSearch"></label><label><span>Content type</span><select v-model="type" @change="trackTypeFilter"><option value="all">All published content</option><option v-for="entryType in types" :key="entryType" :value="entryType">{{ entryType }}</option></select></label></div><p class="result-count" role="status">{{ results.length }} results</p><div v-if="results.length" class="search-results"><article v-for="entry in results" :key="entry.href"><span>{{ entry.type }}</span><h2><a :href="entry.href">{{ entry.title }}</a></h2><p>{{ entry.summary }}</p></article></div><div v-else class="empty-state"><h2>No matching published content</h2><p>Search results only include the current public projection.</p></div></section>
  </main>
</template>
