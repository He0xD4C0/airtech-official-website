<script setup lang="ts">
import { computed, ref } from 'vue'
import type { PublicSearchItem, PublicSearchType } from '@airtek/contracts'
import PublicBlockRenderer from '@/components/blocks/PublicBlockRenderer.vue'
import { useQueryHistory } from '@/lib/queryHistory'
import { trackAnalyticsEvent } from '@/lib/analytics'
import { searchPublishedSite } from '@/lib/api'
import type { PublicPageModel } from '@/types/content'

const props = defineProps<{ page: PublicPageModel }>()
const query = ref(props.page.searchState?.q ?? '')
const type = ref<PublicSearchType | 'all'>(props.page.searchState?.type ?? 'all')
const items = ref<PublicSearchItem[]>([...(props.page.searchResults ?? [])])
const nextCursor = ref<string | null>(props.page.searchNextCursor ?? null)
const total = ref(props.page.searchTotal ?? 0)
const typeCounts = ref([...(props.page.searchTypeCounts ?? [])])
const cursorHistory = ref<Array<string | null>>([props.page.searchState?.cursor ?? null])
const pageIndex = ref(0)
const loading = ref(false)
const error = ref('')
let requestGeneration = 0

const typeLabels: Record<PublicSearchType, string> = {
  product: 'Product', solution: 'Solution', technology: 'Technology', article: 'Article',
  news: 'News', faq: 'FAQ', caseStudy: 'Case study', download: 'Download', company: 'Company', page: 'Page',
}
const availableTypes = computed(() => typeCounts.value.map((facet) => ({
  value: facet.value as PublicSearchType,
  label: typeLabels[facet.value as PublicSearchType] ?? facet.value,
  count: facet.count,
})))

const writeQuery = useQueryHistory((params, cursors) => {
  query.value = params.get('q') ?? ''
  const requestedType = params.get('type')
  type.value = requestedType && requestedType in typeLabels ? requestedType as PublicSearchType : 'all'
  cursorHistory.value = cursors
  pageIndex.value = Math.max(0, cursors.indexOf(params.get('cursor')))
  void loadPage(params.get('cursor'))
})

function syncUrl(): void {
  writeQuery({
    q: query.value.trim(),
    type: type.value === 'all' ? undefined : type.value,
    cursor: cursorHistory.value[pageIndex.value] ?? undefined,
  }, cursorHistory.value)
}

function requestQuery(cursor: string | null) {
  return {
    limit: 20,
    ...(query.value.trim() ? { q: query.value.trim() } : {}),
    ...(type.value !== 'all' ? { type: type.value } : {}),
    ...(cursor ? { cursor } : {}),
  }
}

async function loadPage(cursor: string | null): Promise<boolean> {
  const generation = ++requestGeneration
  loading.value = true
  error.value = ''
  try {
    const page = await searchPublishedSite(requestQuery(cursor))
    if (generation !== requestGeneration) return false
    items.value = page.items
    nextCursor.value = page.nextCursor
    total.value = page.total
    typeCounts.value = page.typeCounts
    return true
  } catch (cause) {
    if (generation === requestGeneration) {
      error.value = cause instanceof Error ? cause.message : 'Published search is temporarily unavailable.'
    }
    return false
  } finally {
    if (generation === requestGeneration) loading.value = false
  }
}

async function reset(kind: 'query' | 'type'): Promise<void> {
  cursorHistory.value = [null]
  pageIndex.value = 0
  nextCursor.value = null
  syncUrl()
  await loadPage(null)
  if (kind === 'query' && query.value.trim()) {
    void trackAnalyticsEvent('internalSearch', { queryLength: query.value.trim().length, resultCount: total.value })
  } else if (kind === 'type') {
    void trackAnalyticsEvent('filterApplied', { filterName: 'contentType', resultCount: total.value })
  }
}

async function nextPage(): Promise<void> {
  const cursor = nextCursor.value
  if (!cursor || loading.value) return
  if (await loadPage(cursor)) {
    cursorHistory.value = [...cursorHistory.value.slice(0, pageIndex.value + 1), cursor]
    pageIndex.value += 1
    syncUrl()
  }
}

async function previousPage(): Promise<void> {
  if (!pageIndex.value || loading.value) return
  const target = pageIndex.value - 1
  if (await loadPage(cursorHistory.value[target] ?? null)) {
    pageIndex.value = target
    syncUrl()
  }
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
    <section class="section shell search-layout">
      <div class="search-controls" :aria-busy="loading">
        <label><span>Search the public site</span><input v-model="query" autofocus type="search" placeholder="Search published products and content" @change="reset('query')"></label>
        <label><span>Content type</span><select v-model="type" @change="reset('type')">
          <option value="all">All published content</option>
          <option v-for="entryType in availableTypes" :key="entryType.value" :value="entryType.value">{{ entryType.label }} ({{ entryType.count }})</option>
        </select></label>
      </div>
      <p class="result-count" role="status">{{ total }} {{ total === 1 ? 'result' : 'results' }}</p>
      <p v-if="error" class="form-error" role="alert">{{ error }}</p>
      <div v-if="items.length" class="search-results">
        <article v-for="entry in items" :key="`${entry.entityType}:${entry.entityId}`">
          <span>{{ typeLabels[entry.displayType] }}</span>
          <h2><a :href="entry.canonicalPath">{{ entry.title }}</a></h2>
          <p>{{ entry.summary || 'Summary not published.' }}</p>
        </article>
      </div>
      <div v-else class="empty-state"><h2>No matching published content</h2><p>Search results include only canonical, indexable records in the current public projection.</p></div>
      <nav v-if="pageIndex > 0 || nextCursor" class="catalog-pagination" aria-label="Search result pages">
        <button class="button secondary" type="button" :disabled="pageIndex === 0 || loading" @click="previousPage">Previous page</button>
        <span>Page {{ pageIndex + 1 }}<small>{{ items.length }} of {{ total }}</small></span>
        <button class="button secondary" type="button" :disabled="!nextCursor || loading" @click="nextPage">Next page</button>
      </nav>
      <p v-if="loading" role="status">Searching published records…</p>
    </section>
  </main>
</template>
