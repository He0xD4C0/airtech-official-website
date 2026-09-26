<script setup lang="ts">
import { computed, ref } from 'vue'
import PublicBlockRenderer from '@/features/content/components/blocks/PublicBlockRenderer.vue'
import { trackAnalyticsEvent } from '@/features/analytics'
import type { PublicPageModel } from '@/shared/types/content'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ page: PublicPageModel }>()
const { locale, t } = useI18n({ useScope: 'global' })
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
    <section class="section shell" :lang="locale">
      <div class="filter-panel downloads-filter-panel">
        <label><span>{{ t('content.resource') }}</span><input v-model="query" type="search" :placeholder="t('content.resourcePlaceholder')" @change="trackSearch"></label>
        <p class="result-count" role="status">{{ t('content.records', { count: results.length }, results.length) }}</p>
      </div>
      <div v-if="results.length" class="card-grid collection-grid">
        <article v-for="entry in results" :key="entry.href" class="card collection-card">
          <p class="eyebrow">{{ t('content.controlledResource') }}</p>
          <h2 lang="en"><a :href="entry.href" @click="trackRecordOpen">{{ entry.title }}</a></h2>
          <p v-if="entry.summary" lang="en">{{ entry.summary }}</p>
          <a class="text-link" :href="entry.href" @click="trackRecordOpen">{{ t('content.viewRecord') }} <span aria-hidden="true">→</span></a>
        </article>
      </div>
      <div v-else-if="page.entries?.length" class="empty-state">
        <h2>{{ t('content.noResource') }}</h2>
        <p>{{ t('content.broaderResource') }}</p>
        <button class="button secondary" type="button" @click="clearFilters">{{ t('catalog.clearFilters') }}</button>
      </div>
      <div v-else class="empty-state large">
        <p class="eyebrow">{{ t('content.noApproved') }}</p>
        <h2>{{ t('content.noFiles') }}</h2>
        <p>{{ t('content.unpublishedFilesExcluded') }}</p>
      </div>
    </section>
  </main>
</template>
