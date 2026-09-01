<script setup lang="ts">
import { computed, ref } from 'vue'
import PageHero from '@/components/common/PageHero.vue'
import PublishedEditorialSection from '@/components/content/PublishedEditorialSection.vue'
import { trackAnalyticsEvent } from '@/lib/analytics'
import type { PublicPageModel } from '@/types/content'

const props = defineProps<{ page: PublicPageModel }>()
const query = ref('')
const selectedType = ref('all')
const selectedModel = ref('all')
const entries = computed(() => props.page.entries ?? [])
const resourceTypes = computed(() => [...new Set(entries.value.flatMap((entry) => entry.download?.resourceType ? [entry.download.resourceType] : []))].sort())
const applicableModels = computed(() => [...new Set(entries.value.flatMap((entry) => entry.download?.applicableModels ?? []))].sort())
const results = computed(() => entries.value.filter((entry) => {
  const metadata = entry.download
  const searchText = [
    entry.title, entry.summary, metadata?.resourceType, metadata?.version,
    metadata?.fileDescription, ...(metadata?.applicableModels ?? []),
  ].filter(Boolean).join(' ').toLowerCase()
  const matchesQuery = searchText.includes(query.value.trim().toLowerCase())
  const matchesType = selectedType.value === 'all' || metadata?.resourceType === selectedType.value
  const matchesModel = selectedModel.value === 'all' || metadata?.applicableModels.includes(selectedModel.value)
  return matchesQuery && matchesType && matchesModel
}))

function trackSearch(): void {
  const normalizedQuery = query.value.trim()
  if (!normalizedQuery) return
  void trackAnalyticsEvent('internalSearch', {
    queryLength: normalizedQuery.length,
    resultCount: results.value.length,
  })
}

function trackFilter(filterName: 'resourceType' | 'applicableModel', filterValue: string): void {
  void trackAnalyticsEvent('filterApplied', { filterName, filterValue, resultCount: results.value.length })
}

function clearFilters(): void {
  query.value = ''
  selectedType.value = 'all'
  selectedModel.value = 'all'
}

function safeDownloadRecordPath(href: string): string {
  return /^\/en\/resources\/downloads\/[a-z0-9-]+$/u.test(href)
    ? href
    : '/en/resources/downloads'
}

function trackRecordOpen(href: string): void {
  // This opens a controlled-resource record; it is not evidence that a file download started.
  void trackAnalyticsEvent('ctaClicked', {
    ctaId: 'download-record-open',
    destinationPath: safeDownloadRecordPath(href),
    placement: 'downloads-list',
  })
}

function trackDocumentRequest(): void {
  void trackAnalyticsEvent('ctaClicked', {
    ctaId: 'controlled-document-request',
    destinationPath: '/en/company/contact',
    placement: 'downloads-empty-state',
  })
}
</script>

<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <PublishedEditorialSection :content="page.publishedContent" />
    <section class="section shell">
      <div class="filter-panel downloads-filter-panel">
        <label><span>Published resource</span><input v-model="query" type="search" placeholder="Search title, description, type or model" @change="trackSearch"></label>
        <label><span>Resource type</span><select v-model="selectedType" @change="trackFilter('resourceType', selectedType)"><option value="all">All published types</option><option v-for="type in resourceTypes" :key="type" :value="type">{{ type }}</option></select></label>
        <label><span>Applicable model</span><select v-model="selectedModel" @change="trackFilter('applicableModel', selectedModel)"><option value="all">All published models</option><option v-for="model in applicableModels" :key="model" :value="model">{{ model }}</option></select></label>
        <p class="result-count" role="status">{{ results.length }} published {{ results.length === 1 ? 'record' : 'records' }}</p>
      </div>
      <div v-if="results.length" class="card-grid collection-grid">
        <article v-for="entry in results" :key="entry.href" class="card collection-card">
          <p class="eyebrow">Controlled resource</p>
          <h2><a :href="entry.href" @click="trackRecordOpen(entry.href)">{{ entry.title }}</a></h2>
          <p v-if="entry.summary">{{ entry.summary }}</p>
          <dl v-if="entry.download?.resourceType || entry.download?.version || entry.download?.applicableModels.length" class="download-card-meta">
            <div v-if="entry.download.resourceType"><dt>Type</dt><dd>{{ entry.download.resourceType }}</dd></div>
            <div v-if="entry.download.version"><dt>Version</dt><dd>{{ entry.download.version }}</dd></div>
            <div v-if="entry.download.applicableModels.length"><dt>Models</dt><dd>{{ entry.download.applicableModels.join(', ') }}</dd></div>
          </dl>
          <a class="text-link" :href="entry.href" @click="trackRecordOpen(entry.href)">View published record <span aria-hidden="true">→</span></a>
        </article>
      </div>
      <div v-else-if="page.entries?.length" class="empty-state">
        <h2>No matching published resource</h2>
        <p>Try a broader title, description, type or applicable model.</p>
        <button class="button secondary" type="button" @click="clearFilters">Clear filters</button>
      </div>
      <div v-else class="empty-state large">
        <p class="eyebrow">No approved resources</p>
        <h2>No controlled files are published.</h2>
        <p>Drafts, placeholders and files without a current indexable record are not exposed.</p>
        <a class="button" href="/en/company/contact" @click="trackDocumentRequest">Request a controlled document</a>
      </div>
    </section>
  </main>
</template>
